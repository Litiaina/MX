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
        lifecycle::{capture_record_version, ensure_record_lifecycle_schema},
        live::publish_live_event,
        modules::{DEFAULT_MODULE_UID, module_can_for_user},
        mx::{
            attachment_fields::ensure_attachment_fields_schema,
            formula::evaluate_expression,
            model::FileAttachment,
            schema::{
                FieldDefinition, ensure_dynamic_schema, field_map_by_key,
                load_active_module_schema, load_module_fields_db,
            },
        },
        notifications::notify_module_readers,
        operations::{self, ClientOperation},
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
    pub operation_uid: Option<String>,
    pub base_revision: Option<i64>,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct DynamicPatchRequest {
    #[serde(default)]
    pub changes: BTreeMap<String, Value>,

    #[serde(default)]
    pub base_revision: i64,
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
    revision: i64,
}

#[derive(Debug)]
enum PatchDbResult {
    Applied(PatchOutcome),
    Conflict {
        conflicts: Vec<FieldConflict>,
        current_revision: i64,
    },
    NotFound,
}

#[derive(Debug, Serialize, Clone)]
pub struct DynamicRecord {
    pub uid: String,
    pub revision: i64,
    pub values: BTreeMap<String, Value>,
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

    fn as_number(&self) -> Option<f64> {
        match self {
            Self::Integer(value) => Some(*value as f64),
            Self::Real(value) => Some(*value),
            Self::Boolean(value) => Some(if *value { 1.0 } else { 0.0 }),
            Self::Text(value) => value.trim().parse::<f64>().ok(),
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
        "relationship" => {
            super::relationships::normalize(field, value).map(|v| v.map(NormalizedValue::Text))
        }
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

        "auto_number" | "attachments" | "formula" | "lookup" | "rollup" => Ok(None),

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
        if matches!(
            field.field_type.as_str(),
            "auto_number" | "attachments" | "formula" | "lookup" | "rollup"
        ) {
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

        if matches!(
            field.field_type.as_str(),
            "auto_number" | "attachments" | "formula" | "lookup" | "rollup"
        ) {
            return Err(format!(
                "{} cannot be modified through the record field PATCH API.",
                field.label
            ));
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

fn apply_formula_values(
    fields: &[FieldDefinition],
    values: &mut BTreeMap<String, NormalizedValue>,
) -> Result<(), String> {
    let formula_fields = fields
        .iter()
        .filter(|field| field.field_type == "formula")
        .collect::<Vec<_>>();
    if formula_fields.is_empty() {
        return Ok(());
    }

    // Re-evaluate several times so formulas may reference earlier or later
    // computed fields. Blank numeric inputs are treated as zero.
    for _ in 0..=formula_fields.len() {
        for field in &formula_fields {
            let mut variables = fields
                .iter()
                .map(|candidate| (candidate.key.to_ascii_lowercase(), 0.0))
                .collect::<BTreeMap<_, _>>();
            for (key, value) in values.iter() {
                if let Some(number) = value.as_number() {
                    variables.insert(key.to_ascii_lowercase(), number);
                }
            }
            let expression = field
                .config
                .get("expression")
                .and_then(Value::as_str)
                .unwrap_or("");
            let decimals = field
                .config
                .get("decimals")
                .and_then(Value::as_u64)
                .unwrap_or(2)
                .min(12) as i32;
            let raw = evaluate_expression(expression, Some(&variables))
                .map_err(|error| format!("{}: {error}", field.label))?;
            let factor = 10_f64.powi(decimals);
            values.insert(
                field.key.clone(),
                NormalizedValue::Real((raw * factor).round() / factor),
            );
        }
    }
    Ok(())
}

fn calculate_formula_json_values(
    fields: &[FieldDefinition],
    values: &mut BTreeMap<String, Value>,
) -> Result<(), String> {
    let mut normalized = BTreeMap::new();
    for field in fields {
        let Some(value) = values.get(&field.key) else {
            continue;
        };
        let converted = match field.field_type.as_str() {
            "integer" | "auto_number" => value.as_i64().map(NormalizedValue::Integer),
            "decimal" | "formula" => value.as_f64().map(NormalizedValue::Real),
            "boolean" => value.as_bool().map(NormalizedValue::Boolean),
            _ => value
                .as_str()
                .map(|value| NormalizedValue::Text(value.to_string())),
        };
        if let Some(converted) = converted {
            normalized.insert(field.key.clone(), converted);
        }
    }
    apply_formula_values(fields, &mut normalized)?;
    for field in fields.iter().filter(|field| field.field_type == "formula") {
        if let Some(value) = normalized.get(&field.key) {
            values.insert(field.key.clone(), normalized_to_json(value));
        }
    }
    Ok(())
}

fn load_normalized_record_values(
    transaction: &rusqlite::Transaction<'_>,
    record_uid: &str,
    fields: &[FieldDefinition],
) -> rusqlite::Result<BTreeMap<String, NormalizedValue>> {
    let mut values = BTreeMap::new();
    for field in fields
        .iter()
        .filter(|field| field.field_type != "attachments")
    {
        let row = transaction
            .query_row(
                "SELECT value_text, value_integer, value_real, value_boolean FROM mx_record_values WHERE record_uid=?1 AND field_uid=?2",
                params![record_uid, &field.uid],
                |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, Option<i64>>(1)?, row.get::<_, Option<f64>>(2)?, row.get::<_, Option<i64>>(3)?)),
            )
            .optional()?;
        let value = match row {
            Some((_, Some(value), _, _))
                if matches!(field.field_type.as_str(), "integer" | "auto_number") =>
            {
                Some(NormalizedValue::Integer(value))
            }
            Some((_, _, Some(value), _))
                if matches!(field.field_type.as_str(), "decimal" | "formula") =>
            {
                Some(NormalizedValue::Real(value))
            }
            Some((_, _, _, Some(value))) if field.field_type == "boolean" => {
                Some(NormalizedValue::Boolean(value != 0))
            }
            Some((Some(value), _, _, _)) => Some(NormalizedValue::Text(value)),
            _ => None,
        };
        if let Some(value) = value {
            values.insert(field.key.clone(), value);
        }
    }
    Ok(values)
}

pub(crate) fn recalculate_module_formulas(
    connection: &rusqlite::Connection,
    module_uid: &str,
    actor_uid: &str,
) -> rusqlite::Result<()> {
    let fields = load_module_fields_db(connection, module_uid, false)?;
    if !fields.iter().any(|field| field.field_type == "formula") {
        return Ok(());
    }
    let record_uids = connection
        .prepare("SELECT uid FROM mx_records WHERE module_uid=?1 AND deleted_at IS NULL")?
        .query_map(params![module_uid], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let transaction = connection.unchecked_transaction()?;
    for record_uid in record_uids {
        let mut values = load_normalized_record_values(&transaction, &record_uid, &fields)?;
        apply_formula_values(&fields, &mut values)
            .map_err(|error| rusqlite::Error::InvalidParameterName(format!("MX_CONFIG:{error}")))?;
        for field in fields.iter().filter(|field| field.field_type == "formula") {
            let Some(value) = values.get(&field.key) else {
                continue;
            };
            write_single_dynamic_value(&transaction, &record_uid, field, Some(value))?;
            let revision =
                current_field_revision(&transaction, &record_uid, &field.uid)?.saturating_add(1);
            set_field_revision(&transaction, &record_uid, &field.uid, revision, actor_uid)?;
        }
    }
    transaction.commit()
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
    )?;
    let has_record_revision = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('mx_records') WHERE name='revision')",
        [],
        |row| row.get::<_, bool>(0),
    )?;
    if !has_record_revision {
        connection.execute(
            "ALTER TABLE mx_records ADD COLUMN revision INTEGER NOT NULL DEFAULT 1 CHECK(revision>=1)",
            [],
        )?;
    }
    connection.execute(
        r#"UPDATE mx_records
           SET revision=MAX(revision,COALESCE((
             SELECT MAX(field_revision.revision)
             FROM mx_record_field_revisions field_revision
             WHERE field_revision.record_uid=mx_records.uid
           ),1))
           WHERE revision<COALESCE((
             SELECT MAX(field_revision.revision)
             FROM mx_record_field_revisions field_revision
             WHERE field_revision.record_uid=mx_records.uid
           ),1)"#,
        [],
    )?;
    Ok(())
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

fn field_changed_after_base(
    current_record_revision: i64,
    base_revision: i64,
    field_revision: i64,
) -> bool {
    current_record_revision > base_revision && field_revision > base_revision
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
    revision: i64,
    actor_uid: &str,
) -> rusqlite::Result<()> {
    for field in fields
        .iter()
        .filter(|field| field.field_type != "attachments")
    {
        set_field_revision(transaction, record_uid, &field.uid, revision, actor_uid)?;
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

fn load_record_revisions(
    connection: &rusqlite::Connection,
    record_uids: &[String],
) -> rusqlite::Result<HashMap<String, i64>> {
    ensure_record_collaboration_schema(connection)?;

    if record_uids.is_empty() {
        return Ok(HashMap::new());
    }

    let placeholders = std::iter::repeat_n("?", record_uids.len())
        .collect::<Vec<_>>()
        .join(",");
    let params = record_uids
        .iter()
        .cloned()
        .map(SqlValue::Text)
        .collect::<Vec<_>>();

    let mut revisions = HashMap::new();
    let sql = format!("SELECT uid,revision FROM mx_records WHERE uid IN ({placeholders})");
    let mut statement = connection.prepare(&sql)?;
    let mut rows = statement.query(params_from_iter(params.iter()))?;
    while let Some(row) = rows.next()? {
        revisions.insert(row.get::<_, String>(0)?, row.get::<_, i64>(1)?.max(1));
    }

    Ok(revisions)
}

fn patch_record_db(
    module_uid: String,
    uid: String,
    changes: BTreeMap<String, Option<NormalizedValue>>,
    raw_changes: BTreeMap<String, Value>,
    base_revision: i64,
    actor_uid: String,
) -> Result<PatchDbResult, SqliteDatabaseError> {
    with_sql_connection(|connection| {
        ensure_dynamic_schema(connection)?;
        ensure_record_collaboration_schema(connection)?;
        ensure_record_lifecycle_schema(connection)?;

        let fields = load_module_fields_db(connection, &module_uid, false)?;
        let field_map = field_map_by_key(&fields);
        for _ in 0..3 {
            let transaction = rusqlite::Transaction::new_unchecked(
                connection,
                rusqlite::TransactionBehavior::Immediate,
            )?;
            let current_record_revision = transaction.query_row(
                "SELECT revision FROM mx_records WHERE uid=?1 AND module_uid=?2 AND deleted_at IS NULL",
                params![uid,module_uid],
                |row| row.get::<_,i64>(0),
            ).optional()?;
            let Some(current_record_revision) = current_record_revision else {
                return Ok(PatchDbResult::NotFound);
            };
            super::relationships::validate_selection(
                &transaction,
                &fields,
                &raw_changes,
                &actor_uid,
            )?;
            if base_revision < 1 || base_revision > current_record_revision {
                return Err(rusqlite::Error::InvalidParameterName(
                    "MX_REVISION:The loaded record revision is invalid. Reload the record and try again.".to_string(),
                ));
            }

            let mut conflicts = Vec::new();
            for key in changes.keys() {
                let Some(field) = field_map.get(key) else {
                    continue;
                };
                let field_revision = current_field_revision(&transaction, &uid, &field.uid)?;
                if field_changed_after_base(current_record_revision, base_revision, field_revision)
                {
                    let mut current_value = current_field_value(&transaction, &uid, field)?;
                    if field.field_type == "relationship" {
                        let level: i64 = transaction.query_row(
                            "SELECT access_level FROM users WHERE uid=?1",
                            [&actor_uid],
                            |r| r.get(0),
                        )?;
                        if !module_can_for_user(
                            &transaction,
                            field.config["target_module_uid"].as_str().unwrap_or(""),
                            &actor_uid,
                            level,
                            "read",
                        )? {
                            current_value = Value::Null;
                        }
                    }
                    conflicts.push(FieldConflict {
                        field_uid: field.uid.clone(),
                        field_key: field.key.clone(),
                        label: field.label.clone(),
                        base_revision,
                        current_revision: field_revision,
                        current_value,
                        your_value: raw_changes.get(key).cloned().unwrap_or(Value::Null),
                    });
                }
            }
            if !conflicts.is_empty() {
                return Ok(PatchDbResult::Conflict {
                    conflicts,
                    current_revision: current_record_revision,
                });
            }

            let next_revision = current_record_revision.saturating_add(1);
            let advanced = transaction.execute(
                "UPDATE mx_records SET revision=?3 WHERE uid=?1 AND module_uid=?2 AND revision=?4 AND deleted_at IS NULL",
                params![uid,module_uid,next_revision,current_record_revision],
            )?;
            if advanced == 0 {
                drop(transaction);
                continue;
            }

            let mut changed_values = BTreeMap::new();
            for (key, value) in &changes {
                let Some(field) = field_map.get(key) else {
                    continue;
                };
                write_single_dynamic_value(&transaction, &uid, field, value.as_ref())?;
                update_legacy_mirror_field(&transaction, &uid, key, value.as_ref())?;
                set_field_revision(&transaction, &uid, &field.uid, next_revision, &actor_uid)?;
                changed_values.insert(
                    key.clone(),
                    value
                        .as_ref()
                        .map(normalized_to_json)
                        .unwrap_or(Value::Null),
                );
            }

            let mut calculated_values = load_normalized_record_values(&transaction, &uid, &fields)?;
            apply_formula_values(&fields, &mut calculated_values).map_err(|error| {
                rusqlite::Error::InvalidParameterName(format!("MX_FORMULA:{error}"))
            })?;
            for field in fields.iter().filter(|field| field.field_type == "formula") {
                let Some(value) = calculated_values.get(&field.key) else {
                    continue;
                };
                let current = current_field_value(&transaction, &uid, field)?;
                let next = normalized_to_json(value);
                if current.as_f64() == next.as_f64() {
                    continue;
                }
                write_single_dynamic_value(&transaction, &uid, field, Some(value))?;
                set_field_revision(&transaction, &uid, &field.uid, next_revision, &actor_uid)?;
                changed_values.insert(field.key.clone(), next);
            }
            capture_record_version(&transaction, &uid, &module_uid, "updated", &actor_uid)?;
            transaction.commit()?;
            return Ok(PatchDbResult::Applied(PatchOutcome {
                changed_values,
                revision: next_revision,
            }));
        }
        Err(rusqlite::Error::InvalidParameterName(
            "MX_REVISION:The record kept changing while this save was processed. Retry using the latest revision.".to_string(),
        ))
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
    module_uid: String,
    request_values: BTreeMap<String, Value>,
    actor_uid: String,
    operation: Option<ClientOperation>,
) -> Result<(String, bool), SqliteDatabaseError> {
    with_sql_connection(|connection| {
        ensure_dynamic_schema(connection)?;
        ensure_record_collaboration_schema(connection)?;
        ensure_record_lifecycle_schema(connection)?;
        operations::ensure_schema(connection)?;
        let transaction = rusqlite::Transaction::new_unchecked(
            connection,
            rusqlite::TransactionBehavior::Immediate,
        )?;
        let scope = format!("record:create:{module_uid}");
        if let Some(uid) =
            operations::existing(&transaction, &actor_uid, &scope, operation.as_ref())?
        {
            return Ok((uid, false));
        }
        // A committed operation must remain reconcilable even if required
        // fields changed while its acknowledgement was lost. New creates are
        // validated against the actual schema inside the write transaction.
        let fields = load_module_fields_db(&transaction, &module_uid, false)?;
        super::relationships::validate_selection(
            &transaction,
            &fields,
            &request_values,
            &actor_uid,
        )?;
        let mut values = validate_payload(&fields, &request_values)
            .map_err(rusqlite::Error::InvalidParameterName)?;
        let uid = Uuid::new_v4().to_string();

        fill_auto_numbers(&transaction, &fields, &mut values)?;
        apply_formula_values(&fields, &mut values).map_err(|error| {
            rusqlite::Error::InvalidParameterName(format!("MX_FORMULA:{error}"))
        })?;

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
                ,module_uid
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10
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
                module_uid,
            ],
        )?;

        write_dynamic_values(&transaction, &uid, &fields, &values)?;
        initialize_field_revisions(&transaction, &uid, &fields, &values, &actor_uid)?;
        capture_record_version(&transaction, &uid, &module_uid, "created", &actor_uid)?;
        operations::complete(&transaction, &actor_uid, &scope, operation.as_ref(), &uid)?;
        transaction.commit()?;

        Ok((uid, true))
    })
}

fn update_record_db(
    module_uid: String,
    uid: String,
    request_values: BTreeMap<String, NormalizedValue>,
    actor_uid: String,
    base_revision: Option<i64>,
) -> Result<(), SqliteDatabaseError> {
    with_sql_connection(|connection| {
        ensure_dynamic_schema(connection)?;
        ensure_record_collaboration_schema(connection)?;
        ensure_record_lifecycle_schema(connection)?;
        let fields = load_module_fields_db(connection, &module_uid, false)?;
        let transaction = rusqlite::Transaction::new_unchecked(
            connection,
            rusqlite::TransactionBehavior::Immediate,
        )?;

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
                WHERE uid = ?1 AND module_uid = ?2 AND deleted_at IS NULL
                "#,
            params![&uid, module_uid],
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

        let current_revision: i64 = transaction.query_row(
            "SELECT revision FROM mx_records WHERE uid=?1 AND module_uid=?2 AND deleted_at IS NULL",
            params![&uid, &module_uid],
            |row| row.get(0),
        )?;
        validate_replacement_revision(base_revision, current_revision)?;
        let relationship_values = request_values
            .iter()
            .map(|(key, v)| {
                let value = match v {
                    NormalizedValue::Text(text) if text.starts_with('[') => {
                        serde_json::from_str(text).unwrap_or_else(|_| json!(text))
                    }
                    _ => normalized_to_json(v),
                };
                (key.clone(), value)
            })
            .collect();
        super::relationships::validate_selection(
            &transaction,
            &fields,
            &relationship_values,
            &actor_uid,
        )?;
        let mut values = request_values;
        let existing_auto = load_existing_auto_numbers(&transaction, &uid, &fields)?;

        for (key, value) in existing_auto {
            values.insert(key, value);
        }

        // If a new auto-number field was added after this record existed,
        // assign it on the first edit of the historical record.
        fill_auto_numbers(&transaction, &fields, &mut values)?;
        apply_formula_values(&fields, &mut values).map_err(|error| {
            rusqlite::Error::InvalidParameterName(format!("MX_FORMULA:{error}"))
        })?;

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

        let next_revision = current_revision.saturating_add(1);

        let affected = transaction.execute(
            r#"
            UPDATE mx_records
            SET
                date = ?2,
                office = ?3,
                requestor = ?4,
                subject = ?5,
                routed_to_div = ?6,
                remarks = ?7,
                revision = ?9
            WHERE uid = ?1 AND module_uid = ?8 AND deleted_at IS NULL AND revision = ?10
            "#,
            params![
                &uid,
                date,
                office,
                requestor,
                subject,
                routed_to_div,
                remarks,
                module_uid,
                next_revision,
                current_revision,
            ],
        )?;

        if affected == 0 {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }

        write_dynamic_values(&transaction, &uid, &fields, &values)?;
        bump_all_field_revisions(&transaction, &uid, &fields, next_revision, &actor_uid)?;
        capture_record_version(&transaction, &uid, &module_uid, "updated", &actor_uid)?;
        transaction.commit()?;

        Ok(())
    })
}

fn validate_replacement_revision(
    base_revision: Option<i64>,
    current_revision: i64,
) -> rusqlite::Result<()> {
    let Some(base_revision) = base_revision else {
        return Err(rusqlite::Error::InvalidParameterName(
            "MX_BASE_REVISION:Full-record replacement requires base_revision. Reload the record or use PATCH with only changed fields.".to_string(),
        ));
    };
    if base_revision < 1 || base_revision != current_revision {
        return Err(rusqlite::Error::InvalidParameterName(format!(
            "MX_REVISION:The record changed since this replacement was loaded (current revision {current_revision}). Reload it or use PATCH with only changed fields."
        )));
    }
    Ok(())
}

fn value_from_row(
    field_type: &str,
    text: Option<String>,
    integer: Option<i64>,
    real: Option<f64>,
    boolean: Option<i64>,
) -> Value {
    match field_type {
        "relationship" => text.map_or(Value::Null, |value| {
            if value.starts_with('[') {
                serde_json::from_str(&value).unwrap_or(Value::Null)
            } else {
                json!(value)
            }
        }),
        "integer" | "auto_number" => integer.map_or(Value::Null, |value| json!(value)),
        "decimal" | "formula" => real.map_or(Value::Null, |value| json!(value)),
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

    let placeholders = std::iter::repeat_n("?", record_uids.len())
        .collect::<Vec<_>>()
        .join(",");

    let sql = format!(
        r#"
        SELECT
            rv.record_uid,
            COALESCE(f.module_key, f.field_key),
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

    let placeholders = std::iter::repeat_n("?", record_uids.len())
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
            WHEN 'formula' THEN CAST({alias}.value_real AS TEXT)
            WHEN 'boolean' THEN CASE {alias}.value_boolean WHEN 1 THEN 'true' ELSE 'false' END
            ELSE COALESCE({alias}.value_text, '')
        END"#
    )
}

fn build_where_clause(
    module_uid: &str,
    query: &DynamicListQuery,
    fields: &[FieldDefinition],
    include_attachments: bool,
) -> Result<(String, Vec<SqlValue>), String> {
    let mut conditions: Vec<String> = vec![
        "e.module_uid = ?".to_string(),
        "e.deleted_at IS NULL".to_string(),
    ];
    let mut sql_params: Vec<SqlValue> = vec![SqlValue::Text(module_uid.to_string())];
    let match_mode = normalize_match_mode(query.match_mode.as_deref());
    let expression = sql_value_expression("rv");

    if let Some(q) = query
        .q
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let pattern = search_pattern(q, match_mode);

        conditions.push(if include_attachments {
            format!(
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
            )
        } else {
            format!(
                r#"EXISTS (
                SELECT 1 FROM mx_record_values rv
                JOIN mx_fields f ON f.uid=rv.field_uid
                WHERE rv.record_uid=e.uid AND f.active=1 AND f.searchable=1
                  AND LOWER({expression}) LIKE LOWER(?) ESCAPE '!'
            )"#
            )
        });

        if include_attachments {
            sql_params.push(SqlValue::Text(pattern.clone()));
        }
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

    if !include_attachments
        && query
            .attachments
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
    {
        return Err("Attachment access is required to filter by files.".to_string());
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
        "decimal" | "formula" => "rv.value_real",
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

fn list_records_db(
    module_uid: String,
    query: DynamicListQuery,
    include_attachments: bool,
) -> Result<DynamicPage, SqliteDatabaseError> {
    with_sql_connection(|connection| {
        ensure_dynamic_schema(connection)?;
        ensure_record_lifecycle_schema(connection)?;
        let fields = load_module_fields_db(connection, &module_uid, false)?;

        let page = query.page.unwrap_or(1).max(1);
        let limit = query
            .limit
            .unwrap_or(DEFAULT_PAGE_LIMIT)
            .clamp(1, MAX_PAGE_LIMIT);
        let offset = page.saturating_sub(1).saturating_mul(limit);

        let (where_clause, mut base_params) =
            build_where_clause(&module_uid, &query, &fields, include_attachments)
                .map_err(rusqlite::Error::InvalidParameterName)?;

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
        let mut revisions = load_record_revisions(connection, &record_uids)?;
        let mut attachments = if include_attachments {
            load_attachments(connection, &record_uids)?
        } else {
            HashMap::new()
        };

        for record_values in values.values_mut() {
            calculate_formula_json_values(&fields, record_values).map_err(|error| {
                rusqlite::Error::InvalidParameterName(format!("MX_FORMULA:{error}"))
            })?;
        }

        let data = record_uids
            .into_iter()
            .map(|uid| DynamicRecord {
                values: values.remove(&uid).unwrap_or_default(),
                revision: revisions.remove(&uid).unwrap_or(1),
                attached_files: attachments.remove(&uid).unwrap_or_default(),
                uid,
            })
            .collect::<Vec<_>>();

        let total = total.max(0) as usize;
        let total_pages = if total == 0 { 1 } else { total.div_ceil(limit) };

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

fn get_record_db(
    module_uid: String,
    uid: String,
    include_attachments: bool,
) -> Result<DynamicRecord, SqliteDatabaseError> {
    with_sql_connection(|connection| {
        ensure_dynamic_schema(connection)?;
        ensure_record_collaboration_schema(connection)?;
        ensure_record_lifecycle_schema(connection)?;

        let revision = connection
            .query_row(
                "SELECT revision FROM mx_records WHERE uid = ?1 AND module_uid = ?2 AND deleted_at IS NULL",
                params![&uid, module_uid],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;

        let Some(revision) = revision else {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        };

        let record_uids = vec![uid.clone()];
        let fields = load_module_fields_db(connection, &module_uid, false)?;
        let mut values = load_record_values(connection, &record_uids)?;
        let mut attachments = if include_attachments {
            load_attachments(connection, &record_uids)?
        } else {
            HashMap::new()
        };

        let mut record_values = values.remove(&uid).unwrap_or_default();
        calculate_formula_json_values(&fields, &mut record_values).map_err(|error| {
            rusqlite::Error::InvalidParameterName(format!("MX_FORMULA:{error}"))
        })?;
        Ok(DynamicRecord {
            values: record_values,
            revision: revision.max(1),
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
            let (status, prefix) = if message.starts_with("MX_BASE_REVISION:") {
                (StatusCode::PRECONDITION_REQUIRED, "MX_BASE_REVISION:")
            } else if message.starts_with("MX_OPERATION:") {
                (StatusCode::CONFLICT, "MX_OPERATION:")
            } else if message.starts_with("MX_REVISION:") {
                (StatusCode::CONFLICT, "MX_REVISION:")
            } else {
                (StatusCode::BAD_REQUEST, "MX_FORMULA:")
            };
            api_json(
                status,
                json!({"response":message.strip_prefix(prefix).unwrap_or(&message)}),
            )
        }

        SqliteDatabaseError::Sqlite(rusqlite::Error::SqliteFailure(error, _))
            if error.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            api_json(
                StatusCode::CONFLICT,
                json!({
                    "response": "A unique field or record relationship conflicts with the current stored data."
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

async fn module_permission(
    module_uid: &str,
    user_uid: &str,
    access_level: i64,
    capability: &str,
) -> bool {
    let module_uid = module_uid.to_string();
    let user_uid = user_uid.to_string();
    let capability = capability.to_string();
    tokio::task::spawn_blocking(move || {
        with_sql_connection(|connection| {
            module_can_for_user(
                connection,
                &module_uid,
                &user_uid,
                access_level,
                &capability,
            )
        })
    })
    .await
    .ok()
    .and_then(Result::ok)
    .unwrap_or(false)
}

async fn present_response(status: StatusCode, claims: &Claims, mut value: Value) -> Response {
    let claims = claims.clone();
    match tokio::task::spawn_blocking(move || {
        with_sql_connection(|c| {
            let tx = c.unchecked_transaction()?;
            if let Some(rows) = value.get_mut("data").and_then(Value::as_array_mut) {
                super::relationships::present_many(&tx, &claims, rows)?;
            } else {
                super::relationships::present(&tx, &claims, &mut value)?;
            }
            tx.commit()?;
            Ok(value)
        })
    })
    .await
    {
        Ok(Ok(value)) => api_json(status, value),
        Ok(Err(error)) => database_error_response(error),
        Err(_) => api_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"response":"Linked record lookup failed."}),
        ),
    }
}

async fn create_record_for_module(
    claims: Claims,
    module_uid: String,
    request: DynamicRecordRequest,
) -> Response {
    let operation =
        match ClientOperation::parse(request.operation_uid.clone(), &json!(request.values)) {
            Ok(operation) => operation,
            Err(message) => return api_json(StatusCode::BAD_REQUEST, json!({"response":message})),
        };
    if !claims.can_write_records()
        || !module_permission(&module_uid, &claims.uid, claims.access_level, "create").await
    {
        return access_denied();
    }
    let include_attachments =
        module_permission(&module_uid, &claims.uid, claims.access_level, "attachments").await;
    let actor_uid = claims.uid.clone();
    let db_module_uid = module_uid.clone();
    let (uid, created) = match tokio::task::spawn_blocking(move || {
        create_record_db(db_module_uid, request.values, actor_uid, operation)
    })
    .await
    {
        Ok(Ok(uid)) => uid,
        Ok(Err(error)) => return database_error_response(error),
        Err(error) => {
            crate::report_error!(format!("{error}"), "function", "create_record_for_module()");
            return api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"response":"record creation task failed"}),
            );
        }
    };
    match tokio::task::spawn_blocking({
        let module_uid = module_uid.clone();
        move || get_record_db(module_uid, uid, include_attachments)
    })
    .await
    {
        Ok(Ok(record)) => {
            if !created {
                return present_response(StatusCode::OK, &claims, json!(record)).await;
            }
            publish_live_event(
                "record.created",
                Some(&claims.uid),
                json!({"record_uid":record.uid,"module_uid":module_uid}),
            );
            tokio::spawn(notify_module_readers(
                module_uid.clone(),
                claims.uid.clone(),
                "record.created".to_string(),
                "Record created".to_string(),
                "A record was added to a module you can access.".to_string(),
                record.uid.clone(),
                json!({"record_uid":record.uid,"module_uid":module_uid}),
            ));
            present_response(StatusCode::CREATED, &claims, json!(record)).await
        }
        Ok(Err(error)) => database_error_response(error),
        Err(_) => api_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"response":"record was created but could not be reloaded"}),
        ),
    }
}

async fn update_record_for_module(
    claims: Claims,
    module_uid: String,
    uid: String,
    request: DynamicRecordRequest,
) -> Response {
    if !claims.can_write_records()
        || !module_permission(&module_uid, &claims.uid, claims.access_level, "update").await
    {
        return access_denied();
    }
    let include_attachments =
        module_permission(&module_uid, &claims.uid, claims.access_level, "attachments").await;
    let schema = match load_active_module_schema(module_uid.clone()).await {
        Ok(schema) => schema,
        Err(_) => {
            return api_json(
                StatusCode::NOT_FOUND,
                json!({"response":"module structure was not found"}),
            );
        }
    };
    let normalized = match validate_payload(&schema.fields, &request.values) {
        Ok(values) => values,
        Err(error) => return api_json(StatusCode::BAD_REQUEST, json!({"response":error})),
    };
    let db_uid = uid.clone();
    let db_module_uid = module_uid.clone();
    let actor_uid = claims.uid.clone();
    let base_revision = request.base_revision;
    match tokio::task::spawn_blocking(move || {
        update_record_db(db_module_uid, db_uid, normalized, actor_uid, base_revision)
    })
    .await
    {
        Ok(Ok(())) => {}
        Ok(Err(error)) => return database_error_response(error),
        Err(_) => {
            return api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"response":"record update task failed"}),
            );
        }
    }
    match tokio::task::spawn_blocking({
        let module_uid = module_uid.clone();
        move || get_record_db(module_uid, uid, include_attachments)
    })
    .await
    {
        Ok(Ok(record)) => {
            publish_live_event(
                "record.updated",
                Some(&claims.uid),
                json!({"record_uid":record.uid,"module_uid":module_uid}),
            );
            tokio::spawn(notify_module_readers(
                module_uid.clone(),
                claims.uid.clone(),
                "record.updated".to_string(),
                "Record updated".to_string(),
                "A record in a module you can access was updated.".to_string(),
                record.uid.clone(),
                json!({"record_uid":record.uid,"module_uid":module_uid}),
            ));
            present_response(StatusCode::OK, &claims, json!(record)).await
        }
        Ok(Err(error)) => database_error_response(error),
        Err(_) => api_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"response":"record was updated but could not be reloaded"}),
        ),
    }
}

async fn get_record_for_module(claims: Claims, module_uid: String, uid: String) -> Response {
    if !claims.can_read_records()
        || !module_permission(&module_uid, &claims.uid, claims.access_level, "read").await
    {
        return access_denied();
    }
    let include_attachments =
        module_permission(&module_uid, &claims.uid, claims.access_level, "attachments").await;
    match tokio::task::spawn_blocking(move || get_record_db(module_uid, uid, include_attachments))
        .await
    {
        Ok(Ok(record)) => present_response(StatusCode::OK, &claims, json!(record)).await,
        Ok(Err(error)) => database_error_response(error),
        Err(_) => api_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"response":"record lookup task failed"}),
        ),
    }
}

async fn patch_record_for_module(
    claims: Claims,
    module_uid: String,
    uid: String,
    request: DynamicPatchRequest,
) -> Response {
    if !claims.can_write_records()
        || !module_permission(&module_uid, &claims.uid, claims.access_level, "update").await
    {
        return access_denied();
    }
    let include_attachments =
        module_permission(&module_uid, &claims.uid, claims.access_level, "attachments").await;
    let schema = match load_active_module_schema(module_uid.clone()).await {
        Ok(schema) => schema,
        Err(_) => {
            return api_json(
                StatusCode::NOT_FOUND,
                json!({"response":"module structure was not found"}),
            );
        }
    };
    let normalized = match validate_patch_payload(&schema.fields, &request.changes) {
        Ok(values) => values,
        Err(error) => return api_json(StatusCode::BAD_REQUEST, json!({"response":error})),
    };
    if normalized.is_empty() {
        return get_record_for_module(claims, module_uid, uid).await;
    }
    let raw_changes = request.changes.clone();
    let base_revision = request.base_revision;
    let actor_uid = claims.uid.clone();
    let db_uid = uid.clone();
    let db_module_uid = module_uid.clone();
    let outcome = match tokio::task::spawn_blocking(move || {
        patch_record_db(
            db_module_uid,
            db_uid,
            normalized,
            raw_changes,
            base_revision,
            actor_uid,
        )
    })
    .await
    {
        Ok(Ok(PatchDbResult::Applied(outcome))) => outcome,
        Ok(Ok(PatchDbResult::Conflict {
            conflicts,
            current_revision,
        })) => {
            return api_json(
                StatusCode::CONFLICT,
                json!({"error":"field_conflict","response":"One or more of the submitted fields changed after you opened this record.","record_uid":uid,"base_revision":request.base_revision,"current_record_revision":current_revision,"conflicts":conflicts}),
            );
        }
        Ok(Ok(PatchDbResult::NotFound)) => {
            return api_json(
                StatusCode::NOT_FOUND,
                json!({"response":"MX record was not found."}),
            );
        }
        Ok(Err(error)) => return database_error_response(error),
        Err(_) => {
            return api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"response":"record patch task failed"}),
            );
        }
    };
    publish_live_event(
        "record.fields.updated",
        Some(&claims.uid),
        json!({"record_uid":uid,"module_uid":module_uid,"changes":outcome.changed_values.iter().filter(|(key,_)|!schema.fields.iter().any(|f|f.key==**key && f.field_type=="relationship")).collect::<BTreeMap<_,_>>(),"record_revision":outcome.revision}),
    );
    tokio::spawn(notify_module_readers(
        module_uid.clone(),
        claims.uid.clone(),
        "record.updated".to_string(),
        "Record updated".to_string(),
        "Fields changed on a record in a module you can access.".to_string(),
        uid.clone(),
        json!({"record_uid":uid,"module_uid":module_uid}),
    ));
    match tokio::task::spawn_blocking(move || get_record_db(module_uid, uid, include_attachments))
        .await
    {
        Ok(Ok(record)) => present_response(StatusCode::OK, &claims, json!(record)).await,
        Ok(Err(error)) => database_error_response(error),
        Err(_) => api_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"response":"record was updated but could not be reloaded"}),
        ),
    }
}

async fn list_records_for_module(
    claims: Claims,
    module_uid: String,
    query: DynamicListQuery,
) -> Response {
    if !claims.can_read_records()
        || !module_permission(&module_uid, &claims.uid, claims.access_level, "read").await
    {
        return access_denied();
    }
    let include_attachments =
        module_permission(&module_uid, &claims.uid, claims.access_level, "attachments").await;
    match tokio::task::spawn_blocking(move || {
        list_records_db(module_uid, query, include_attachments)
    })
    .await
    {
        Ok(Ok(page)) => present_response(StatusCode::OK, &claims, json!(page)).await,
        Ok(Err(error)) => database_error_response(error),
        Err(_) => api_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"response":"record list task failed"}),
        ),
    }
}

pub async fn create_mx_record(
    claims: Claims,
    Json(request): Json<DynamicRecordRequest>,
) -> Response {
    create_record_for_module(claims, DEFAULT_MODULE_UID.to_string(), request).await
}
pub async fn create_module_record(
    claims: Claims,
    Path(module_uid): Path<String>,
    Json(request): Json<DynamicRecordRequest>,
) -> Response {
    create_record_for_module(claims, module_uid, request).await
}
pub async fn update_mx_record(
    claims: Claims,
    Path(uid): Path<String>,
    Json(request): Json<DynamicRecordRequest>,
) -> Response {
    update_record_for_module(claims, DEFAULT_MODULE_UID.to_string(), uid, request).await
}
pub async fn update_module_record(
    claims: Claims,
    Path((module_uid, uid)): Path<(String, String)>,
    Json(request): Json<DynamicRecordRequest>,
) -> Response {
    update_record_for_module(claims, module_uid, uid, request).await
}
pub async fn get_mx_record(claims: Claims, Path(uid): Path<String>) -> Response {
    get_record_for_module(claims, DEFAULT_MODULE_UID.to_string(), uid).await
}
pub async fn get_module_record(
    claims: Claims,
    Path((module_uid, uid)): Path<(String, String)>,
) -> Response {
    get_record_for_module(claims, module_uid, uid).await
}
pub async fn patch_mx_record(
    claims: Claims,
    Path(uid): Path<String>,
    Json(request): Json<DynamicPatchRequest>,
) -> Response {
    patch_record_for_module(claims, DEFAULT_MODULE_UID.to_string(), uid, request).await
}
pub async fn patch_module_record(
    claims: Claims,
    Path((module_uid, uid)): Path<(String, String)>,
    Json(request): Json<DynamicPatchRequest>,
) -> Response {
    patch_record_for_module(claims, module_uid, uid, request).await
}
pub async fn list_mx_records(claims: Claims, Query(query): Query<DynamicListQuery>) -> Response {
    list_records_for_module(claims, DEFAULT_MODULE_UID.to_string(), query).await
}
pub async fn list_module_records(
    claims: Claims,
    Path(module_uid): Path<String>,
    Query(query): Query<DynamicListQuery>,
) -> Response {
    list_records_for_module(claims, module_uid, query).await
}

#[cfg(test)]
mod concurrency_tests {
    use super::{field_changed_after_base, validate_replacement_revision};
    use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

    #[test]
    fn full_replacement_requires_the_exact_loaded_revision() {
        assert!(validate_replacement_revision(Some(7), 7).is_ok());
        for base in [None, Some(0), Some(6), Some(8)] {
            assert!(validate_replacement_revision(base, 7).is_err());
        }
    }

    fn save_field(
        connection: &Connection,
        base_revision: i64,
        field_uid: &str,
    ) -> rusqlite::Result<Option<i64>> {
        let transaction =
            rusqlite::Transaction::new_unchecked(connection, TransactionBehavior::Immediate)?;
        let current_revision: i64 = transaction.query_row(
            "SELECT revision FROM records WHERE uid='record'",
            [],
            |row| row.get(0),
        )?;
        let field_revision = transaction
            .query_row(
                "SELECT revision FROM field_revisions WHERE field_uid=?1",
                params![field_uid],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .unwrap_or(0);
        if field_changed_after_base(current_revision, base_revision, field_revision) {
            return Ok(None);
        }
        let next_revision = current_revision + 1;
        let affected = transaction.execute(
            "UPDATE records SET revision=?1 WHERE uid='record' AND revision=?2",
            params![next_revision, current_revision],
        )?;
        assert_eq!(affected, 1);
        transaction.execute(
            "INSERT INTO field_revisions(field_uid,revision) VALUES(?1,?2) ON CONFLICT(field_uid) DO UPDATE SET revision=excluded.revision",
            params![field_uid, next_revision],
        )?;
        transaction.commit()?;
        Ok(Some(next_revision))
    }

    #[test]
    fn unrelated_field_changes_merge() {
        assert!(!field_changed_after_base(8, 6, 5));
        assert!(!field_changed_after_base(8, 6, 6));
    }

    #[test]
    fn same_field_change_conflicts() {
        assert!(field_changed_after_base(8, 6, 7));
        assert!(field_changed_after_base(8, 6, 8));
    }

    #[test]
    fn unchanged_record_never_conflicts() {
        assert!(!field_changed_after_base(6, 6, 99));
    }

    #[test]
    fn sqlite_commit_merges_unrelated_stale_edits_and_rejects_same_field() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE records(uid TEXT PRIMARY KEY,revision INTEGER NOT NULL);\
                 CREATE TABLE field_revisions(field_uid TEXT PRIMARY KEY,revision INTEGER NOT NULL);\
                 INSERT INTO records(uid,revision) VALUES('record',1);\
                 INSERT INTO field_revisions(field_uid,revision) VALUES('status',1),('office',1);",
            )
            .unwrap();

        assert_eq!(save_field(&connection, 1, "status").unwrap(), Some(2));
        assert_eq!(save_field(&connection, 1, "office").unwrap(), Some(3));
        assert_eq!(save_field(&connection, 1, "status").unwrap(), None);
        assert_eq!(
            connection
                .query_row(
                    "SELECT revision FROM records WHERE uid='record'",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            3
        );
    }
}
