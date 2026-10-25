//! Presentation is opt-in and independent of record values and process decisions.
use crate::{
    api::{
        modules::module_can_for_user_initialized as can,
        mx::schema::load_module_fields_initialized_db,
    },
    db::connector::{SqliteDatabaseError, with_sql_connection},
    middleware::auth::{Claims, auth},
};
use axum::{
    Json, Router,
    extract::Path,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct View {
    pub id: String,
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub title_field: String,
    #[serde(default)]
    pub fields: Vec<String>,
    #[serde(default)]
    pub summary_fields: Vec<String>,
    #[serde(default)]
    pub description: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Section {
    pub name: String,
    pub fields: Vec<String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Layout {
    pub default_view: String,
    pub views: Vec<View>,
    #[serde(default)]
    pub sections: Vec<Section>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Configuration {
    revision: i64,
    layout: Layout,
}
pub(crate) fn ensure_schema(c: &Connection) -> rusqlite::Result<()> {
    if crate::db::schema::initialized(c) {
        return Ok(());
    }
    c.execute_batch("CREATE TABLE IF NOT EXISTS mx_module_workspaces(module_uid TEXT PRIMARY KEY REFERENCES mx_modules(uid) ON DELETE CASCADE, revision INTEGER NOT NULL CHECK(revision>0), layout_json TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS mx_workspace_history(module_uid TEXT NOT NULL REFERENCES mx_modules(uid) ON DELETE CASCADE, revision INTEGER NOT NULL, layout_json TEXT NOT NULL, actor_uid TEXT NOT NULL, created_at INTEGER NOT NULL, PRIMARY KEY(module_uid,revision));")
}
fn fail(code: &str, message: impl AsRef<str>) -> rusqlite::Error {
    rusqlite::Error::InvalidParameterName(format!("MX_LAYOUT_{code}:{}", message.as_ref()))
}
fn access(c: &Connection, module: &str, actor: &Claims, capability: &str) -> rusqlite::Result<()> {
    if can(c, module, &actor.uid, actor.access_level, capability)? {
        Ok(())
    } else {
        Err(fail(
            "FORBIDDEN",
            "Your account does not have permission for this module workspace.",
        ))
    }
}
fn load(c: &Connection, module: &str) -> rusqlite::Result<Configuration> {
    c.query_row(
        "SELECT revision,layout_json FROM mx_module_workspaces WHERE module_uid=?1",
        [module],
        |r| {
            let raw: String = r.get(1)?;
            Ok(Configuration {
                revision: r.get(0)?,
                layout: serde_json::from_str(&raw).map_err(|_| {
                    fail(
                        "INVALID",
                        "Stored workspace layout is invalid. Contact an administrator.",
                    )
                })?,
            })
        },
    )
    .optional()
    .map(|v| {
        v.unwrap_or(Configuration {
            revision: 0,
            layout: Layout::default(),
        })
    })
}
fn text(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.chars().count() <= max && !value.chars().any(char::is_control)
}
fn validate(c: &Connection, module: &str, layout: &Layout) -> rusqlite::Result<()> {
    if layout.views.is_empty() || layout.views.len() > 12 {
        return Err(fail(
            "INVALID",
            "Configure between one and twelve workspace views.",
        ));
    }
    let fields = load_module_fields_initialized_db(c, module, false)?;
    let public = |key: &str| {
        fields.iter().find(|f| {
            f.key == key
                && f.active
                && f.config["access_scope"].as_str().unwrap_or("module") == "module"
        })
    };
    let mut ids = BTreeSet::new();
    for view in &layout.views {
        if !text(&view.id, 40)
            || !view
                .id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            || !ids.insert(&view.id)
            || !text(&view.name, 60)
            || view.description.chars().count() > 300
        {
            return Err(fail(
                "INVALID",
                "Each view needs a unique stable ID, a name of up to 60 characters and a description of up to 300 characters.",
            ));
        }
        if !matches!(
            view.kind.as_str(),
            "records" | "board" | "cards" | "overview" | "documents" | "ledger" | "inventory"
        ) {
            return Err(fail(
                "INVALID",
                "Choose Records, Pipeline board, Cards, Operations overview, Documents, Accounting or Inventory.",
            ));
        }
        if view.fields.len() > 8 || view.summary_fields.len() > 4 {
            return Err(fail(
                "INVALID",
                "A view may show up to eight card fields and four summary fields.",
            ));
        }
        let mut keys = BTreeSet::new();
        for key in &view.fields {
            if public(key).is_none() || !keys.insert(key) {
                return Err(fail(
                    "INVALID",
                    format!(
                        "{}: choose distinct active, module-visible card fields.",
                        view.name
                    ),
                ));
            }
        }
        if !view.title_field.is_empty()
            && public(&view.title_field).is_none_or(|f| f.field_type == "attachments")
        {
            return Err(fail(
                "INVALID",
                format!(
                    "{}: choose an active, module-visible title field.",
                    view.name
                ),
            ));
        }
        let mut summaries = BTreeSet::new();
        for key in &view.summary_fields {
            if public(key).is_none_or(|f| {
                !matches!(
                    f.field_type.as_str(),
                    "integer" | "decimal" | "formula" | "rollup"
                ) || f.config["table_summary"]
                    .as_str()
                    .is_none_or(|s| s == "none")
            }) || !summaries.insert(key)
            {
                return Err(fail(
                    "INVALID",
                    format!(
                        "{}: choose numeric summary fields with Column totals configured first.",
                        view.name
                    ),
                ));
            }
        }
        if view.kind == "board" {
            let enabled:bool=c.query_row("SELECT COALESCE(json_extract(definition_json,'$.enabled'),0) FROM mx_workflows WHERE module_uid=?1",[module],|r|r.get(0)).optional()?.unwrap_or(false);
            if !enabled {
                return Err(fail(
                    "INVALID",
                    "Configure and enable Process & actions before adding a pipeline board.",
                ));
            }
        }
        if view.kind == "documents"
            && !fields
                .iter()
                .any(|f| f.active && f.field_type == "attachments")
        {
            return Err(fail(
                "INVALID",
                "Add a File Attachment field before adding a document library.",
            ));
        }
        if matches!(view.kind.as_str(), "ledger" | "inventory") {
            let raw: Option<String> = c
                .query_row(
                    "SELECT settings_json FROM mx_books WHERE module_uid=?1",
                    [module],
                    |r| r.get(0),
                )
                .optional()?;
            let settings: Value = raw
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or(json!({}));
            if settings[if view.kind == "ledger" {
                "accounting"
            } else {
                "inventory"
            }] != true
            {
                return Err(fail(
                    "INVALID",
                    "Enable the corresponding Accounting & stock book before adding its workspace view.",
                ));
            }
        }
    }
    if !ids.contains(&layout.default_view) {
        return Err(fail(
            "INVALID",
            "Choose an existing view as the module’s opening view.",
        ));
    }
    if layout.sections.len() > 16 {
        return Err(fail("INVALID", "Use up to sixteen record layout sections."));
    }
    let mut assigned = BTreeSet::new();
    for section in &layout.sections {
        if !text(&section.name, 80) || section.fields.is_empty() || section.fields.len() > 100 {
            return Err(fail(
                "INVALID",
                "Each record section needs a name and between one and 100 fields.",
            ));
        }
        for key in &section.fields {
            if !fields.iter().any(|f| f.key == *key && f.active) || !assigned.insert(key) {
                return Err(fail(
                    "INVALID",
                    "Each active record field may belong to only one layout section.",
                ));
            }
        }
    }
    Ok(())
}
fn save(
    c: &Connection,
    module: &str,
    actor: &Claims,
    request: Configuration,
) -> rusqlite::Result<Value> {
    let tx = rusqlite::Transaction::new_unchecked(c, TransactionBehavior::Immediate)?;
    access(&tx, module, actor, "configure")?;
    if request.revision != load(&tx, module)?.revision {
        return Err(fail(
            "CONFLICT",
            "Another administrator changed this workspace. Reload before saving; your draft has not been applied.",
        ));
    }
    validate(&tx, module, &request.layout)?;
    let revision = request
        .revision
        .checked_add(1)
        .ok_or_else(|| fail("INVALID", "Workspace revision is out of range."))?;
    let raw = serde_json::to_string(&request.layout)
        .map_err(|_| fail("INVALID", "Workspace layout could not be encoded."))?;
    tx.execute("INSERT INTO mx_module_workspaces(module_uid,revision,layout_json) VALUES(?1,?2,?3) ON CONFLICT(module_uid) DO UPDATE SET revision=excluded.revision,layout_json=excluded.layout_json",params![module,revision,raw])?;
    tx.execute("INSERT INTO mx_workspace_history(module_uid,revision,layout_json,actor_uid,created_at) VALUES(?1,?2,?3,?4,unixepoch())",params![module,revision,raw,actor.uid])?;
    tx.commit()?;
    Ok(json!({"revision":revision,"layout":request.layout}))
}
pub(crate) fn field_dependencies(
    c: &Connection,
    module: &str,
    key: &str,
) -> rusqlite::Result<Vec<String>> {
    if !c.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='mx_module_workspaces')",[],|r|r.get::<_,bool>(0))? { return Ok(Vec::new()); }
    let layout = load(c, module)?.layout;
    let mut result = Vec::new();
    for v in layout.views {
        if v.title_field == key
            || v.fields.iter().any(|k| k == key)
            || v.summary_fields.iter().any(|k| k == key)
        {
            result.push(format!("Workspace view “{}” displays this field.", v.name));
        }
    }
    for s in layout.sections {
        if s.fields.iter().any(|k| k == key) {
            result.push(format!(
                "Record layout section “{}” contains this field.",
                s.name
            ));
        }
    }
    Ok(result)
}
async fn run(
    task: impl FnOnce(&Connection) -> rusqlite::Result<Value> + Send + 'static,
) -> Response {
    match tokio::task::spawn_blocking(move || with_sql_connection(task)).await {
        Ok(Ok(value)) => Json(value).into_response(),
        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::InvalidParameterName(message)))) => {
            let code = if message.starts_with("MX_LAYOUT_FORBIDDEN:") {
                StatusCode::FORBIDDEN
            } else if message.starts_with("MX_LAYOUT_CONFLICT:") {
                StatusCode::CONFLICT
            } else {
                StatusCode::BAD_REQUEST
            };
            (
                code,
                Json(json!({"response":message.split_once(':').map(|(_,s)|s).unwrap_or(&message)})),
            )
                .into_response()
        }
        result => {
            tracing::error!("Module workspace operation failed: {result:?}");
            (StatusCode::INTERNAL_SERVER_ERROR,Json(json!({"response":"MX could not load or save this workspace. Retry after checking its current configuration."}))).into_response()
        }
    }
}
async fn get_config(actor: Claims, Path(module): Path<String>) -> Response {
    run(move |c| {
        access(c, &module, &actor, "read")?;
        Ok(json!(load(c, &module)?))
    })
    .await
}
async fn put_config(
    actor: Claims,
    Path(module): Path<String>,
    Json(request): Json<Configuration>,
) -> Response {
    let event_module = module.clone();
    let event_actor = actor.uid.clone();
    let response = run(move |c| save(c, &module, &actor, request)).await;
    if response.status().is_success() {
        crate::api::live::publish_live_event(
            "module.workspace.updated",
            Some(&event_actor),
            json!({"module_uid":event_module}),
        );
    }
    response
}
pub fn routes() -> Router {
    Router::new()
        .route(
            "/mx/v1/modules/{module}/workspace",
            get(get_config).put(put_config),
        )
        .layer(axum::middleware::from_fn(auth))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn actor(level: i64) -> Claims {
        Claims {
            uid: "admin".into(),
            email: "a@x".into(),
            access_level: level,
            auth_version: 0,
            token_kind: "access".into(),
            exp: usize::MAX,
        }
    }
    fn setup() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("PRAGMA foreign_keys=ON;CREATE TABLE users(uid TEXT PRIMARY KEY,name TEXT,email TEXT,access_level INTEGER);INSERT INTO users VALUES('admin','Admin','a@x',0);").unwrap();
        crate::db::schema::initialize(&c).unwrap();
        c.execute_batch("INSERT INTO mx_modules(uid,slug,name,singular_name,created_at,updated_at)VALUES('m','m','Module','Item',0,0);").unwrap();
        c
    }
    fn layout() -> Layout {
        Layout {
            default_view: "cards".into(),
            views: vec![View {
                id: "cards".into(),
                name: "Customer directory".into(),
                kind: "cards".into(),
                title_field: String::new(),
                fields: vec![],
                summary_fields: vec![],
                description: String::new(),
            }],
            sections: vec![],
        }
    }
    #[test]
    fn no_implicit_workspace_or_data() {
        let c = setup();
        assert_eq!(load(&c, "m").unwrap().revision, 0);
        assert!(load(&c, "m").unwrap().layout.views.is_empty());
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM mx_records", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    #[test]
    fn revision_and_atomic_history() {
        let c = setup();
        save(
            &c,
            "m",
            &actor(0),
            Configuration {
                revision: 0,
                layout: layout(),
            },
        )
        .unwrap();
        assert!(
            save(
                &c,
                "m",
                &actor(0),
                Configuration {
                    revision: 0,
                    layout: layout()
                }
            )
            .is_err()
        );
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM mx_workspace_history", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
    #[test]
    fn validates_view_types_and_prerequisites() {
        let c = setup();
        for kind in ["board", "documents", "ledger", "inventory", "pretend-erp"] {
            let mut l = layout();
            l.views[0].kind = kind.into();
            assert!(validate(&c, "m", &l).is_err(), "{kind}");
        }
        let mut l = layout();
        l.default_view = "missing".into();
        assert!(validate(&c, "m", &l).is_err());
        l = layout();
        l.views.push(l.views[0].clone());
        assert!(validate(&c, "m", &l).is_err());
    }
    #[test]
    fn cannot_change_without_configure() {
        let c = setup();
        assert!(
            save(
                &c,
                "m",
                &actor(3),
                Configuration {
                    revision: 0,
                    layout: layout()
                }
            )
            .is_err()
        );
        assert_eq!(load(&c, "m").unwrap().revision, 0);
    }
    #[test]
    fn read_only_load_does_not_request_writer() {
        let c = setup();
        c.execute_batch("BEGIN IMMEDIATE").unwrap();
        assert!(load(&c, "m").is_ok());
        c.execute_batch("ROLLBACK").unwrap();
    }
}
