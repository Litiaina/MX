//! Opt-in module processes. Ordinary record writes cannot set a workflow state.
//! Decisions reference an exact record snapshot; reapproval after edits is an
//! explicit policy, evaluated without adding writes to any record read path.
use crate::{
    api::{
        modules::module_can_for_user_initialized,
        mx::{privacy, schema::load_module_fields_initialized_db},
        operations::{self, ClientOperation},
    },
    db::connector::{SqliteDatabaseError, with_sql_connection},
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
use std::collections::BTreeSet;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Definition {
    pub enabled: bool,
    pub name: String,
    pub kind: String,
    pub initial_state: String,
    pub title_field: String,
    pub restart_on_edit: bool,
    pub states: Vec<State>,
    pub actions: Vec<Action>,
}
impl Default for Definition {
    fn default() -> Self {
        Self {
            enabled: false,
            name: "Custom process".into(),
            kind: "custom".into(),
            initial_state: String::new(),
            title_field: String::new(),
            restart_on_edit: true,
            states: Vec::new(),
            actions: Vec::new(),
        }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub key: String,
    pub name: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Action {
    pub key: String,
    pub name: String,
    pub from: String,
    pub to: String,
    pub access_levels: Vec<i64>,
    pub creator_policy: String,
    pub required_fields: Vec<String>,
    pub note_required: bool,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
struct Configuration {
    revision: i64,
    definition: Definition,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SaveConfiguration {
    revision: i64,
    definition: Definition,
    confirm_existing_records: bool,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Execute {
    operation_uid: String,
    action: String,
    configuration_revision: i64,
    workflow_revision: i64,
    record_revision: i64,
    note: String,
}
#[derive(Default, Deserialize)]
struct QueueQuery {
    after: Option<String>,
    state: Option<String>,
    actionable: Option<bool>,
    q: Option<String>,
    counts_only: Option<bool>,
    include_total: Option<bool>,
}
#[derive(Default, Deserialize)]
struct HistoryQuery {
    before: Option<i64>,
}

fn fail(code: &str, message: impl AsRef<str>) -> rusqlite::Error {
    rusqlite::Error::InvalidParameterName(format!("MX_WORKFLOW_{code}:{}", message.as_ref()))
}
pub(crate) fn ensure_schema(c: &Connection) -> rusqlite::Result<()> {
    if crate::db::schema::initialized(c) {
        return Ok(());
    }
    c.execute_batch("CREATE TABLE IF NOT EXISTS mx_workflows(
        module_uid TEXT PRIMARY KEY REFERENCES mx_modules(uid) ON DELETE CASCADE,
        revision INTEGER NOT NULL CHECK(revision>0), definition_json TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS mx_workflow_config_history(
        module_uid TEXT NOT NULL REFERENCES mx_modules(uid) ON DELETE CASCADE,
        revision INTEGER NOT NULL, definition_json TEXT NOT NULL, actor_uid TEXT NOT NULL,
        created_at INTEGER NOT NULL, PRIMARY KEY(module_uid,revision));
      CREATE TABLE IF NOT EXISTS mx_workflow_instances(
        record_uid TEXT PRIMARY KEY REFERENCES mx_records(uid) ON DELETE CASCADE,
        state TEXT NOT NULL, revision INTEGER NOT NULL CHECK(revision>0),
        record_revision INTEGER NOT NULL, configuration_revision INTEGER NOT NULL);
      CREATE INDEX IF NOT EXISTS mx_workflow_instances_state ON mx_workflow_instances(state,record_uid);
      CREATE INDEX IF NOT EXISTS mx_records_workflow_queue ON mx_records(module_uid,uid) WHERE deleted_at IS NULL;
      CREATE TABLE IF NOT EXISTS mx_workflow_events(
        uid TEXT PRIMARY KEY, record_uid TEXT NOT NULL REFERENCES mx_records(uid) ON DELETE CASCADE,
        sequence INTEGER NOT NULL, configuration_revision INTEGER NOT NULL,
        action_key TEXT NOT NULL, action_name TEXT NOT NULL, from_state TEXT NOT NULL,
        from_name TEXT NOT NULL, to_state TEXT NOT NULL, to_name TEXT NOT NULL,
        actor_uid TEXT NOT NULL, note TEXT NOT NULL, record_revision INTEGER NOT NULL,
        snapshot_uid TEXT NOT NULL, created_at INTEGER NOT NULL,
        UNIQUE(record_uid,sequence));")
}
fn load(c: &Connection, module: &str) -> rusqlite::Result<Configuration> {
    c.query_row(
        "SELECT revision,definition_json FROM mx_workflows WHERE module_uid=?1",
        [module],
        |r| {
            let raw: String = r.get(1)?;
            Ok(Configuration {
                revision: r.get(0)?,
                definition: serde_json::from_str(&raw).map_err(|_| {
                    fail(
                        "INVALID",
                        "Stored workflow configuration is invalid. Contact an administrator.",
                    )
                })?,
            })
        },
    )
    .optional()
    .map(|v| {
        v.unwrap_or(Configuration {
            revision: 0,
            definition: Definition::default(),
        })
    })
}
fn key(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 40
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}
pub(crate) fn validate_book_stage(
    c: &Connection,
    module: &str,
    stage: &str,
) -> rusqlite::Result<()> {
    let cfg = load(c, module)?;
    if !cfg.definition.enabled || !cfg.definition.states.iter().any(|s| s.key == stage) {
        return Err(crate::api::books::fail(
            "INVALID",
            "Choose a stage from this module's enabled process before requiring it for posting.",
        ));
    }
    Ok(())
}
pub(crate) fn check_book_stage(
    c: &Connection,
    module: &str,
    record: &str,
    stage: &str,
    actor: &Claims,
) -> rusqlite::Result<()> {
    validate_book_stage(c, module, stage)?;
    let revision = record_access(c, module, record, actor)?;
    let cfg = load(c, module)?;
    let p = position(c, record, revision, &cfg.definition)?;
    if p.state != stage || p.changed || p.revision == 0 {
        return Err(crate::api::books::fail(
            "CONFLICT",
            "The source record must have a completed decision in the required stage, with no edits since that decision. Review and approve it again before posting.",
        ));
    }
    Ok(())
}
fn label(s: &str) -> bool {
    !s.trim().is_empty() && s.chars().count() <= 120 && !s.chars().any(char::is_control)
}
fn validate(c: &Connection, module: &str, d: &Definition) -> rusqlite::Result<()> {
    if d.name.chars().count() > 120
        || d.kind.len() > 24
        || d.initial_state.len() > 40
        || d.title_field.len() > 64
    {
        return Err(fail(
            "INVALID",
            "Process names may contain at most 120 characters; use configured stage and field keys.",
        ));
    }
    if d.states.is_empty() && !d.enabled && d.actions.is_empty() {
        return Ok(());
    }
    if !label(&d.name) || !matches!(d.kind.as_str(), "custom" | "crm" | "erp" | "accounting") {
        return Err(fail(
            "INVALID",
            "Give the process a name and choose Custom, CRM, ERP or Accounting approvals.",
        ));
    }
    if !(2..=24).contains(&d.states.len())
        || d.actions.len() > 64
        || (d.enabled && d.actions.is_empty())
    {
        return Err(fail(
            "INVALID",
            "Configure 2–24 stages and 1–64 actions before enabling this process.",
        ));
    }
    let states: BTreeSet<_> = d.states.iter().map(|s| s.key.as_str()).collect();
    if states.len() != d.states.len()
        || !states.contains(d.initial_state.as_str())
        || d.states.iter().any(|s| !key(&s.key) || !label(&s.name))
    {
        return Err(fail(
            "INVALID",
            "Stages need distinct stable keys, names and a valid starting stage.",
        ));
    }
    let fields = load_module_fields_initialized_db(c, module, false)?;
    if !d.title_field.is_empty()
        && !fields.iter().any(|f| {
            f.key == d.title_field
                && f.config["access_scope"].as_str().unwrap_or("module") == "module"
                && matches!(
                    f.field_type.as_str(),
                    "text" | "long_text" | "auto_number" | "integer" | "date" | "select"
                )
        })
    {
        return Err(fail(
            "INVALID",
            "The queue title must be an active, module-visible text, number, date or select field.",
        ));
    }
    let mut keys = BTreeSet::new();
    for a in &d.actions {
        if !key(&a.key)
            || !label(&a.name)
            || !keys.insert(&a.key)
            || a.from == a.to
            || !states.contains(a.from.as_str())
            || !states.contains(a.to.as_str())
        {
            return Err(fail(
                "INVALID",
                "Each action needs a distinct key, a name, and different valid source and destination stages.",
            ));
        }
        if a.access_levels.is_empty()
            || a.access_levels.len() > 4
            || a.access_levels.iter().any(|l| !(0..=3).contains(l))
            || !matches!(a.creator_policy.as_str(), "any" | "creator" | "not_creator")
        {
            return Err(fail(
                "INVALID",
                format!(
                    "{}: choose permitted account levels and a valid creator policy. Module Edit permission is also required.",
                    a.name
                ),
            ));
        }
        if a.required_fields.len() > 32
            || a.required_fields.iter().collect::<BTreeSet<_>>().len() != a.required_fields.len()
        {
            return Err(fail(
                "INVALID",
                format!("{}: choose at most 32 distinct required fields.", a.name),
            ));
        }
        for field in &a.required_fields {
            if !fields.iter().any(|f| {
                &f.key == field
                    && f.config["access_scope"].as_str().unwrap_or("module") == "module"
                    && !matches!(f.field_type.as_str(), "lookup" | "rollup" | "formula")
            }) {
                return Err(fail(
                    "INVALID",
                    format!(
                        "{}: required field ‘{}’ is missing, archived, calculated or restricted. Reconfigure this action first.",
                        a.name, field
                    ),
                ));
            }
        }
    }
    Ok(())
}
/// Field edits/archives must not strand a saved process. Never creates tables.
pub(crate) fn validate_module_fields(c: &Connection, module: &str) -> rusqlite::Result<()> {
    let exists: bool = c.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='mx_workflows')",
        [],
        |r| r.get(0),
    )?;
    if exists {
        validate(c, module, &load(c, module)?.definition).map_err(|e| {
            if let rusqlite::Error::InvalidParameterName(message) = e {
                rusqlite::Error::InvalidParameterName(format!(
                    "MX_CONFIG:Process & actions: {}",
                    message.split_once(':').map(|(_, m)| m).unwrap_or(&message)
                ))
            } else {
                e
            }
        })?;
    }
    Ok(())
}
pub(crate) fn field_dependencies(
    c: &Connection,
    module: &str,
    key: &str,
) -> rusqlite::Result<Vec<String>> {
    let exists: bool = c.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='mx_workflows')",
        [],
        |r| r.get(0),
    )?;
    if !exists {
        return Ok(Vec::new());
    }
    let d = load(c, module)?.definition;
    let mut reasons = Vec::new();
    if d.title_field == key {
        reasons.push(format!(
            "{} uses this field as its work-queue title.",
            d.name
        ));
    }
    for a in d.actions {
        if a.required_fields.iter().any(|f| f == key) {
            reasons.push(format!(
                "{} → {} requires this field before the action may be taken.",
                d.name, a.name
            ));
        }
    }
    Ok(reasons)
}
fn access(c: &Connection, module: &str, actor: &Claims, capability: &str) -> rusqlite::Result<()> {
    if !module_can_for_user_initialized(c, module, &actor.uid, actor.access_level, capability)? {
        return Err(fail(
            "FORBIDDEN",
            "You do not have permission for this module process.",
        ));
    }
    Ok(())
}
fn record_access(
    c: &Connection,
    module: &str,
    record: &str,
    actor: &Claims,
) -> rusqlite::Result<i64> {
    access(c, module, actor, "read")?;
    c.query_row(&format!("SELECT r.revision FROM mx_records r WHERE r.uid=?1 AND r.module_uid=?2 AND r.deleted_at IS NULL AND {}",privacy::record_sql("r",&actor.uid,actor.access_level)),params![record,module],|r|r.get(0))
}
fn read_configuration(c: &Connection, module: &str, actor: &Claims) -> rusqlite::Result<Value> {
    // A short snapshot transaction keeps the config revision/count coherent.
    let tx = c.unchecked_transaction()?;
    let can_read =
        module_can_for_user_initialized(&tx, module, &actor.uid, actor.access_level, "read")?;
    let can_configure =
        module_can_for_user_initialized(&tx, module, &actor.uid, actor.access_level, "configure")?;
    if !can_read && !can_configure {
        return Err(fail(
            "FORBIDDEN",
            "You do not have permission to view this module process.",
        ));
    }
    let configuration = load(&tx, module)?;
    // Count is an administration-only impact preview, not a privacy bypass.
    let count = if can_configure {
        Some(tx.query_row(
            "SELECT COUNT(*) FROM mx_records WHERE module_uid=?1 AND deleted_at IS NULL",
            [module],
            |r| r.get::<_, i64>(0),
        )?)
    } else {
        None
    };
    tx.commit()?;
    Ok(
        json!({"revision":configuration.revision,"definition":configuration.definition,"existing_records":count}),
    )
}
fn save(
    c: &Connection,
    module: &str,
    actor: &Claims,
    request: SaveConfiguration,
) -> rusqlite::Result<Value> {
    let tx = rusqlite::Transaction::new_unchecked(c, TransactionBehavior::Immediate)?;
    access(&tx, module, actor, "configure")?;
    let previous = load(&tx, module)?;
    if request.revision != previous.revision {
        return Err(fail(
            "CONFLICT",
            "Another administrator changed this process. Reload it before saving; your draft has not been applied.",
        ));
    }
    validate(&tx, module, &request.definition)?;
    if !previous.definition.states.is_empty() {
        // Initial/stable stage keys are never silently reinterpreted for legacy
        // records. Rename labels, add stages, or disable, but preserve identity.
        if request.definition.initial_state != previous.definition.initial_state
            || previous
                .definition
                .states
                .iter()
                .any(|s| !request.definition.states.iter().any(|n| n.key == s.key))
        {
            return Err(fail(
                "INVALID",
                "Keep the original starting stage and saved stage keys. You may rename their labels, add stages or disable the process without losing history.",
            ));
        }
    }
    if request.definition.enabled
        && !previous.definition.enabled
        && !request.confirm_existing_records
    {
        let count: i64 = tx.query_row(
            "SELECT COUNT(*) FROM mx_records WHERE module_uid=?1 AND deleted_at IS NULL",
            [module],
            |r| r.get(0),
        )?;
        if count > 0 {
            return Err(fail(
                "INVALID",
                format!(
                    "Confirm enabling this process for {count} existing records. Records without workflow history will start in the configured starting stage."
                ),
            ));
        }
    }
    let revision = previous.revision + 1;
    let raw = serde_json::to_string(&request.definition)
        .map_err(|_| fail("INVALID", "Could not encode process configuration."))?;
    tx.execute("INSERT INTO mx_workflows(module_uid,revision,definition_json) VALUES(?1,?2,?3) ON CONFLICT(module_uid) DO UPDATE SET revision=excluded.revision,definition_json=excluded.definition_json",params![module,revision,raw])?;
    tx.execute("INSERT INTO mx_workflow_config_history(module_uid,revision,definition_json,actor_uid,created_at) VALUES(?1,?2,?3,?4,?5)",params![module,revision,raw,actor.uid,chrono::Utc::now().timestamp_millis()])?;
    tx.commit()?;
    Ok(json!({"revision":revision,"definition":request.definition}))
}
struct Position {
    state: String,
    revision: i64,
    changed: bool,
    restarted: bool,
}
fn position(
    c: &Connection,
    record: &str,
    revision: i64,
    d: &Definition,
) -> rusqlite::Result<Position> {
    let stored = c
        .query_row(
            "SELECT state,revision,record_revision FROM mx_workflow_instances WHERE record_uid=?1",
            [record],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            },
        )
        .optional()?;
    let (state, workflow_revision, changed) = stored
        .map(|(s, w, r)| (s, w, r != revision))
        .unwrap_or((d.initial_state.clone(), 0, false));
    let restarted = changed && d.restart_on_edit;
    Ok(Position {
        state: if restarted {
            d.initial_state.clone()
        } else {
            state
        },
        revision: workflow_revision,
        changed,
        restarted,
    })
}
fn state_name<'a>(d: &'a Definition, state: &'a str) -> &'a str {
    d.states
        .iter()
        .find(|s| s.key == state)
        .map(|s| s.name.as_str())
        .unwrap_or(state)
}
fn actor_allowed(a: &Action, actor: &Claims, creator: Option<&str>) -> bool {
    a.access_levels.contains(&actor.access_level)
        && match a.creator_policy.as_str() {
            "creator" => creator == Some(actor.uid.as_str()),
            // Unknown historical ownership is never proof of separation of duties.
            "not_creator" => creator.is_some_and(|c| c != actor.uid),
            "any" => true,
            _ => false,
        }
}
fn missing(
    c: &Connection,
    module: &str,
    record: &str,
    a: &Action,
) -> rusqlite::Result<Vec<String>> {
    // At most 32 requirements, checked in one query (including attachments).
    let mut statement=c.prepare("SELECT f.label FROM mx_fields f LEFT JOIN mx_record_values v ON v.field_uid=f.uid AND v.record_uid=?2
      WHERE f.module_uid=?1 AND f.active=1 AND COALESCE(f.module_key,f.field_key) IN (SELECT value FROM json_each(?3))
      AND CASE WHEN f.field_type='attachments' THEN NOT EXISTS(SELECT 1 FROM mx_attachments a WHERE a.entry_uid=?2 AND a.attachment_field_uid=f.uid)
      ELSE v.record_uid IS NULL OR (v.value_integer IS NULL AND v.value_real IS NULL AND v.value_boolean IS NULL AND (v.value_text IS NULL OR trim(v.value_text)='' OR v.value_text='[]')) END ORDER BY f.position,f.uid")?;
    statement
        .query_map(
            params![module, record, json!(a.required_fields).to_string()],
            |r| r.get::<_, String>(0),
        )?
        .collect()
}
fn status(c: &Connection, module: &str, record: &str, actor: &Claims) -> rusqlite::Result<Value> {
    let revision = record_access(c, module, record, actor)?;
    let configuration = load(c, module)?;
    let d = &configuration.definition;
    if !d.enabled && d.states.is_empty() {
        return Ok(json!({"enabled":false,"configuration_revision":configuration.revision}));
    }
    let p = position(c, record, revision, d)?;
    if !d.enabled {
        return Ok(
            json!({"enabled":false,"has_history":p.revision>0,"name":d.name,"kind":d.kind,"configuration_revision":configuration.revision,"workflow_revision":p.revision,"record_revision":revision,"state_name":state_name(d,&p.state),"changed_since_action":p.changed,"restarted":p.restarted,"actions":[]}),
        );
    }
    let creator: Option<String> = c
        .query_row(
            "SELECT creator_uid FROM mx_record_owners WHERE record_uid=?1",
            [record],
            |r| r.get(0),
        )
        .optional()?;
    let can_update =
        module_can_for_user_initialized(c, module, &actor.uid, actor.access_level, "update")?;
    let mut actions = Vec::new();
    if can_update {
        for a in d
            .actions
            .iter()
            .filter(|a| a.from == p.state && a.access_levels.contains(&actor.access_level))
        {
            let blocked_reason = if actor_allowed(a, actor, creator.as_deref()) {
                None
            } else if creator.is_none() {
                Some(
                    "This record has no verified creator. The configured creator/reviewer rule cannot be satisfied.",
                )
            } else if a.creator_policy == "not_creator" {
                Some("A different person from the record creator must take this action.")
            } else {
                Some("Only the record creator may take this action.")
            };
            actions.push(json!({"key":a.key,"name":a.name,"to":a.to,"to_name":state_name(d,&a.to),"note_required":a.note_required,"missing_fields":missing(c,module,record,a)?,"blocked_reason":blocked_reason}));
        }
    }
    Ok(
        json!({"enabled":true,"name":d.name,"kind":d.kind,"state":p.state,"state_name":state_name(d,&p.state),"workflow_revision":p.revision,"configuration_revision":configuration.revision,"record_revision":revision,"changed_since_action":p.changed,"restarted":p.restarted,"actions":actions}),
    )
}
fn execute(
    c: &Connection,
    module: &str,
    record: &str,
    actor: &Claims,
    request: &Execute,
) -> rusqlite::Result<Value> {
    if request.note.chars().count() > 2000 {
        return Err(fail("INVALID", "Keep the action note to 2,000 characters."));
    }
    let operation = ClientOperation::parse(Some(request.operation_uid.clone()), &json!(request))
        .map_err(|m| fail("INVALID", m))?;
    let tx = rusqlite::Transaction::new_unchecked(c, TransactionBehavior::Immediate)?;
    let revision = record_access(&tx, module, record, actor)?;
    access(&tx, module, actor, "update")?;
    let scope = format!("workflow:{module}:{record}");
    if let Some(event) = operations::existing(&tx, &actor.uid, &scope, operation.as_ref())? {
        return Ok(
            json!({"event_uid":event,"replayed":true,"status":status(&tx,module,record,actor)?}),
        );
    }
    let configuration = load(&tx, module)?;
    let d = &configuration.definition;
    if !d.enabled {
        return Err(fail("INVALID", "This process is not enabled."));
    }
    let p = position(&tx, record, revision, d)?;
    if request.configuration_revision != configuration.revision
        || request.workflow_revision != p.revision
        || request.record_revision != revision
    {
        return Err(fail(
            "CONFLICT",
            "This record or its process changed while you were reviewing it. Reload and review the latest values before taking action.",
        ));
    }
    // Revalidate field dependencies within the write snapshot, not only at
    // administration time. No missing/archived requirement can be bypassed.
    validate(&tx, module, d)?;
    let a = d
        .actions
        .iter()
        .find(|a| a.key == request.action && a.from == p.state)
        .ok_or_else(|| {
            fail(
                "CONFLICT",
                "This action is not available from the current stage.",
            )
        })?;
    let creator: Option<String> = tx
        .query_row(
            "SELECT creator_uid FROM mx_record_owners WHERE record_uid=?1",
            [record],
            |r| r.get(0),
        )
        .optional()?;
    if !actor_allowed(a, actor, creator.as_deref()) {
        return Err(fail(
            "FORBIDDEN",
            "This action is not permitted for your account or this record's creator policy.",
        ));
    }
    let missing = missing(&tx, module, record, a)?;
    if !missing.is_empty() {
        return Err(fail(
            "INVALID",
            format!(
                "Complete these fields before {}: {}.",
                a.name,
                missing.join(", ")
            ),
        ));
    }
    if a.note_required && request.note.trim().is_empty() {
        return Err(fail("INVALID", "Add a note explaining this action."));
    }
    let next = p.revision + 1;
    let event = uuid::Uuid::new_v4().to_string();
    let snapshot = crate::api::lifecycle::capture_record_version(
        &tx,
        record,
        module,
        "workflow_action",
        &actor.uid,
    )?;
    tx.execute("INSERT INTO mx_workflow_instances(record_uid,state,revision,record_revision,configuration_revision) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(record_uid) DO UPDATE SET state=excluded.state,revision=excluded.revision,record_revision=excluded.record_revision,configuration_revision=excluded.configuration_revision",params![record,a.to,next,revision,configuration.revision])?;
    tx.execute("INSERT INTO mx_workflow_events(uid,record_uid,sequence,configuration_revision,action_key,action_name,from_state,from_name,to_state,to_name,actor_uid,note,record_revision,snapshot_uid,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",params![event,record,next,configuration.revision,a.key,a.name,p.state,state_name(d,&p.state),a.to,state_name(d,&a.to),actor.uid,request.note.trim(),revision,snapshot,chrono::Utc::now().timestamp_millis()])?;
    let notifications =
        decision_notifications(&tx, module, record, actor, d, a, creator.as_deref(), &event)?;
    operations::complete(&tx, &actor.uid, &scope, operation.as_ref(), &event)?;
    let result =
        json!({"event_uid":event,"replayed":false,"status":status(&tx,module,record,actor)?});
    tx.commit()?;
    // The durable notification and decision committed together. A missed live
    // delivery is recovered through the existing notification centre/reconnect.
    for (recipient, notification) in notifications {
        crate::api::notifications::deliver_notification(&recipient, &notification);
    }
    Ok(result)
}
fn decision_notifications(
    c: &Connection,
    module: &str,
    record: &str,
    actor: &Claims,
    d: &Definition,
    action: &Action,
    creator: Option<&str>,
    event: &str,
) -> rusqlite::Result<Vec<(String, crate::api::notifications::Notification)>> {
    use crate::api::notifications::{NewNotification, insert_notification};
    let levels: BTreeSet<i64> = d
        .actions
        .iter()
        .filter(|a| a.from == action.to)
        .flat_map(|a| a.access_levels.iter().copied())
        .collect();
    let candidates=c.prepare("SELECT uid,access_level FROM users WHERE uid<>?1 AND (uid=?2 OR access_level IN (SELECT value FROM json_each(?3))) ORDER BY uid")?.query_map(params![actor.uid,creator,json!(levels).to_string()],|r|Ok((r.get::<_,String>(0)?,r.get::<_,i64>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let mut notifications = Vec::new();
    for (uid, level) in candidates {
        let mut recipient = actor.clone();
        recipient.uid = uid.clone();
        recipient.access_level = level;
        match record_access(c, module, record, &recipient) {
            Ok(_) => {}
            Err(rusqlite::Error::QueryReturnedNoRows) => continue,
            Err(rusqlite::Error::InvalidParameterName(message))
                if message.starts_with("MX_WORKFLOW_FORBIDDEN:") =>
            {
                continue;
            }
            Err(error) => return Err(error),
        }
        let is_creator = creator == Some(uid.as_str());
        let can_act = module_can_for_user_initialized(c, module, &uid, level, "update")?
            && d.actions
                .iter()
                .any(|a| a.from == action.to && actor_allowed(a, &recipient, creator));
        if !is_creator && !can_act {
            continue;
        }
        let request = NewNotification {
            recipient_uid: uid.clone(),
            kind: "workflow.changed".into(),
            title: format!("{} · {}", d.name, state_name(d, &action.to)),
            body: format!(
                "{} was recorded. Open the saved record to review its process and decision history.",
                action.name
            ),
            actor_uid: Some(actor.uid.clone()),
            target_type: Some("record".into()),
            target_uid: Some(record.into()),
            module_uid: Some(module.into()),
            data: json!({"event_uid":event,"record_uid":record,"module_uid":module}),
            dedup_key: Some(format!("workflow:{event}")),
            mandatory: false,
        };
        if let Some(notification) = insert_notification(c, &request)? {
            notifications.push((uid, notification));
        }
    }
    Ok(notifications)
}
fn queue(
    c: &Connection,
    module: &str,
    actor: &Claims,
    query: QueueQuery,
) -> rusqlite::Result<Value> {
    access(c, module, actor, "read")?;
    let configuration = load(c, module)?;
    let d = &configuration.definition;
    if !d.enabled {
        return Ok(json!({"enabled":false,"items":[],"has_next":false,"next_cursor":null}));
    }
    if let Some(state) = query.state.as_ref().filter(|s| !s.is_empty()) {
        if !d.states.iter().any(|s| &s.key == state) {
            return Err(fail("INVALID", "Choose an existing process stage."));
        }
    }
    let after = query.after.unwrap_or_default();
    let search = query.q.unwrap_or_default();
    if search.chars().count() > 200 {
        return Err(fail(
            "INVALID",
            "Use up to 200 characters for pipeline search.",
        ));
    }
    if after.len() > 128 {
        return Err(fail("INVALID", "The queue cursor is invalid."));
    }
    let actionable = query.actionable.unwrap_or(false);
    let editable =
        module_can_for_user_initialized(c, module, &actor.uid, actor.access_level, "update")?;
    let eligible: Vec<_> = d
        .actions
        .iter()
        .filter(|a| editable && a.access_levels.contains(&actor.access_level))
        .map(|a| json!({"from":a.from,"policy":a.creator_policy}))
        .collect();
    let state =
        "CASE WHEN ?5 AND i.record_revision<>r.revision THEN ?4 ELSE COALESCE(i.state,?4) END";
    let from=format!("FROM mx_records r
       LEFT JOIN mx_workflow_instances i ON i.record_uid=r.uid
       LEFT JOIN mx_record_owners o ON o.record_uid=r.uid
       LEFT JOIN mx_fields f ON f.module_uid=r.module_uid AND f.active=1 AND COALESCE(f.module_key,f.field_key)=?3
       LEFT JOIN mx_record_values v ON v.record_uid=r.uid AND v.field_uid=f.uid
       WHERE r.module_uid=?1 AND r.deleted_at IS NULL AND (?2='' OR r.uid>?2) AND {}
       AND {} AND (?10='' OR instr(lower(COALESCE(v.value_text,CAST(v.value_integer AS TEXT),'')),lower(?10))>0)
       AND (?6='' OR ({state})=?6)
       AND (NOT ?7 OR EXISTS(SELECT 1 FROM json_each(?8) a WHERE json_extract(a.value,'$.from')=({state}) AND
          (json_extract(a.value,'$.policy')='any' OR (json_extract(a.value,'$.policy')='creator' AND o.creator_uid=?9) OR (json_extract(a.value,'$.policy')='not_creator' AND o.creator_uid<>?9))))
       ",privacy::record_sql("r",&actor.uid,actor.access_level),privacy::field_sql("f","r",&actor.uid,actor.access_level));
    let sql = format!(
        "SELECT r.uid,COALESCE(v.value_text,CAST(v.value_integer AS TEXT),'') {from} ORDER BY r.uid LIMIT 41"
    );
    let totals = if query.counts_only.unwrap_or(false) || query.include_total.unwrap_or(false) {
        c.prepare(&format!(
            "SELECT ({state}),COUNT(*) {from} GROUP BY ({state})"
        ))?
        .query_map(
            params![
                module,
                "",
                d.title_field,
                d.initial_state,
                d.restart_on_edit,
                query.state.clone().unwrap_or_default(),
                actionable,
                json!(eligible).to_string(),
                actor.uid,
                search
            ],
            |r| Ok(json!({"state":r.get::<_,String>(0)?,"total":r.get::<_,i64>(1)?})),
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?
    } else {
        Vec::new()
    };
    let total: Option<i64> =
        if query.counts_only.unwrap_or(false) || query.include_total.unwrap_or(false) {
            Some(totals.iter().filter_map(|v| v["total"].as_i64()).sum())
        } else {
            None
        };
    if query.counts_only.unwrap_or(false) {
        return Ok(
            json!({"enabled":true,"name":d.name,"states":d.states,"configuration_revision":configuration.revision,"items":[],"total":total,"stage_totals":totals,"has_next":false,"next_cursor":null}),
        );
    }
    let rows = c
        .prepare(&sql)?
        .query_map(
            params![
                module,
                after,
                d.title_field,
                d.initial_state,
                d.restart_on_edit,
                query.state.unwrap_or_default(),
                actionable,
                json!(eligible).to_string(),
                actor.uid,
                search
            ],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let has_next = rows.len() > 40;
    let mut items = Vec::new();
    for (uid, title) in rows.into_iter().take(40) {
        let mut item = status(c, module, &uid, actor)?;
        item["uid"] = json!(uid);
        item["title"] = json!(if title.trim().is_empty() {
            format!("Record · {}", uid.chars().take(8).collect::<String>())
        } else {
            title.chars().take(200).collect::<String>()
        });
        items.push(item);
    }
    Ok(
        json!({"enabled":true,"name":d.name,"states":d.states,"configuration_revision":configuration.revision,"items":items,"total":total,"has_next":has_next,"next_cursor":if has_next {items.last().map(|i|i["uid"].clone())}else{None}}),
    )
}
fn history(
    c: &Connection,
    module: &str,
    record: &str,
    actor: &Claims,
    before: i64,
) -> rusqlite::Result<Value> {
    record_access(c, module, record, actor)?;
    let mut items=c.prepare("SELECT e.uid,e.sequence,e.action_name,e.from_name,e.to_name,e.actor_uid,COALESCE(u.name,'Former account'),e.note,e.record_revision,e.snapshot_uid,e.created_at,e.configuration_revision FROM mx_workflow_events e LEFT JOIN users u ON u.uid=e.actor_uid WHERE e.record_uid=?1 AND e.sequence<?2 ORDER BY e.sequence DESC LIMIT 41")?.query_map(params![record,before],|r|Ok(json!({"uid":r.get::<_,String>(0)?,"sequence":r.get::<_,i64>(1)?,"action_name":r.get::<_,String>(2)?,"from_name":r.get::<_,String>(3)?,"to_name":r.get::<_,String>(4)?,"actor_uid":r.get::<_,String>(5)?,"actor_name":r.get::<_,String>(6)?,"note":r.get::<_,String>(7)?,"record_revision":r.get::<_,i64>(8)?,"snapshot_uid":r.get::<_,String>(9)?,"created_at":r.get::<_,i64>(10)?,"configuration_revision":r.get::<_,i64>(11)?})))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let has_next = items.len() > 40;
    items.truncate(40);
    Ok(
        json!({"items":items,"has_next":has_next,"next_cursor":if has_next {items.last().map(|i|i["sequence"].clone())}else{None}}),
    )
}

fn response(result: Result<Value, SqliteDatabaseError>) -> Response {
    match result {
        Ok(v) => Json(v).into_response(),
        Err(SqliteDatabaseError::Sqlite(rusqlite::Error::QueryReturnedNoRows)) => (
            StatusCode::NOT_FOUND,
            Json(json!({"response":"This record is unavailable or you do not have access."})),
        )
            .into_response(),
        Err(SqliteDatabaseError::Sqlite(rusqlite::Error::InvalidParameterName(message))) => {
            let status = if message.starts_with("MX_WORKFLOW_FORBIDDEN:") {
                StatusCode::FORBIDDEN
            } else if message.starts_with("MX_WORKFLOW_CONFLICT:")
                || message.starts_with("MX_OPERATION:")
            {
                StatusCode::CONFLICT
            } else {
                StatusCode::BAD_REQUEST
            };
            (
                status,
                Json(json!({"response":message.split_once(':').map(|(_,m)|m).unwrap_or(&message)})),
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("Module process database operation failed: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR,Json(json!({"response":"MX could not complete the process operation. Reload its status before retrying."}))).into_response()
        }
    }
}
async fn run(
    task: impl FnOnce(&Connection) -> rusqlite::Result<Value> + Send + 'static,
) -> Response {
    match tokio::task::spawn_blocking(move || with_sql_connection(task)).await {
        Ok(r) => response(r),
        Err(e) => {
            tracing::error!("Module process task failed: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR,Json(json!({"response":"MX could not complete the process task. Reload before retrying."}))).into_response()
        }
    }
}
async fn get_config(actor: Claims, Path(module): Path<String>) -> Response {
    run(move |c| read_configuration(c, &module, &actor)).await
}
async fn put_config(
    actor: Claims,
    Path(module): Path<String>,
    Json(request): Json<SaveConfiguration>,
) -> Response {
    let module_event = module.clone();
    let actor_event = actor.uid.clone();
    let result = run(move |c| save(c, &module, &actor, request)).await;
    if result.status().is_success() {
        crate::api::live::publish_live_event(
            "module.workflow.updated",
            Some(&actor_event),
            json!({"module_uid":module_event}),
        );
    }
    result
}
async fn get_status(actor: Claims, Path((module, record)): Path<(String, String)>) -> Response {
    run(move |c| {
        let tx = c.unchecked_transaction()?;
        let result = status(&tx, &module, &record, &actor)?;
        tx.commit()?;
        Ok(result)
    })
    .await
}
async fn get_queue(
    actor: Claims,
    Path(module): Path<String>,
    Query(query): Query<QueueQuery>,
) -> Response {
    run(move |c| {
        let tx = c.unchecked_transaction()?;
        let result = queue(&tx, &module, &actor, query)?;
        tx.commit()?;
        Ok(result)
    })
    .await
}
async fn get_history(
    actor: Claims,
    Path((module, record)): Path<(String, String)>,
    Query(query): Query<HistoryQuery>,
) -> Response {
    run(move |c| {
        let tx = c.unchecked_transaction()?;
        let result = history(
            &tx,
            &module,
            &record,
            &actor,
            query.before.unwrap_or(i64::MAX),
        )?;
        tx.commit()?;
        Ok(result)
    })
    .await
}
async fn take_action(
    actor: Claims,
    Path((module, record)): Path<(String, String)>,
    Json(request): Json<Execute>,
) -> Response {
    let module_event = module.clone();
    let record_event = record.clone();
    let actor_event = actor.uid.clone();
    let result = run(move |c| execute(c, &module, &record, &actor, &request)).await;
    if result.status().is_success() {
        crate::api::live::publish_live_event(
            "record.workflow.updated",
            Some(&actor_event),
            json!({"module_uid":module_event,"record_uid":record_event,"requires_refresh":true}),
        );
    }
    result
}
pub fn routes() -> Router {
    Router::new()
        .route(
            "/mx/v1/modules/{module}/workflow",
            get(get_config).put(put_config),
        )
        .route("/mx/v1/modules/{module}/work-queue", get(get_queue))
        .route(
            "/mx/v1/modules/{module}/records/{record}/workflow",
            get(get_status),
        )
        .route(
            "/mx/v1/modules/{module}/records/{record}/workflow/history",
            get(get_history),
        )
        .route(
            "/mx/v1/modules/{module}/records/{record}/workflow/actions",
            post(take_action),
        )
        .layer(axum::middleware::from_fn(auth))
}

#[cfg(test)]
#[path = "workflow_tests.rs"]
mod tests;
