use std::collections::{BTreeMap, HashMap};

use axum::{
    Json,
    extract::{Path, Query},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use rusqlite::{OptionalExtension, params, params_from_iter, types::Value as SqlValue};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    api::{
        live::publish_live_event,
        mx::{
            attachment_fields::ensure_attachment_fields_schema,
            model::FileAttachment,
            schema::{
                FieldDefinition, ensure_dynamic_schema, field_map_by_key, load_active_schema,
                load_fields_db,
            },
        },
    },
    db::connector::{SqliteDatabaseError, with_sql_connection},
    middleware::auth::Claims,
};

const DEFAULT_PAGE_LIMIT: usize = 256;
const MAX_PAGE_LIMIT: usize = 256;

#[derive(Debug, Deserialize, Clone, Default)]
pub struct DynamicListQuery {
    pub page: Option<usize>,
    pub limit: Option<usize>,
    pub q: Option<String>,

    #[serde(rename = "match")]
    pub match_mode: Option<String>,

    // JSON object: {"field_key":"value", ...}
    pub filters: Option<String>,

    pub attachments: Option<String>,
    pub sort_by: Option<String>,
    pub sort_dir: Option<String>,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct DynamicRecordRequest {
    #[serde(default)]
    pub values: BTreeMap<String, Value>,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct DynamicPatchRequest {
    #[serde(default)]
    pub changes: BTreeMap<String, Value>,

    #[serde(default)]
    pub base_revisions: BTreeMap<String, i64>,
}

#[derive(Debug, Serialize, Clone)]
pub struct FieldConflict {
    pub field_uid: String,
    pub field_key: String,
    pub label: String,
    pub base_revision: i64,
    pub current_revision: i64,
    pub current_value: Value,
    pub your_value: Value,
}

#[derive(Debug, Serialize, Clone)]
struct PatchOutcome {
    changed_values: BTreeMap<String, Value>,
    field_revisions: BTreeMap<String, i64>,
}

#[derive(Debug)]
enum PatchDbResult {
    Applied(PatchOutcome),
    Conflict(Vec<FieldConflict>),
    NotFound,
}

#[derive(Debug, Serialize, Clone)]
pub struct DynamicRecord {
    pub uid: String,
    pub values: BTreeMap<String, Value>,
    pub field_revisions: BTreeMap<String, i64>,
    pub attached_files: Vec<FileAttachment>,
}

#[derive(Debug, Serialize)]
pub struct DynamicPage {
    pub data: Vec<DynamicRecord>,
    pub page: usize,
    pub limit: usize,
    pub has_next: bool,
    pub total: usize,
    pub total_pages: usize,
}

#[derive(Debug, Clone)]
enum NormalizedValue {
    Text(String),
    Integer(i64),
    Real(f64),
    Boolean(bool),
}

impl NormalizedValue {
    fn normalized_unique(&self) -> String {
        match self {
            Self::Text(value) => value.trim().to_ascii_lowercase(),
            Self::Integer(value) => value.to_string(),
            Self::Real(value) => format!("{value:.12}"),
            Self::Boolean(value) => {
                if *value {
                    "1".to_string()
                } else {
                    "0".to_string()
                }
            }
        }
    }
}

fn api_json(status: StatusCode, body: Value) -> Response {
    (status, Json(body)).into_response()
}

fn access_denied() -> Response {
    api_json(
        StatusCode::FORBIDDEN,
        json!({
            "response": "your account does not have permission for this MX operation"
        }),
    )
}

fn normalize_match_mode(value: Option<&str>) -> &'static str {
    match value
        .unwrap_or("contains")
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "exact" => "exact",
        "prefix" => "prefix",
        _ => "contains",
    }
}

fn escape_like(value: &str) -> String {
    /*
     * Use one explicit SQLite LIKE escape character.
     *
     * SQLite does not interpret backslash escapes in ordinary SQL string
     * literals, so using two backslashes inside ESCAPE produces a two-
     * character string and fails with:
     *
     *     ESCAPE expression must be a single character
     *
     * Escape ! first, then LIKE wildcards, so literal searches for !, %, and
     * _ remain correct.
     */
    value
        .replace('!', "!!")
        .replace('%', "!%")
        .replace('_', "!_")
}

fn search_pattern(value: &str, match_mode: &str) -> String {
    match match_mode {
        "exact" => escape_like(value),
        "prefix" => format!("{}%", escape_like(value)),
        _ => format!("%{}%", escape_like(value)),
    }
}

fn parse_date(value: &str) -> bool {
    let bytes = value.as_bytes();

    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes[0..4].iter().all(|byte| byte.is_ascii_digit())
        && bytes[5..7].iter().all(|byte| byte.is_ascii_digit())
        && bytes[8..10].iter().all(|byte| byte.is_ascii_digit())
}

fn is_empty_json(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::String(value) => value.trim().is_empty(),
        _ => false,
    }
}

fn normalize_field_value(
    field: &FieldDefinition,
    value: &Value,
) -> Result<Option<NormalizedValue>, String> {
    if is_empty_json(value) {
        return Ok(None);
    }

    match field.field_type.as_str() {
        "text" | "long_text" => {
            let value = value
                .as_str()
                .ok_or_else(|| format!("{} must be text.", field.label))?
                .trim()
                .to_string();

            if value.is_empty() {
                Ok(None)
            } else {
                Ok(Some(NormalizedValue::Text(value)))
            }
        }

        "date" => {
            let value = value
                .as_str()
                .ok_or_else(|| format!("{} must be a date.", field.label))?
                .trim()
                .to_string();

            if !parse_date(&value) {
                return Err(format!("{} must use YYYY-MM-DD format.", field.label));
            }

            Ok(Some(NormalizedValue::Text(value)))
        }

        "select" => {
            let value = value
                .as_str()
                .ok_or_else(|| format!("{} must be a selected value.", field.label))?
                .trim()
                .to_string();

            let options = field
                .config
                .get("options")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();

            let valid = options.iter().any(|option| {
                option
                    .as_str()
                    .is_some_and(|option| option.eq_ignore_ascii_case(&value))
            });

            if !valid {
                return Err(format!(
                    "{} contains a value that is not in its configured options.",
                    field.label
                ));
            }

            Ok(Some(NormalizedValue::Text(value)))
        }

        "integer" => {
            let integer = if let Some(value) = value.as_i64() {
                value
            } else if let Some(value) = value.as_str() {
                value
                    .trim()
                    .parse::<i64>()
                    .map_err(|_| format!("{} must be an integer.", field.label))?
            } else {
                return Err(format!("{} must be an integer.", field.label));
            };

            Ok(Some(NormalizedValue::Integer(integer)))
        }

        "decimal" => {
            let real = if let Some(value) = value.as_f64() {
                value
            } else if let Some(value) = value.as_str() {
                value
                    .trim()
                    .parse::<f64>()
                    .map_err(|_| format!("{} must be a number.", field.label))?
            } else {
                return Err(format!("{} must be a number.", field.label));
            };

            if !real.is_finite() {
                return Err(format!("{} must be a finite number.", field.label));
            }

            Ok(Some(NormalizedValue::Real(real)))
        }

        "boolean" => {
            let boolean = if let Some(value) = value.as_bool() {
                value
            } else if let Some(value) = value.as_str() {
                match value.trim().to_ascii_lowercase().as_str() {
                    "true" | "1" | "yes" | "on" => true,
                    "false" | "0" | "no" | "off" => false,
                    _ => {
                        return Err(format!("{} must be true or false.", field.label));
                    }
                }
            } else {
                return Err(format!("{} must be true or false.", field.label));
            };

            Ok(Some(NormalizedValue::Boolean(boolean)))
        }

        "auto_number" | "attachments" => Ok(None),

        _ => Err(format!(
            "{} has unsupported type '{}'.",
            field.label, field.field_type
        )),
    }
}

fn validate_payload(
    fields: &[FieldDefinition],
    raw_values: &BTreeMap<String, Value>,
) -> Result<BTreeMap<String, NormalizedValue>, String> {
    let field_map = field_map_by_key(fields);

    for key in raw_values.keys() {
        if !field_map.contains_key(key) {
            return Err(format!(
                "'{key}' is not an active MX record field. Refresh the page and try again."
            ));
        }
    }

    let mut normalized = BTreeMap::new();

    for field in fields {
        if matches!(field.field_type.as_str(), "auto_number" | "attachments") {
            continue;
        }

        let value = raw_values.get(&field.key).unwrap_or(&Value::Null);
        let normalized_value = normalize_field_value(field, value)?;

        if field.required && normalized_value.is_none() {
            return Err(format!("{} is required.", field.label));
        }

        if let Some(value) = normalized_value {
            normalized.insert(field.key.clone(), value);
        }
    }

    Ok(normalized)
}

fn validate_patch_payload(
    fields: &[FieldDefinition],
    raw_changes: &BTreeMap<String, Value>,
    base_revisions: &BTreeMap<String, i64>,
) -> Result<BTreeMap<String, Option<NormalizedValue>>, String> {
    if raw_changes.is_empty() {
        return Ok(BTreeMap::new());
    }

    let field_map = field_map_by_key(fields);
    let mut normalized = BTreeMap::new();

    for (key, raw_value) in raw_changes {
        let Some(field) = field_map.get(key) else {
            return Err(format!(
                "'{key}' is not an active MX record field. Refresh the page and try again."
            ));
        };

        if matches!(field.field_type.as_str(), "auto_number" | "attachments") {
            return Err(format!(
                "{} cannot be modified through the record field PATCH API.",
                field.label
            ));
        }

        let Some(base_revision) = base_revisions.get(key) else {
            return Err(format!(
                "Missing base revision for {}. Refresh this record and try again.",
                field.label
            ));
        };

        if *base_revision < 0 {
            return Err(format!("Invalid base revision for {}.", field.label));
        }

        let value = normalize_field_value(field, raw_value)?;
        if field.required && value.is_none() {
            return Err(format!("{} is required.", field.label));
        }

        normalized.insert(key.clone(), value);
    }

    Ok(normalized)
}

fn scope_key_for_auto_number(
    transaction: &rusqlite::Transaction<'_>,
    field: &FieldDefinition,
) -> rusqlite::Result<String> {
    let scope = field
        .config
        .get("scope")
        .and_then(Value::as_str)
        .unwrap_or("global");

    if scope == "yearly" {
        transaction.query_row("SELECT strftime('%Y', 'now')", [], |row| {
            row.get::<_, String>(0)
        })
    } else {
        Ok("*".to_string())
    }
}

fn next_auto_number(
    transaction: &rusqlite::Transaction<'_>,
    field: &FieldDefinition,
) -> rusqlite::Result<i64> {
    let scope_key = scope_key_for_auto_number(transaction, field)?;

    transaction.execute(
        r#"
        INSERT OR IGNORE INTO mx_field_sequences (
            field_uid,
            scope_key,
            last_value
        ) VALUES (?1, ?2, 0)
        "#,
        params![&field.uid, &scope_key],
    )?;

    transaction.execute(
        r#"
        UPDATE mx_field_sequences
        SET last_value = last_value + 1
        WHERE field_uid = ?1
          AND scope_key = ?2
        "#,
        params![&field.uid, &scope_key],
    )?;

    transaction.query_row(
        r#"
        SELECT last_value
        FROM mx_field_sequences
        WHERE field_uid = ?1
          AND scope_key = ?2
        "#,
        params![&field.uid, &scope_key],
        |row| row.get::<_, i64>(0),
    )
}

fn load_existing_auto_numbers(
    transaction: &rusqlite::Transaction<'_>,
    record_uid: &str,
    fields: &[FieldDefinition],
) -> rusqlite::Result<BTreeMap<String, NormalizedValue>> {
    let mut values = BTreeMap::new();

    for field in fields
        .iter()
        .filter(|field| field.field_type == "auto_number")
    {
        let value = transaction
            .query_row(
                r#"
                SELECT value_integer
                FROM mx_record_values
                WHERE record_uid = ?1
                  AND field_uid = ?2
                "#,
                params![record_uid, &field.uid],
                |row| row.get::<_, Option<i64>>(0),
            )
            .optional()?
            .flatten();

        if let Some(value) = value {
            values.insert(field.key.clone(), NormalizedValue::Integer(value));
        }
    }

    Ok(values)
}

fn fill_auto_numbers(
    transaction: &rusqlite::Transaction<'_>,
    fields: &[FieldDefinition],
    values: &mut BTreeMap<String, NormalizedValue>,
) -> rusqlite::Result<()> {
    for field in fields
        .iter()
        .filter(|field| field.field_type == "auto_number")
    {
        if values.contains_key(&field.key) {
            continue;
        }

        let next = next_auto_number(transaction, field)?;
        values.insert(field.key.clone(), NormalizedValue::Integer(next));
    }

    Ok(())
}

fn insert_record_value(
    transaction: &rusqlite::Transaction<'_>,
    record_uid: &str,
    field: &FieldDefinition,
    value: &NormalizedValue,
) -> rusqlite::Result<()> {
    let (text, integer, real, boolean): (Option<String>, Option<i64>, Option<f64>, Option<i64>) =
        match value {
            NormalizedValue::Text(value) => (Some(value.clone()), None, None, None),
            NormalizedValue::Integer(value) => (None, Some(*value), None, None),
            NormalizedValue::Real(value) => (None, None, Some(*value), None),
            NormalizedValue::Boolean(value) => (None, None, None, Some(if *value { 1 } else { 0 })),
        };

    transaction.execute(
        r#"
        INSERT INTO mx_record_values (
            record_uid,
            field_uid,
            value_text,
            value_integer,
            value_real,
            value_boolean
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        "#,
        params![record_uid, &field.uid, text, integer, real, boolean,],
    )?;

    Ok(())
}

fn write_dynamic_values(
    transaction: &rusqlite::Transaction<'_>,
    record_uid: &str,
    fields: &[FieldDefinition],
    values: &BTreeMap<String, NormalizedValue>,
) -> rusqlite::Result<()> {
    transaction.execute(
        "DELETE FROM mx_unique_values WHERE record_uid = ?1",
        params![record_uid],
    )?;

    for field in fields {
        transaction.execute(
            r#"
            DELETE FROM mx_record_values
            WHERE record_uid = ?1
              AND field_uid = ?2
            "#,
            params![record_uid, &field.uid],
        )?;

        let Some(value) = values.get(&field.key) else {
            continue;
        };

        insert_record_value(transaction, record_uid, field, value)?;

        if field.unique_value && field.field_type != "auto_number" {
            let normalized = value.normalized_unique();

            if !normalized.is_empty() {
                transaction.execute(
                    r#"
                    INSERT INTO mx_unique_values (
                        field_uid,
                        normalized_value,
                        record_uid
                    ) VALUES (?1, ?2, ?3)
                    "#,
                    params![&field.uid, normalized, record_uid],
                )?;
            }
        }
    }

    Ok(())
}

pub(crate) fn ensure_record_collaboration_schema(
    connection: &rusqlite::Connection,
) -> rusqlite::Result<()> {
    connection.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS mx_record_field_revisions (
            record_uid   TEXT NOT NULL,
            field_uid    TEXT NOT NULL,
            revision     INTEGER NOT NULL DEFAULT 1 CHECK(revision >= 0),
            updated_at   INTEGER NOT NULL DEFAULT 0,
            updated_by   TEXT,
            PRIMARY KEY (record_uid, field_uid)
        );

        CREATE INDEX IF NOT EXISTS idx_mx_record_field_revisions_record
        ON mx_record_field_revisions(record_uid);
        "#,
    )
}

fn normalized_to_json(value: &NormalizedValue) -> Value {
    match value {
        NormalizedValue::Text(value) => json!(value),
        NormalizedValue::Integer(value) => json!(value),
        NormalizedValue::Real(value) => json!(value),
        NormalizedValue::Boolean(value) => json!(value),
    }
}

fn normalized_to_legacy_text(value: Option<&NormalizedValue>) -> String {
    match value {
        Some(NormalizedValue::Text(value)) => value.clone(),
        Some(NormalizedValue::Integer(value)) => value.to_string(),
        Some(NormalizedValue::Real(value)) => value.to_string(),
        Some(NormalizedValue::Boolean(value)) => value.to_string(),
        None => String::new(),
    }
}

fn current_field_revision(
    transaction: &rusqlite::Transaction<'_>,
    record_uid: &str,
    field_uid: &str,
) -> rusqlite::Result<i64> {
    if let Some(revision) = transaction
        .query_row(
            r#"
            SELECT revision
            FROM mx_record_field_revisions
            WHERE record_uid = ?1 AND field_uid = ?2
            "#,
            params![record_uid, field_uid],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
    {
        return Ok(revision.max(0));
    }

    let has_value = transaction.query_row(
        r#"
        SELECT EXISTS(
            SELECT 1
            FROM mx_record_values
            WHERE record_uid = ?1 AND field_uid = ?2
        )
        "#,
        params![record_uid, field_uid],
        |row| row.get::<_, i64>(0),
    )? != 0;

    // Legacy values pre-date per-field revisions. Treat an existing legacy
    // value as revision 1 so the first modern PATCH cannot silently overwrite
    // a value opened by another user.
    Ok(if has_value { 1 } else { 0 })
}

fn current_field_value(
    transaction: &rusqlite::Transaction<'_>,
    record_uid: &str,
    field: &FieldDefinition,
) -> rusqlite::Result<Value> {
    let row = transaction
        .query_row(
            r#"
            SELECT value_text, value_integer, value_real, value_boolean
            FROM mx_record_values
            WHERE record_uid = ?1 AND field_uid = ?2
            LIMIT 1
            "#,
            params![record_uid, &field.uid],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<i64>>(1)?,
                    row.get::<_, Option<f64>>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                ))
            },
        )
        .optional()?;

    Ok(match row {
        Some((text, integer, real, boolean)) => {
            value_from_row(&field.field_type, text, integer, real, boolean)
        }
        None => Value::Null,
    })
}

fn set_field_revision(
    transaction: &rusqlite::Transaction<'_>,
    record_uid: &str,
    field_uid: &str,
    revision: i64,
    actor_uid: &str,
) -> rusqlite::Result<()> {
    transaction.execute(
        r#"
        INSERT INTO mx_record_field_revisions (
            record_uid,
            field_uid,
            revision,
            updated_at,
            updated_by
        ) VALUES (
            ?1, ?2, ?3,
            CAST(STRFTIME('%s','now') AS INTEGER) * 1000,
            ?4
        )
        ON CONFLICT(record_uid, field_uid) DO UPDATE SET
            revision = excluded.revision,
            updated_at = excluded.updated_at,
            updated_by = excluded.updated_by
        "#,
        params![record_uid, field_uid, revision.max(0), actor_uid],
    )?;

    Ok(())
}

fn initialize_field_revisions(
    transaction: &rusqlite::Transaction<'_>,
    record_uid: &str,
    fields: &[FieldDefinition],
    values: &BTreeMap<String, NormalizedValue>,
    actor_uid: &str,
) -> rusqlite::Result<()> {
    for field in fields {
        if values.contains_key(&field.key) {
            set_field_revision(transaction, record_uid, &field.uid, 1, actor_uid)?;
        }
    }

    Ok(())
}

fn bump_all_field_revisions(
    transaction: &rusqlite::Transaction<'_>,
    record_uid: &str,
    fields: &[FieldDefinition],
    actor_uid: &str,
) -> rusqlite::Result<()> {
    for field in fields
        .iter()
        .filter(|field| field.field_type != "attachments")
    {
        let current = current_field_revision(transaction, record_uid, &field.uid)?;
        set_field_revision(
            transaction,
            record_uid,
            &field.uid,
            current.saturating_add(1),
            actor_uid,
        )?;
    }

    Ok(())
}

fn write_single_dynamic_value(
    transaction: &rusqlite::Transaction<'_>,
    record_uid: &str,
    field: &FieldDefinition,
    value: Option<&NormalizedValue>,
) -> rusqlite::Result<()> {
    transaction.execute(
        r#"
        DELETE FROM mx_unique_values
        WHERE record_uid = ?1 AND field_uid = ?2
        "#,
        params![record_uid, &field.uid],
    )?;

    transaction.execute(
        r#"
        DELETE FROM mx_record_values
        WHERE record_uid = ?1 AND field_uid = ?2
        "#,
        params![record_uid, &field.uid],
    )?;

    let Some(value) = value else {
        return Ok(());
    };

    insert_record_value(transaction, record_uid, field, value)?;

    if field.unique_value && field.field_type != "auto_number" {
        let normalized = value.normalized_unique();
        if !normalized.is_empty() {
            transaction.execute(
                r#"
                INSERT INTO mx_unique_values (
                    field_uid,
                    normalized_value,
                    record_uid
                ) VALUES (?1, ?2, ?3)
                "#,
                params![&field.uid, normalized, record_uid],
            )?;
        }
    }

    Ok(())
}

fn update_legacy_mirror_field(
    transaction: &rusqlite::Transaction<'_>,
    record_uid: &str,
    field_key: &str,
    value: Option<&NormalizedValue>,
) -> rusqlite::Result<()> {
    let value = normalized_to_legacy_text(value);
    let column = match field_key {
        "date" => Some("date"),
        "office" => Some("office"),
        "requestor" => Some("requestor"),
        "subject" => Some("subject"),
        "routed_to_div" => Some("routed_to_div"),
        "remarks" => Some("remarks"),
        _ => None,
    };

    if let Some(column) = column {
        let sql = format!("UPDATE mx_records SET {column} = ?2 WHERE uid = ?1");
        transaction.execute(&sql, params![record_uid, value])?;
    }

    Ok(())
}

fn load_field_revisions(
    connection: &rusqlite::Connection,
    record_uids: &[String],
) -> rusqlite::Result<HashMap<String, BTreeMap<String, i64>>> {
    ensure_record_collaboration_schema(connection)?;

    if record_uids.is_empty() {
        return Ok(HashMap::new());
    }

    let placeholders = std::iter::repeat("?")
        .take(record_uids.len())
        .collect::<Vec<_>>()
        .join(",");
    let params = record_uids
        .iter()
        .cloned()
        .map(SqlValue::Text)
        .collect::<Vec<_>>();

    // First seed legacy values with revision 1.
    let mut revisions: HashMap<String, BTreeMap<String, i64>> = HashMap::new();
    let sql = format!(
        r#"
        SELECT rv.record_uid, f.field_key
        FROM mx_record_values rv
        JOIN mx_fields f ON f.uid = rv.field_uid
        WHERE rv.record_uid IN ({placeholders})
        "#
    );
    let mut statement = connection.prepare(&sql)?;
    let mut rows = statement.query(params_from_iter(params.iter()))?;
    while let Some(row) = rows.next()? {
        let record_uid: String = row.get(0)?;
        let key: String = row.get(1)?;
        revisions.entry(record_uid).or_default().insert(key, 1);
    }

    // Modern revision rows override the legacy seed and remain present even if
    // the field value was cleared.
    let sql = format!(
        r#"
        SELECT r.record_uid, f.field_key, r.revision
        FROM mx_record_field_revisions r
        JOIN mx_fields f ON f.uid = r.field_uid
        WHERE r.record_uid IN ({placeholders})
        "#
    );
    let mut statement = connection.prepare(&sql)?;
    let mut rows = statement.query(params_from_iter(params.iter()))?;
    while let Some(row) = rows.next()? {
        let record_uid: String = row.get(0)?;
        let key: String = row.get(1)?;
        let revision: i64 = row.get(2)?;
        revisions
            .entry(record_uid)
            .or_default()
            .insert(key, revision.max(0));
    }

    Ok(revisions)
}

fn patch_record_db(
    uid: String,
    changes: BTreeMap<String, Option<NormalizedValue>>,
    raw_changes: BTreeMap<String, Value>,
    base_revisions: BTreeMap<String, i64>,
    actor_uid: String,
) -> Result<PatchDbResult, SqliteDatabaseError> {
    with_sql_connection(|connection| {
        ensure_dynamic_schema(connection)?;
        ensure_record_collaboration_schema(connection)?;

        let fields = load_fields_db(connection, false)?;
        let field_map = field_map_by_key(&fields);
        let transaction = connection.unchecked_transaction()?;

        let exists = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM mx_records WHERE uid = ?1)",
            params![&uid],
            |row| row.get::<_, i64>(0),
        )? != 0;

        if !exists {
            return Ok(PatchDbResult::NotFound);
        }

        let mut conflicts = Vec::new();
        let mut current_revisions = BTreeMap::new();

        for key in changes.keys() {
            let Some(field) = field_map.get(key) else {
                continue;
            };

            let current_revision = current_field_revision(&transaction, &uid, &field.uid)?;
            current_revisions.insert(key.clone(), current_revision);

            let base_revision = base_revisions.get(key).copied().unwrap_or(-1);
            if base_revision != current_revision {
                conflicts.push(FieldConflict {
                    field_uid: field.uid.clone(),
                    field_key: field.key.clone(),
                    label: field.label.clone(),
                    base_revision,
                    current_revision,
                    current_value: current_field_value(&transaction, &uid, field)?,
                    your_value: raw_changes.get(key).cloned().unwrap_or(Value::Null),
                });
            }
        }

        if !conflicts.is_empty() {
            return Ok(PatchDbResult::Conflict(conflicts));
        }

        let mut changed_values = BTreeMap::new();
        let mut new_revisions = BTreeMap::new();

        for (key, value) in &changes {
            let Some(field) = field_map.get(key) else {
                continue;
            };

            write_single_dynamic_value(&transaction, &uid, field, value.as_ref())?;
            update_legacy_mirror_field(&transaction, &uid, key, value.as_ref())?;

            let next_revision = current_revisions
                .get(key)
                .copied()
                .unwrap_or(0)
                .saturating_add(1);
            set_field_revision(&transaction, &uid, &field.uid, next_revision, &actor_uid)?;

            changed_values.insert(
                key.clone(),
                value
                    .as_ref()
                    .map(normalized_to_json)
                    .unwrap_or(Value::Null),
            );
            new_revisions.insert(key.clone(), next_revision);
        }

        transaction.commit()?;

        Ok(PatchDbResult::Applied(PatchOutcome {
            changed_values,
            field_revisions: new_revisions,
        }))
    })
}

fn value_as_text(values: &BTreeMap<String, NormalizedValue>, key: &str, fallback: &str) -> String {
    match values.get(key) {
        Some(NormalizedValue::Text(value)) => value.clone(),
        Some(NormalizedValue::Integer(value)) => value.to_string(),
        Some(NormalizedValue::Real(value)) => value.to_string(),
        Some(NormalizedValue::Boolean(value)) => value.to_string(),
        None => fallback.to_string(),
    }
}

fn value_as_i64(values: &BTreeMap<String, NormalizedValue>, key: &str) -> Option<i64> {
    match values.get(key) {
        Some(NormalizedValue::Integer(value)) => Some(*value),
        _ => None,
    }
}

fn current_year(transaction: &rusqlite::Transaction<'_>) -> rusqlite::Result<i32> {
    let year: String =
        transaction.query_row("SELECT strftime('%Y', 'now')", [], |row| row.get(0))?;

    Ok(year.parse::<i32>().unwrap_or(0))
}

fn year_from_date(value: &str) -> Option<i32> {
    if !parse_date(value) {
        return None;
    }

    value.get(0..4)?.parse::<i32>().ok()
}

fn next_hidden_legacy_number(transaction: &rusqlite::Transaction<'_>) -> rusqlite::Result<i64> {
    transaction.execute(
        r#"
        INSERT OR IGNORE INTO mx_control_sequence (
            year,
            last_value
        ) VALUES (0, 0)
        "#,
        [],
    )?;

    transaction.execute(
        r#"
        UPDATE mx_control_sequence
        SET last_value = last_value + 1
        WHERE year = 0
        "#,
        [],
    )?;

    transaction.query_row(
        "SELECT last_value FROM mx_control_sequence WHERE year = 0",
        [],
        |row| row.get::<_, i64>(0),
    )
}

fn create_record_db(
    request_values: BTreeMap<String, NormalizedValue>,
    actor_uid: String,
) -> Result<String, SqliteDatabaseError> {
    with_sql_connection(|connection| {
        ensure_dynamic_schema(connection)?;
        ensure_record_collaboration_schema(connection)?;
        let fields = load_fields_db(connection, false)?;
        let transaction = connection.unchecked_transaction()?;
        let uid = Uuid::new_v4().to_string();
        let mut values = request_values;

        fill_auto_numbers(&transaction, &fields, &mut values)?;

        let date = value_as_text(&values, "date", "");
        let office = value_as_text(&values, "office", "");
        let requestor = value_as_text(&values, "requestor", "");
        let subject = value_as_text(&values, "subject", "");
        let routed_to_div = value_as_text(&values, "routed_to_div", "");
        let remarks = value_as_text(&values, "remarks", "");

        let configured_control = value_as_i64(&values, "control_no");

        let (control_year, control_no) = match configured_control {
            Some(value) if value > 0 => {
                let year = year_from_date(&date).unwrap_or(current_year(&transaction)?);

                (year, value)
            }

            _ => (0, next_hidden_legacy_number(&transaction)?),
        };

        transaction.execute(
            r#"
            INSERT INTO mx_records (
                uid,
                control_year,
                control_no,
                date,
                office,
                requestor,
                subject,
                routed_to_div,
                remarks
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9
            )
            "#,
            params![
                &uid,
                control_year,
                control_no,
                date,
                office,
                requestor,
                subject,
                routed_to_div,
                remarks,
            ],
        )?;

        write_dynamic_values(&transaction, &uid, &fields, &values)?;
        initialize_field_revisions(&transaction, &uid, &fields, &values, &actor_uid)?;
        transaction.commit()?;

        Ok(uid)
    })
}

fn update_record_db(
    uid: String,
    request_values: BTreeMap<String, NormalizedValue>,
    actor_uid: String,
) -> Result<(), SqliteDatabaseError> {
    with_sql_connection(|connection| {
        ensure_dynamic_schema(connection)?;
        ensure_record_collaboration_schema(connection)?;
        let fields = load_fields_db(connection, false)?;
        let transaction = connection.unchecked_transaction()?;

        let existing = transaction.query_row(
            r#"
                SELECT
                    date,
                    office,
                    requestor,
                    subject,
                    routed_to_div,
                    remarks
                FROM mx_records
                WHERE uid = ?1
                "#,
            params![&uid],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        )?;

        let mut values = request_values;
        let existing_auto = load_existing_auto_numbers(&transaction, &uid, &fields)?;

        for (key, value) in existing_auto {
            values.insert(key, value);
        }

        // If a new auto-number field was added after this record existed,
        // assign it on the first edit of the historical record.
        fill_auto_numbers(&transaction, &fields, &mut values)?;

        let field_map = field_map_by_key(&fields);

        let date = if field_map.contains_key("date") {
            value_as_text(&values, "date", "")
        } else {
            existing.0
        };

        let office = if field_map.contains_key("office") {
            value_as_text(&values, "office", "")
        } else {
            existing.1
        };

        let requestor = if field_map.contains_key("requestor") {
            value_as_text(&values, "requestor", "")
        } else {
            existing.2
        };

        let subject = if field_map.contains_key("subject") {
            value_as_text(&values, "subject", "")
        } else {
            existing.3
        };

        let routed_to_div = if field_map.contains_key("routed_to_div") {
            value_as_text(&values, "routed_to_div", "")
        } else {
            existing.4
        };

        let remarks = if field_map.contains_key("remarks") {
            value_as_text(&values, "remarks", "")
        } else {
            existing.5
        };

        let affected = transaction.execute(
            r#"
            UPDATE mx_records
            SET
                date = ?2,
                office = ?3,
                requestor = ?4,
                subject = ?5,
                routed_to_div = ?6,
                remarks = ?7
            WHERE uid = ?1
            "#,
            params![
                &uid,
                date,
                office,
                requestor,
                subject,
                routed_to_div,
                remarks,
            ],
        )?;

        if affected == 0 {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }

        write_dynamic_values(&transaction, &uid, &fields, &values)?;
        bump_all_field_revisions(&transaction, &uid, &fields, &actor_uid)?;
        transaction.commit()?;

        Ok(())
    })
}

fn value_from_row(
    field_type: &str,
    text: Option<String>,
    integer: Option<i64>,
    real: Option<f64>,
    boolean: Option<i64>,
) -> Value {
    match field_type {
        "integer" | "auto_number" => integer.map_or(Value::Null, |value| json!(value)),
        "decimal" => real.map_or(Value::Null, |value| json!(value)),
        "boolean" => boolean.map_or(Value::Null, |value| json!(value != 0)),
        _ => text.map_or(Value::Null, |value| json!(value)),
    }
}

fn load_record_values(
    connection: &rusqlite::Connection,
    record_uids: &[String],
) -> rusqlite::Result<HashMap<String, BTreeMap<String, Value>>> {
    if record_uids.is_empty() {
        return Ok(HashMap::new());
    }

    let placeholders = std::iter::repeat("?")
        .take(record_uids.len())
        .collect::<Vec<_>>()
        .join(",");

    let sql = format!(
        r#"
        SELECT
            rv.record_uid,
            f.field_key,
            f.field_type,
            rv.value_text,
            rv.value_integer,
            rv.value_real,
            rv.value_boolean
        FROM mx_record_values rv
        JOIN mx_fields f
          ON f.uid = rv.field_uid
        WHERE f.active = 1
          AND rv.record_uid IN ({placeholders})
        ORDER BY f.position ASC, f.label COLLATE NOCASE ASC
        "#
    );

    let params = record_uids
        .iter()
        .cloned()
        .map(SqlValue::Text)
        .collect::<Vec<_>>();

    let mut statement = connection.prepare(&sql)?;
    let mut rows = statement.query(params_from_iter(params.iter()))?;
    let mut values: HashMap<String, BTreeMap<String, Value>> = HashMap::new();

    while let Some(row) = rows.next()? {
        let record_uid: String = row.get(0)?;
        let key: String = row.get(1)?;
        let field_type: String = row.get(2)?;
        let text: Option<String> = row.get(3)?;
        let integer: Option<i64> = row.get(4)?;
        let real: Option<f64> = row.get(5)?;
        let boolean: Option<i64> = row.get(6)?;

        values.entry(record_uid).or_default().insert(
            key,
            value_from_row(&field_type, text, integer, real, boolean),
        );
    }

    Ok(values)
}

fn load_attachments(
    connection: &rusqlite::Connection,
    record_uids: &[String],
) -> rusqlite::Result<HashMap<String, Vec<FileAttachment>>> {
    ensure_attachment_fields_schema(connection)?;

    if record_uids.is_empty() {
        return Ok(HashMap::new());
    }

    let placeholders = std::iter::repeat("?")
        .take(record_uids.len())
        .collect::<Vec<_>>()
        .join(",");

    let sql = format!(
        r#"
        SELECT
            entry_uid,
            uid,
            file_name,
            mime_type,
            size,
            object_key,
            version_id,
            attachment_field_uid,
            attachment_field_label,
            attachment_field_storage_name
        FROM mx_attachments
        WHERE entry_uid IN ({placeholders})
        ORDER BY rowid ASC
        "#
    );

    let params = record_uids
        .iter()
        .cloned()
        .map(SqlValue::Text)
        .collect::<Vec<_>>();

    let mut statement = connection.prepare(&sql)?;
    let mut rows = statement.query(params_from_iter(params.iter()))?;
    let mut attachments: HashMap<String, Vec<FileAttachment>> = HashMap::new();

    while let Some(row) = rows.next()? {
        let entry_uid: String = row.get(0)?;
        let size: i64 = row.get(4)?;

        attachments
            .entry(entry_uid)
            .or_default()
            .push(FileAttachment {
                uid: row.get(1)?,
                file_name: row.get(2)?,
                mime_type: row.get(3)?,
                size: size.max(0) as u64,
                object_key: row.get(5)?,
                version_id: row.get(6)?,
                attachment_field_uid: row.get(7)?,
                attachment_field_label: row.get(8)?,
                attachment_field_storage_name: row.get(9)?,
            });
    }

    Ok(attachments)
}

fn sql_value_expression(alias: &str) -> String {
    format!(
        r#"CASE f.field_type
            WHEN 'integer' THEN CAST({alias}.value_integer AS TEXT)
            WHEN 'auto_number' THEN CAST({alias}.value_integer AS TEXT)
            WHEN 'decimal' THEN CAST({alias}.value_real AS TEXT)
            WHEN 'boolean' THEN CASE {alias}.value_boolean WHEN 1 THEN 'true' ELSE 'false' END
            ELSE COALESCE({alias}.value_text, '')
        END"#
    )
}

fn build_where_clause(
    query: &DynamicListQuery,
    fields: &[FieldDefinition],
) -> Result<(String, Vec<SqlValue>), String> {
    let mut conditions: Vec<String> = Vec::new();
    let mut sql_params: Vec<SqlValue> = Vec::new();
    let match_mode = normalize_match_mode(query.match_mode.as_deref());
    let expression = sql_value_expression("rv");

    if let Some(q) = query
        .q
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let pattern = search_pattern(q, match_mode);

        conditions.push(format!(
            r#"(
                EXISTS (
                    SELECT 1
                    FROM mx_record_values rv
                    JOIN mx_fields f
                      ON f.uid = rv.field_uid
                    WHERE rv.record_uid = e.uid
                      AND f.active = 1
                      AND f.searchable = 1
                      AND LOWER({expression}) LIKE LOWER(?) ESCAPE '!'
                )
                OR EXISTS (
                    SELECT 1
                    FROM mx_attachments af
                    WHERE af.entry_uid = e.uid
                      AND LOWER(af.file_name) LIKE LOWER(?) ESCAPE '!'
                )
            )"#
        ));

        sql_params.push(SqlValue::Text(pattern.clone()));
        sql_params.push(SqlValue::Text(pattern));
    }

    let field_map = field_map_by_key(fields);

    if let Some(raw_filters) = query
        .filters
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let filters = serde_json::from_str::<BTreeMap<String, Value>>(raw_filters)
            .map_err(|_| "Invalid MX field filter payload.".to_string())?;

        for (key, raw_value) in filters {
            let Some(field) = field_map.get(&key) else {
                return Err(format!("Unknown MX filter field '{key}'."));
            };

            if !field.searchable {
                return Err(format!("{} is not configured as searchable.", field.label));
            }

            let value = match raw_value {
                Value::String(value) => value.trim().to_string(),
                Value::Bool(value) => value.to_string(),
                Value::Number(value) => value.to_string(),
                Value::Null => String::new(),
                _ => {
                    return Err(format!(
                        "{} contains an unsupported filter value.",
                        field.label
                    ));
                }
            };

            if value.is_empty() {
                continue;
            }

            let pattern = search_pattern(&value, match_mode);

            conditions.push(format!(
                r#"EXISTS (
                    SELECT 1
                    FROM mx_record_values rv
                    JOIN mx_fields f
                      ON f.uid = rv.field_uid
                    WHERE rv.record_uid = e.uid
                      AND f.uid = ?
                      AND LOWER({expression}) LIKE LOWER(?) ESCAPE '!'
                )"#
            ));

            sql_params.push(SqlValue::Text(field.uid.clone()));
            sql_params.push(SqlValue::Text(pattern));
        }
    }

    match query
        .attachments
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some("with") => conditions.push(
            "EXISTS (SELECT 1 FROM mx_attachments af WHERE af.entry_uid = e.uid)".to_string(),
        ),

        Some("without") => conditions.push(
            "NOT EXISTS (SELECT 1 FROM mx_attachments af WHERE af.entry_uid = e.uid)".to_string(),
        ),

        Some(_) => {
            return Err("attachments must be 'with', 'without', or empty.".to_string());
        }

        None => {}
    }

    if conditions.is_empty() {
        Ok((String::new(), sql_params))
    } else {
        Ok((format!("WHERE {}", conditions.join(" AND ")), sql_params))
    }
}

fn quote_sql_literal(value: &str) -> String {
    value.replace('\'', "''")
}

fn build_order_clause(query: &DynamicListQuery, fields: &[FieldDefinition]) -> String {
    let direction = match query
        .sort_dir
        .as_deref()
        .unwrap_or("desc")
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "asc" => "ASC",
        _ => "DESC",
    };

    let Some(sort_key) = query
        .sort_by
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return "ORDER BY e.rowid DESC".to_string();
    };

    let Some(field) = fields
        .iter()
        .find(|field| field.key == sort_key && field.sortable)
    else {
        return "ORDER BY e.rowid DESC".to_string();
    };

    let field_uid = quote_sql_literal(&field.uid);

    let value_column = match field.field_type.as_str() {
        "integer" | "auto_number" => "rv.value_integer",
        "decimal" => "rv.value_real",
        "boolean" => "rv.value_boolean",
        _ => "rv.value_text COLLATE NOCASE",
    };

    format!(
        r#"ORDER BY (
            SELECT {value_column}
            FROM mx_record_values rv
            WHERE rv.record_uid = e.uid
              AND rv.field_uid = '{field_uid}'
            LIMIT 1
        ) {direction}, e.rowid DESC"#
    )
}

fn list_records_db(query: DynamicListQuery) -> Result<DynamicPage, SqliteDatabaseError> {
    with_sql_connection(|connection| {
        ensure_dynamic_schema(connection)?;
        let fields = load_fields_db(connection, false)?;

        let page = query.page.unwrap_or(1).max(1);
        let limit = query
            .limit
            .unwrap_or(DEFAULT_PAGE_LIMIT)
            .clamp(1, MAX_PAGE_LIMIT);
        let offset = page.saturating_sub(1).saturating_mul(limit);

        let (where_clause, mut base_params) =
            build_where_clause(&query, &fields).map_err(rusqlite::Error::InvalidParameterName)?;

        let count_sql = format!(
            r#"
            SELECT COUNT(*)
            FROM mx_records e
            {where_clause}
            "#
        );

        let total: i64 =
            connection.query_row(&count_sql, params_from_iter(base_params.iter()), |row| {
                row.get(0)
            })?;

        let order_clause = build_order_clause(&query, &fields);
        let data_sql = format!(
            r#"
            SELECT e.uid
            FROM mx_records e
            {where_clause}
            {order_clause}
            LIMIT ? OFFSET ?
            "#
        );

        base_params.push(SqlValue::Integer(limit as i64));
        base_params.push(SqlValue::Integer(offset as i64));

        let mut statement = connection.prepare(&data_sql)?;
        let record_uids = statement
            .query_map(params_from_iter(base_params.iter()), |row| {
                row.get::<_, String>(0)
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut values = load_record_values(connection, &record_uids)?;
        let mut revisions = load_field_revisions(connection, &record_uids)?;
        let mut attachments = load_attachments(connection, &record_uids)?;

        let data = record_uids
            .into_iter()
            .map(|uid| DynamicRecord {
                values: values.remove(&uid).unwrap_or_default(),
                field_revisions: revisions.remove(&uid).unwrap_or_default(),
                attached_files: attachments.remove(&uid).unwrap_or_default(),
                uid,
            })
            .collect::<Vec<_>>();

        let total = total.max(0) as usize;
        let total_pages = if total == 0 {
            1
        } else {
            (total + limit - 1) / limit
        };

        Ok(DynamicPage {
            data,
            page,
            limit,
            has_next: page < total_pages,
            total,
            total_pages,
        })
    })
}

fn get_record_db(uid: String) -> Result<DynamicRecord, SqliteDatabaseError> {
    with_sql_connection(|connection| {
        ensure_dynamic_schema(connection)?;
        ensure_record_collaboration_schema(connection)?;

        let exists = connection
            .query_row(
                "SELECT uid FROM mx_records WHERE uid = ?1",
                params![&uid],
                |row| row.get::<_, String>(0),
            )
            .optional()?;

        if exists.is_none() {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }

        let record_uids = vec![uid.clone()];
        let mut values = load_record_values(connection, &record_uids)?;
        let mut revisions = load_field_revisions(connection, &record_uids)?;
        let mut attachments = load_attachments(connection, &record_uids)?;

        Ok(DynamicRecord {
            values: values.remove(&uid).unwrap_or_default(),
            field_revisions: revisions.remove(&uid).unwrap_or_default(),
            attached_files: attachments.remove(&uid).unwrap_or_default(),
            uid,
        })
    })
}

fn database_error_response(error: SqliteDatabaseError) -> Response {
    match error {
        SqliteDatabaseError::Sqlite(rusqlite::Error::QueryReturnedNoRows) => api_json(
            StatusCode::NOT_FOUND,
            json!({ "response": "MX record was not found." }),
        ),

        SqliteDatabaseError::Sqlite(rusqlite::Error::InvalidParameterName(message)) => {
            api_json(StatusCode::BAD_REQUEST, json!({ "response": message }))
        }

        SqliteDatabaseError::Sqlite(rusqlite::Error::SqliteFailure(error, _))
            if error.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            api_json(
                StatusCode::CONFLICT,
                json!({
                    "response": "A configured unique MX field conflicts with an existing record."
                }),
            )
        }

        error => {
            crate::report_error!(
                format!("{error}"),
                "function",
                "dynamic MX record operation"
            );

            api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({ "response": "SQLite database operation failed." }),
            )
        }
    }
}

pub async fn create_mx_record(
    claims: Claims,
    Json(request): Json<DynamicRecordRequest>,
) -> Response {
    if !claims.can_write_records() {
        return access_denied();
    }

    let schema = match load_active_schema().await {
        Ok(schema) => schema,
        Err(error) => {
            crate::report_error!(error, "function", "create_mx_record()");

            return api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({ "response": "failed to load MX record structure" }),
            );
        }
    };

    let normalized = match validate_payload(&schema.fields, &request.values) {
        Ok(values) => values,
        Err(error) => {
            return api_json(StatusCode::BAD_REQUEST, json!({ "response": error }));
        }
    };

    let actor_uid = claims.uid.clone();
    let database_result =
        tokio::task::spawn_blocking(move || create_record_db(normalized, actor_uid)).await;

    let uid = match database_result {
        Ok(Ok(uid)) => uid,
        Ok(Err(error)) => return database_error_response(error),
        Err(error) => {
            crate::report_error!(format!("{error}"), "function", "create_mx_record()");

            return api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({ "response": "record creation task failed" }),
            );
        }
    };

    let record_result = tokio::task::spawn_blocking(move || get_record_db(uid)).await;

    match record_result {
        Ok(Ok(record)) => {
            publish_live_event(
                "record.created",
                Some(&claims.uid),
                json!({"record_uid": record.uid}),
            );
            api_json(StatusCode::CREATED, json!(record))
        }
        Ok(Err(error)) => database_error_response(error),
        Err(error) => {
            crate::report_error!(format!("{error}"), "function", "create_mx_record()");

            api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({ "response": "record was created but could not be reloaded" }),
            )
        }
    }
}

pub async fn update_mx_record(
    claims: Claims,
    Path(uid): Path<String>,
    Json(request): Json<DynamicRecordRequest>,
) -> Response {
    if !claims.can_write_records() {
        return access_denied();
    }

    let schema = match load_active_schema().await {
        Ok(schema) => schema,
        Err(error) => {
            crate::report_error!(error, "function", "update_mx_record()");

            return api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({ "response": "failed to load MX record structure" }),
            );
        }
    };

    let normalized = match validate_payload(&schema.fields, &request.values) {
        Ok(values) => values,
        Err(error) => {
            return api_json(StatusCode::BAD_REQUEST, json!({ "response": error }));
        }
    };

    let uid_for_update = uid.clone();
    let actor_uid = claims.uid.clone();
    let database_result = tokio::task::spawn_blocking(move || {
        update_record_db(uid_for_update, normalized, actor_uid)
    })
    .await;

    match database_result {
        Ok(Ok(())) => {}
        Ok(Err(error)) => return database_error_response(error),
        Err(error) => {
            crate::report_error!(format!("{error}"), "function", "update_mx_record()");

            return api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({ "response": "record update task failed" }),
            );
        }
    }

    let record_result = tokio::task::spawn_blocking(move || get_record_db(uid)).await;

    match record_result {
        Ok(Ok(record)) => {
            publish_live_event(
                "record.updated",
                Some(&claims.uid),
                json!({
                    "record_uid": record.uid,
                    "mode": "legacy_full_update"
                }),
            );
            api_json(StatusCode::OK, json!(record))
        }
        Ok(Err(error)) => database_error_response(error),
        Err(error) => {
            crate::report_error!(format!("{error}"), "function", "update_mx_record()");

            api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({ "response": "record was updated but could not be reloaded" }),
            )
        }
    }
}

pub async fn get_mx_record(claims: Claims, Path(uid): Path<String>) -> Response {
    if !claims.can_read_records() {
        return access_denied();
    }

    let result = tokio::task::spawn_blocking(move || get_record_db(uid)).await;
    match result {
        Ok(Ok(record)) => api_json(StatusCode::OK, json!(record)),
        Ok(Err(error)) => database_error_response(error),
        Err(error) => {
            crate::report_error!(format!("{error}"), "function", "get_mx_record()");
            api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"response":"record lookup task failed"}),
            )
        }
    }
}

pub async fn patch_mx_record(
    claims: Claims,
    Path(uid): Path<String>,
    Json(request): Json<DynamicPatchRequest>,
) -> Response {
    if !claims.can_write_records() {
        return access_denied();
    }

    let schema = match load_active_schema().await {
        Ok(schema) => schema,
        Err(error) => {
            crate::report_error!(error, "function", "patch_mx_record()");
            return api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"response":"failed to load MX record structure"}),
            );
        }
    };

    let normalized =
        match validate_patch_payload(&schema.fields, &request.changes, &request.base_revisions) {
            Ok(values) => values,
            Err(error) => {
                return api_json(StatusCode::BAD_REQUEST, json!({"response": error}));
            }
        };

    if normalized.is_empty() {
        let uid_for_load = uid.clone();
        return match tokio::task::spawn_blocking(move || get_record_db(uid_for_load)).await {
            Ok(Ok(record)) => api_json(StatusCode::OK, json!(record)),
            Ok(Err(error)) => database_error_response(error),
            Err(error) => {
                crate::report_error!(format!("{error}"), "function", "patch_mx_record()");
                api_json(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    json!({"response":"record lookup task failed"}),
                )
            }
        };
    }

    let raw_changes = request.changes.clone();
    let base_revisions = request.base_revisions.clone();
    let actor_uid = claims.uid.clone();
    let uid_for_patch = uid.clone();

    let result = tokio::task::spawn_blocking(move || {
        patch_record_db(
            uid_for_patch,
            normalized,
            raw_changes,
            base_revisions,
            actor_uid,
        )
    })
    .await;

    let outcome = match result {
        Ok(Ok(PatchDbResult::Applied(outcome))) => outcome,
        Ok(Ok(PatchDbResult::Conflict(conflicts))) => {
            return api_json(
                StatusCode::CONFLICT,
                json!({
                    "error":"field_conflict",
                    "response":"One or more fields changed after you opened this record.",
                    "record_uid": uid,
                    "conflicts": conflicts,
                }),
            );
        }
        Ok(Ok(PatchDbResult::NotFound)) => {
            return api_json(
                StatusCode::NOT_FOUND,
                json!({"response":"MX record was not found."}),
            );
        }
        Ok(Err(error)) => return database_error_response(error),
        Err(error) => {
            crate::report_error!(format!("{error}"), "function", "patch_mx_record()");
            return api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"response":"record patch task failed"}),
            );
        }
    };

    publish_live_event(
        "record.fields.updated",
        Some(&claims.uid),
        json!({
            "record_uid": uid,
            "changes": outcome.changed_values,
            "field_revisions": outcome.field_revisions,
        }),
    );

    let uid_for_load = uid.clone();
    match tokio::task::spawn_blocking(move || get_record_db(uid_for_load)).await {
        Ok(Ok(record)) => api_json(StatusCode::OK, json!(record)),
        Ok(Err(error)) => database_error_response(error),
        Err(error) => {
            crate::report_error!(format!("{error}"), "function", "patch_mx_record()");
            api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"response":"record was updated but could not be reloaded"}),
            )
        }
    }
}

pub async fn list_mx_records(claims: Claims, Query(query): Query<DynamicListQuery>) -> Response {
    if !claims.can_read_records() {
        return access_denied();
    }

    let database_result = tokio::task::spawn_blocking(move || list_records_db(query)).await;

    match database_result {
        Ok(Ok(page)) => api_json(StatusCode::OK, json!(page)),
        Ok(Err(error)) => database_error_response(error),
        Err(error) => {
            crate::report_error!(format!("{error}"), "function", "list_mx_records()");

            api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({ "response": "record list task failed" }),
            )
        }
    }
}
