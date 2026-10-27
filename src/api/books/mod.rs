//! Opt-in operational books. Amounts are scaled integers, never floating point.
//! Every command, its immutable evidence and its retry receipt share one transaction.
use crate::{
    api::{
        modules::module_can_for_user_initialized as can,
        mx::privacy,
        operations::{self, ClientOperation},
    },
    db::connector::{SqliteDatabaseError, with_sql_connection, with_sql_write},
    middleware::auth::{Claims, auth},
};
use axum::{
    Json, Router,
    extract::{Path, Query},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
mod journal;
mod stock;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub accounting: bool,
    pub inventory: bool,
    pub currency: String,
    pub money_decimals: u32,
    pub stock_decimals: u32,
    pub operator_levels: Vec<i64>,
    pub closed_through: String,
    pub required_stage: String,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            accounting: false,
            inventory: false,
            currency: String::new(),
            money_decimals: 2,
            stock_decimals: 3,
            operator_levels: vec![0],
            closed_through: String::new(),
            required_stage: String::new(),
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Configuration {
    revision: i64,
    settings: Settings,
}
#[derive(Clone, Serialize, Deserialize)]
struct Command {
    operation_uid: String,
    configuration_revision: i64,
    #[serde(flatten)]
    action: Action,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Action {
    Account {
        code: String,
        name: String,
        category: String,
    },
    Item {
        code: String,
        name: String,
        unit: String,
    },
    Journal {
        date: String,
        description: String,
        source: Option<Source>,
        lines: Vec<journal::Line>,
    },
    Reverse {
        journal_uid: String,
        date: String,
        reason: String,
    },
    Stock {
        item_uid: String,
        movement: String,
        quantity: String,
        note: String,
        source: Option<Source>,
    },
    Reservation {
        reservation_uid: String,
        action: String,
        note: String,
    },
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    record_uid: String,
    record_revision: i64,
}
#[derive(Default, Deserialize)]
struct Page {
    after: Option<i64>,
}
pub(super) fn fail(code: &str, message: impl AsRef<str>) -> rusqlite::Error {
    rusqlite::Error::InvalidParameterName(format!("MX_BOOK_{code}:{}", message.as_ref()))
}
pub(crate) fn ensure_schema(c: &Connection) -> rusqlite::Result<()> {
    if crate::db::schema::initialized(c) {
        return Ok(());
    }
    c.execute_batch("CREATE TABLE IF NOT EXISTS mx_books(module_uid TEXT PRIMARY KEY REFERENCES mx_modules(uid) ON DELETE CASCADE, revision INTEGER NOT NULL, settings_json TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS mx_book_audit(sequence INTEGER PRIMARY KEY AUTOINCREMENT,module_uid TEXT NOT NULL REFERENCES mx_modules(uid) ON DELETE RESTRICT,actor_uid TEXT NOT NULL,event TEXT NOT NULL,evidence_json TEXT NOT NULL,created_at INTEGER NOT NULL);
      CREATE INDEX IF NOT EXISTS mx_book_audit_page ON mx_book_audit(module_uid,sequence);
      CREATE TABLE IF NOT EXISTS mx_book_accounts(sequence INTEGER PRIMARY KEY AUTOINCREMENT,uid TEXT UNIQUE NOT NULL,module_uid TEXT NOT NULL REFERENCES mx_modules(uid) ON DELETE CASCADE,code TEXT NOT NULL,name TEXT NOT NULL,category TEXT NOT NULL,debit INTEGER NOT NULL DEFAULT 0 CHECK(debit>=0),credit INTEGER NOT NULL DEFAULT 0 CHECK(credit>=0),UNIQUE(module_uid,code));
      CREATE INDEX IF NOT EXISTS mx_book_accounts_page ON mx_book_accounts(module_uid,sequence);
      CREATE TABLE IF NOT EXISTS mx_book_journals(sequence INTEGER PRIMARY KEY AUTOINCREMENT,uid TEXT UNIQUE NOT NULL,module_uid TEXT NOT NULL REFERENCES mx_modules(uid) ON DELETE RESTRICT,date TEXT NOT NULL,description TEXT NOT NULL,actor_uid TEXT NOT NULL,source_uid TEXT,snapshot_uid TEXT,reversal_of TEXT UNIQUE REFERENCES mx_book_journals(uid),created_at INTEGER NOT NULL);
      CREATE INDEX IF NOT EXISTS mx_book_journals_page ON mx_book_journals(module_uid,sequence);
      CREATE TABLE IF NOT EXISTS mx_book_lines(journal_uid TEXT NOT NULL REFERENCES mx_book_journals(uid) ON DELETE RESTRICT,position INTEGER NOT NULL,account_uid TEXT NOT NULL REFERENCES mx_book_accounts(uid) ON DELETE RESTRICT,account_code TEXT NOT NULL,account_name TEXT NOT NULL,debit INTEGER NOT NULL CHECK(debit>=0),credit INTEGER NOT NULL CHECK(credit>=0),CHECK((debit>0 AND credit=0) OR (credit>0 AND debit=0)),PRIMARY KEY(journal_uid,position));
      CREATE TABLE IF NOT EXISTS mx_book_items(sequence INTEGER PRIMARY KEY AUTOINCREMENT,uid TEXT UNIQUE NOT NULL,module_uid TEXT NOT NULL REFERENCES mx_modules(uid) ON DELETE CASCADE,code TEXT NOT NULL,name TEXT NOT NULL,unit TEXT NOT NULL,on_hand INTEGER NOT NULL DEFAULT 0,reserved INTEGER NOT NULL DEFAULT 0,CHECK(on_hand>=0 AND reserved>=0 AND reserved<=on_hand),UNIQUE(module_uid,code));
      CREATE INDEX IF NOT EXISTS mx_book_items_page ON mx_book_items(module_uid,sequence);
      CREATE TABLE IF NOT EXISTS mx_book_reservations(sequence INTEGER PRIMARY KEY AUTOINCREMENT,uid TEXT UNIQUE NOT NULL,module_uid TEXT NOT NULL REFERENCES mx_modules(uid) ON DELETE RESTRICT,item_uid TEXT NOT NULL REFERENCES mx_book_items(uid) ON DELETE RESTRICT,quantity INTEGER NOT NULL CHECK(quantity>0),status TEXT NOT NULL CHECK(status IN ('held','released','consumed')),note TEXT NOT NULL,actor_uid TEXT NOT NULL,created_at INTEGER NOT NULL);
      CREATE INDEX IF NOT EXISTS mx_book_reservations_page ON mx_book_reservations(module_uid,sequence);
      CREATE TABLE IF NOT EXISTS mx_book_stock_events(sequence INTEGER PRIMARY KEY AUTOINCREMENT,uid TEXT UNIQUE NOT NULL,module_uid TEXT NOT NULL REFERENCES mx_modules(uid) ON DELETE RESTRICT,item_uid TEXT NOT NULL REFERENCES mx_book_items(uid) ON DELETE RESTRICT,movement TEXT NOT NULL,quantity INTEGER NOT NULL CHECK(quantity>0),on_hand INTEGER NOT NULL,reserved INTEGER NOT NULL,note TEXT NOT NULL,actor_uid TEXT NOT NULL,source_uid TEXT,snapshot_uid TEXT,reservation_uid TEXT REFERENCES mx_book_reservations(uid),created_at INTEGER NOT NULL);
      CREATE INDEX IF NOT EXISTS mx_book_stock_page ON mx_book_stock_events(module_uid,sequence);")?;
    // Defence in depth: no application route (present or future) may rewrite evidence.
    for table in [
        "mx_book_journals",
        "mx_book_lines",
        "mx_book_stock_events",
        "mx_book_audit",
    ] {
        for action in ["UPDATE", "DELETE"] {
            c.execute_batch(&format!("CREATE TRIGGER IF NOT EXISTS {table}_no_{action} BEFORE {action} ON {table} BEGIN SELECT RAISE(ABORT,'Operational book evidence is immutable; use a compensating entry'); END;"))?;
        }
    }
    Ok(())
}
fn load(c: &Connection, m: &str) -> rusqlite::Result<Configuration> {
    c.query_row(
        "SELECT revision,settings_json FROM mx_books WHERE module_uid=?1",
        [m],
        |r| {
            let raw: String = r.get(1)?;
            Ok(Configuration {
                revision: r.get(0)?,
                settings: serde_json::from_str(&raw).map_err(|_| {
                    fail(
                        "INVALID",
                        "Stored book settings need administrator attention.",
                    )
                })?,
            })
        },
    )
    .optional()
    .map(|v| {
        v.unwrap_or(Configuration {
            revision: 0,
            settings: Settings::default(),
        })
    })
}
fn authorize(c: &Connection, m: &str, a: &Claims, configure: bool) -> rusqlite::Result<()> {
    if !can(
        c,
        m,
        &a.uid,
        a.access_level,
        if configure { "configure" } else { "read" },
    )? {
        return Err(fail(
            "FORBIDDEN",
            "Your account does not have access to this module's books.",
        ));
    }
    Ok(())
}
fn operator(c: &Connection, m: &str, a: &Claims, s: &Settings) -> rusqlite::Result<()> {
    authorize(c, m, a, false)?;
    if !s.operator_levels.contains(&a.access_level)
        || !can(c, m, &a.uid, a.access_level, "update")?
        || !can(c, m, &a.uid, a.access_level, "report")?
    {
        return Err(fail(
            "FORBIDDEN",
            "Whole-book access requires an allowed book-operator level and this module's Read, Edit and Reports permissions.",
        ));
    }
    Ok(())
}
fn info(c: &Connection, m: &str, a: &Claims) -> rusqlite::Result<Value> {
    authorize(c, m, a, false)?;
    let config = load(c, m)?;
    Ok(
        json!({"revision":config.revision,"settings":config.settings,"can_configure":can(c,m,&a.uid,a.access_level,"configure")?,"can_operate":operator(c,m,a,&config.settings).is_ok()}),
    )
}
fn text(s: &str, max: usize) -> bool {
    !s.trim().is_empty() && s.chars().count() <= max && !s.chars().any(char::is_control)
}
fn multiline(s: &str, max: usize) -> bool {
    !s.trim().is_empty()
        && s.chars().count() <= max
        && !s
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
}
fn date(s: &str) -> bool {
    s.len() == 10
        && chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .is_ok_and(|d| d.format("%Y-%m-%d").to_string() == s)
}
/// Decimal strings only: no binary float, rounding, exponent, separators or silent truncation.
fn units(s: &str, scale: u32) -> rusqlite::Result<i64> {
    if scale > 6 || s.len() > 24 || s.is_empty() {
        return Err(fail(
            "INVALID",
            "Enter an exact, positive decimal amount within the configured precision.",
        ));
    }
    let (whole, fraction) = s.split_once('.').unwrap_or((s, ""));
    if whole.is_empty()
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || !fraction.bytes().all(|b| b.is_ascii_digit())
        || fraction.len() > scale as usize
        || s.ends_with('.')
    {
        return Err(fail(
            "INVALID",
            format!("Use digits and at most {scale} decimal places; no rounding or separators."),
        ));
    }
    let factor = 10_i64.pow(scale);
    let fraction_value = if fraction.is_empty() {
        Some(0)
    } else {
        fraction.parse::<i64>().ok()
    };
    let value = whole
        .parse::<i64>()
        .ok()
        .and_then(|w| w.checked_mul(factor))
        .and_then(|w| {
            fraction_value
                .and_then(|f| f.checked_mul(10_i64.pow(scale - fraction.len() as u32)))
                .and_then(|f| w.checked_add(f))
        });
    value.filter(|v| *v > 0).ok_or_else(|| {
        fail(
            "INVALID",
            "The amount must be greater than zero and within the supported range.",
        )
    })
}
fn decimal(value: i64, scale: u32) -> String {
    let v = i128::from(value);
    let factor = 10_i128.pow(scale);
    if scale == 0 {
        v.to_string()
    } else {
        format!(
            "{}{}.{:0width$}",
            if v < 0 { "-" } else { "" },
            v.abs() / factor,
            v.abs() % factor,
            width = scale as usize
        )
    }
}
fn add(a: i64, b: i64) -> rusqlite::Result<i64> {
    a.checked_add(b).ok_or_else(|| {
        fail(
            "INVALID",
            "This operation exceeds the book's exact-integer balance limit. Nothing was posted.",
        )
    })
}
fn audit(c: &Connection, m: &str, a: &Claims, event: &str, value: &Value) -> rusqlite::Result<()> {
    c.execute("INSERT INTO mx_book_audit(module_uid,actor_uid,event,evidence_json,created_at) VALUES(?1,?2,?3,?4,?5)",params![m,a.uid,event,value.to_string(),chrono::Utc::now().timestamp_millis()])?;
    Ok(())
}
fn save(c: &Connection, m: &str, a: &Claims, next: Configuration) -> rusqlite::Result<Value> {
    let tx = rusqlite::Transaction::new_unchecked(c, TransactionBehavior::Immediate)?;
    authorize(&tx, m, a, true)?;
    let old = load(&tx, m)?;
    let s = &next.settings;
    if old.revision != next.revision {
        return Err(fail(
            "CONFLICT",
            "Book settings changed. Reload before saving.",
        ));
    }
    if s.money_decimals > 6
        || s.stock_decimals > 6
        || s.operator_levels.is_empty()
        || !s.operator_levels.contains(&0)
        || s.operator_levels.iter().any(|n| !matches!(n, 0 | 1 | 2))
        || s.operator_levels.len() > 3
        || (!s.closed_through.is_empty() && !date(&s.closed_through))
        || (!s.currency.is_empty()
            && (s.currency.len() != 3 || !s.currency.bytes().all(|b| b.is_ascii_uppercase())))
        || (s.accounting && s.currency.is_empty())
    {
        return Err(fail(
            "INVALID",
            "Choose a three-letter currency, 0–6 decimal places, valid book-operator levels including Administrator, and a valid closed-through date.",
        ));
    }
    if !s.required_stage.is_empty() {
        crate::api::workflows::validate_book_stage(&tx, m, &s.required_stage)?;
    }
    let journals: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM mx_book_journals WHERE module_uid=?1)",
        [m],
        |r| r.get(0),
    )?;
    let items: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM mx_book_items WHERE module_uid=?1)",
        [m],
        |r| r.get(0),
    )?;
    if (journals
        && (s.money_decimals != old.settings.money_decimals || s.currency != old.settings.currency))
        || (items && s.stock_decimals != old.settings.stock_decimals)
    {
        return Err(fail(
            "CONFLICT",
            "Currency and money precision cannot change after posting. Stock precision cannot change after creating items. This protects existing amounts.",
        ));
    }
    if !old.settings.closed_through.is_empty() && s.closed_through < old.settings.closed_through {
        return Err(fail(
            "CONFLICT",
            "A closed accounting period cannot be reopened or moved backwards.",
        ));
    }
    tx.execute("INSERT INTO mx_books(module_uid,revision,settings_json) VALUES(?1,?2,?3) ON CONFLICT(module_uid) DO UPDATE SET revision=excluded.revision,settings_json=excluded.settings_json",params![m,add(old.revision,1)?,serde_json::to_string(s).unwrap()])?;
    // Settings changes are evidence too. Empty never-configured modules remain deletable.
    audit(
        &tx,
        m,
        a,
        "settings",
        &json!({"before":old.settings,"after":s}),
    )?;
    let result = info(&tx, m, a)?;
    tx.commit()?;
    Ok(result)
}
fn source(
    c: &rusqlite::Transaction<'_>,
    m: &str,
    a: &Claims,
    s: &Settings,
    value: &Option<Source>,
) -> rusqlite::Result<(Option<String>, Option<String>)> {
    let Some(value) = value else {
        if s.required_stage.is_empty() {
            return Ok((None, None));
        }
        return Err(fail(
            "INVALID",
            "This book requires a source record in the configured process stage.",
        ));
    };
    let revision: Option<i64> = c
        .query_row(
            "SELECT revision FROM mx_records WHERE uid=?1 AND module_uid=?2 AND deleted_at IS NULL",
            params![value.record_uid, m],
            |r| r.get(0),
        )
        .optional()?;
    if revision.is_none() || !privacy::record_allowed(c, &value.record_uid, &a.uid, a.access_level)?
    {
        return Err(fail(
            "FORBIDDEN",
            "The source record is unavailable to your account.",
        ));
    }
    if revision != Some(value.record_revision) {
        return Err(fail(
            "CONFLICT",
            "The source record changed. Review its latest values before posting.",
        ));
    }
    if !s.required_stage.is_empty() {
        crate::api::workflows::check_book_stage(c, m, &value.record_uid, &s.required_stage, a)?;
    }
    let snapshot = crate::api::lifecycle::capture_record_version(
        c,
        &value.record_uid,
        m,
        "book.posted",
        &a.uid,
    )?;
    Ok((Some(value.record_uid.clone()), Some(snapshot)))
}
fn execute(c: &Connection, m: &str, a: &Claims, request: &Command) -> rusqlite::Result<Value> {
    let op = ClientOperation::parse(
        Some(request.operation_uid.clone()),
        &serde_json::to_value(request).unwrap(),
    )
    .map_err(|e| fail("INVALID", e))?;
    let tx = rusqlite::Transaction::new_unchecked(c, TransactionBehavior::Immediate)?;
    let scope = format!("books:{m}");
    // An authenticated actor may reconcile ONLY their exact, already-committed
    // receipt after settings/access change. This exposes no book contents and
    // cannot authorize a new posting or another actor's receipt.
    if let Some(uid) = operations::existing(&tx, &a.uid, &scope, op.as_ref())? {
        tx.commit()?;
        return Ok(json!({"uid":uid,"replayed":true}));
    }
    let cfg = load(&tx, m)?;
    operator(&tx, m, a, &cfg.settings)?;
    if cfg.revision != request.configuration_revision {
        return Err(fail(
            "CONFLICT",
            "Book settings changed. Refresh the book and review this operation.",
        ));
    }
    let s = &cfg.settings;
    let uid = match &request.action {
        Action::Account {
            code,
            name,
            category,
        } => {
            authorize(&tx, m, a, true)?;
            if !s.accounting
                || !text(code, 40)
                || !text(name, 120)
                || !matches!(
                    category.as_str(),
                    "asset" | "liability" | "equity" | "income" | "expense"
                )
            {
                return Err(fail(
                    "INVALID",
                    "Enable accounting and enter an account code, name and category.",
                ));
            }
            if tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM mx_book_accounts WHERE module_uid=?1 AND code=?2)",
                params![m, code.trim()],
                |r| r.get::<_, bool>(0),
            )? {
                return Err(fail(
                    "CONFLICT",
                    "This account code already exists in this module.",
                ));
            }
            let uid = uuid::Uuid::new_v4().to_string();
            tx.execute("INSERT INTO mx_book_accounts(uid,module_uid,code,name,category) VALUES(?1,?2,?3,?4,?5)",params![uid,m,code.trim(),name.trim(),category])?;
            uid
        }
        Action::Item { code, name, unit } => {
            authorize(&tx, m, a, true)?;
            if !s.inventory || !text(code, 40) || !text(name, 120) || !text(unit, 40) {
                return Err(fail(
                    "INVALID",
                    "Enable inventory and enter an item code, name and unit.",
                ));
            }
            if tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM mx_book_items WHERE module_uid=?1 AND code=?2)",
                params![m, code.trim()],
                |r| r.get::<_, bool>(0),
            )? {
                return Err(fail(
                    "CONFLICT",
                    "This item code already exists in this module.",
                ));
            }
            let uid = uuid::Uuid::new_v4().to_string();
            tx.execute(
                "INSERT INTO mx_book_items(uid,module_uid,code,name,unit) VALUES(?1,?2,?3,?4,?5)",
                params![uid, m, code.trim(), name.trim(), unit.trim()],
            )?;
            uid
        }
        Action::Journal {
            date,
            description,
            source: src,
            lines,
        } => journal::post(&tx, m, a, s, date, description, src, lines, None)?,
        Action::Reverse {
            journal_uid,
            date,
            reason,
        } => journal::reverse(&tx, m, a, s, journal_uid, date, reason)?,
        Action::Stock {
            item_uid,
            movement,
            quantity,
            note,
            source: src,
        } => stock::move_stock(&tx, m, a, s, item_uid, movement, quantity, note, src)?,
        Action::Reservation {
            reservation_uid,
            action,
            note,
        } => stock::finish(&tx, m, a, s, reservation_uid, action, note)?,
    };
    audit(&tx, m, a, "command", &json!({"uid":uid,"command":request}))?;
    operations::complete(&tx, &a.uid, &scope, op.as_ref(), &uid)?;
    tx.commit()?;
    Ok(json!({"uid":uid,"replayed":false}))
}
// Permanent module deletion must never cascade through operational evidence.
pub(crate) fn prevent_delete(c: &Connection, m: &str) -> rusqlite::Result<()> {
    if c.query_row(
        "SELECT EXISTS(SELECT 1 FROM mx_book_audit WHERE module_uid=?1)",
        [m],
        |r| r.get::<_, bool>(0),
    )? {
        return Err(rusqlite::Error::InvalidParameterName("Operational books retain immutable audit history. Archive this module instead of permanently deleting it.".into()));
    }
    Ok(())
}
async fn run(write: bool, f: impl FnOnce(&Connection) -> rusqlite::Result<Value> + Send + 'static) -> Response {
    match tokio::task::spawn_blocking(move || if write { with_sql_write(f) } else { with_sql_connection(f) }).await {
        Ok(Ok(v)) => (StatusCode::OK, Json(v)).into_response(),
        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::InvalidParameterName(e))))
            if e.starts_with("MX_BOOK_") || e.starts_with("MX_OPERATION:") =>
        {
            let (prefix, message) = e.split_once(':').unwrap_or(("", e.as_str()));
            let status = if prefix.contains("FORBIDDEN") {
                StatusCode::FORBIDDEN
            } else if prefix.contains("CONFLICT") || prefix == "MX_OPERATION" {
                StatusCode::CONFLICT
            } else {
                StatusCode::BAD_REQUEST
            };
            (status, Json(json!({"response":message}))).into_response()
        }
        other => {
            crate::report_error!(format!("{other:?}"), "books", "run()");
            (StatusCode::INTERNAL_SERVER_ERROR,Json(json!({"response":"The book operation could not be confirmed. Retry the same operation; do not enter it a second time."}))).into_response()
        }
    }
}
async fn get_info(a: Claims, Path(m): Path<String>) -> Response {
    run(false, move |c| info(c, &m, &a)).await
}
async fn put_info(a: Claims, Path(m): Path<String>, Json(v): Json<Configuration>) -> Response {
    run(true, move |c| save(c, &m, &a, v)).await
}
async fn command(a: Claims, Path(m): Path<String>, Json(v): Json<Command>) -> Response {
    run(true, move |c| execute(c, &m, &a, &v)).await
}
async fn list(
    a: Claims,
    Path((m, kind)): Path<(String, String)>,
    Query(page): Query<Page>,
) -> Response {
    run(false, move |c| {
        let tx = c.unchecked_transaction()?;
        let cfg = load(&tx, &m)?;
        operator(&tx, &m, &a, &cfg.settings)?;
        let after = page.after.unwrap_or(0);
        if after < 0 {
            return Err(fail("INVALID", "The page cursor must be nonnegative."));
        }
        let result = match kind.as_str() {
            "accounts" | "journals" => journal::list(&tx, &m, &cfg.settings, &kind, after)?,
            "items" | "reservations" | "stock-events" => {
                stock::list(&tx, &m, &cfg.settings, &kind, after)?
            }
            _ => {
                return Err(fail(
                    "INVALID",
                    "Choose accounts, journals, items, reservations or stock-events.",
                ));
            }
        };
        tx.commit()?;
        Ok(result)
    })
    .await
}
async fn detail(a: Claims, Path((m, uid)): Path<(String, String)>) -> Response {
    run(false, move |c| {
        let tx = c.unchecked_transaction()?;
        let cfg = load(&tx, &m)?;
        operator(&tx, &m, &a, &cfg.settings)?;
        let result = journal::detail(&tx, &m, &uid, &cfg.settings, &a)?;
        tx.commit()?;
        Ok(result)
    })
    .await
}
pub fn routes() -> Router {
    Router::new()
        .route("/mx/v1/modules/{module}/books", get(get_info).put(put_info))
        .route("/mx/v1/modules/{module}/books/commands", post(command))
        .route("/mx/v1/modules/{module}/books/journals/{uid}", get(detail))
        .route("/mx/v1/modules/{module}/books/list/{kind}", get(list))
        .layer(axum::middleware::from_fn(auth))
}

#[cfg(test)]
mod tests;
