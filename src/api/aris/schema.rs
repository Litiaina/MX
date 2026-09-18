use std::collections::{BTreeMap, HashSet};

use axum::{
    Json,
    extract::Path,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    api::aris::{
        handler::ensure_aris_record_schema,
        storage::{ensure_storage_layout_schema, field_used_by_storage_layout_db},
    },
    db::connector::{SqliteDatabaseError, with_sql_connection},
    middleware::auth::Claims,
};

const DEFAULT_TABLE_PRIORITY: i64 = 50;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldDefinition {
    pub uid: String,
    pub key: String,
    pub label: String,
    pub field_type: String,
    pub required: bool,
    pub unique_value: bool,
    pub searchable: bool,
    pub sortable: bool,
    pub table_visible: bool,
    pub table_priority: i64,
    pub position: i64,
    pub active: bool,
    pub config: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemFieldDefinition {
    pub key: String,
    pub label: String,
    pub field_type: String,
    pub searchable: bool,
    pub sortable: bool,
    pub table_visible: bool,
    pub table_priority: i64,
    pub position: i64,
}

#[derive(Debug, Serialize)]
pub struct SchemaResponse {
    pub revision: u64,
    pub fields: Vec<FieldDefinition>,
    pub system_fields: Vec<SystemFieldDefinition>,
}

#[derive(Debug, Deserialize)]
pub struct CreateFieldRequest {
    pub key: String,
    pub label: String,
    pub field_type: String,

    #[serde(default)]
    pub required: bool,

    #[serde(default)]
    pub unique_value: bool,

    #[serde(default = "default_true")]
    pub searchable: bool,

    #[serde(default = "default_true")]
    pub sortable: bool,

    #[serde(default = "default_true")]
    pub table_visible: bool,

    #[serde(default = "default_table_priority")]
    pub table_priority: i64,

    #[serde(default)]
    pub position: Option<i64>,

    #[serde(default)]
    pub config: Value,
}

#[derive(Debug, Deserialize)]
pub struct UpdateFieldRequest {
    pub label: Option<String>,
    pub required: Option<bool>,
    pub unique_value: Option<bool>,
    pub searchable: Option<bool>,
    pub sortable: Option<bool>,
    pub table_visible: Option<bool>,
    pub table_priority: Option<i64>,
    pub position: Option<i64>,
    pub active: Option<bool>,
    pub config: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSystemFieldRequest {
    pub table_visible: Option<bool>,
    pub table_priority: Option<i64>,
    pub position: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSchemaOrderRequest {
    pub items: Vec<SchemaOrderItem>,
}

#[derive(Debug, Deserialize)]
pub struct SchemaOrderItem {
    pub kind: String,
    pub id: String,
}

fn default_true() -> bool {
    true
}

fn default_table_priority() -> i64 {
    DEFAULT_TABLE_PRIORITY
}

fn api_json(status: StatusCode, body: Value) -> Response {
    (status, Json(body)).into_response()
}

fn access_denied() -> Response {
    api_json(
        StatusCode::FORBIDDEN,
        json!({
            "response": "administrator access is required to modify the ARIS record structure"
        }),
    )
}

fn read_denied() -> Response {
    api_json(
        StatusCode::FORBIDDEN,
        json!({
            "response": "your account does not have permission to read the ARIS record structure"
        }),
    )
}

fn normalize_key(value: &str) -> Result<String, String> {
    let key = value.trim().to_ascii_lowercase();

    if key.is_empty() {
        return Err("Field key is required.".to_string());
    }

    if key.len() > 64 {
        return Err("Field key cannot exceed 64 characters.".to_string());
    }

    if !key.chars().all(|character| {
        character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
    }) {
        return Err(
            "Field key may contain only lowercase letters, numbers, and underscores.".to_string(),
        );
    }

    if key
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_digit())
    {
        return Err("Field key cannot begin with a number.".to_string());
    }

    Ok(key)
}

fn normalize_type(value: &str) -> Result<String, String> {
    let field_type = value.trim().to_ascii_lowercase();

    match field_type.as_str() {
        "text" | "long_text" | "integer" | "decimal" | "date" | "boolean" | "select"
        | "auto_number" => Ok(field_type),

        _ => Err(format!("Unsupported ARIS field type '{field_type}'.")),
    }
}

fn validate_config(field_type: &str, config: &Value) -> Result<Value, String> {
    match field_type {
        "select" => {
            let options = config
                .get("options")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    "Select fields require config.options as an array of values.".to_string()
                })?;

            let mut cleaned: Vec<String> = Vec::new();

            for option in options {
                let value = option.as_str().unwrap_or_default().trim().to_string();

                if value.is_empty() {
                    continue;
                }

                if !cleaned
                    .iter()
                    .any(|existing| existing.eq_ignore_ascii_case(&value))
                {
                    cleaned.push(value);
                }
            }

            if cleaned.is_empty() {
                return Err("Select fields require at least one non-empty option.".to_string());
            }

            Ok(json!({ "options": cleaned }))
        }

        "auto_number" => {
            let scope = config
                .get("scope")
                .and_then(Value::as_str)
                .unwrap_or("global")
                .trim()
                .to_ascii_lowercase();

            if scope != "global" && scope != "yearly" {
                return Err("Auto Number scope must be 'global' or 'yearly'.".to_string());
            }

            let prefix = config
                .get("prefix")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string();

            let padding = config
                .get("padding")
                .and_then(Value::as_u64)
                .unwrap_or(0)
                .min(12);

            Ok(json!({
                "scope": scope,
                "prefix": prefix,
                "padding": padding,
            }))
        }

        _ => Ok(json!({})),
    }
}

fn bool_from_i64(value: i64) -> bool {
    value != 0
}

fn row_to_field(row: &rusqlite::Row<'_>) -> rusqlite::Result<FieldDefinition> {
    let config_text: String = row.get(12)?;

    Ok(FieldDefinition {
        uid: row.get(0)?,
        key: row.get(1)?,
        label: row.get(2)?,
        field_type: row.get(3)?,
        required: bool_from_i64(row.get(4)?),
        unique_value: bool_from_i64(row.get(5)?),
        searchable: bool_from_i64(row.get(6)?),
        sortable: bool_from_i64(row.get(7)?),
        table_visible: bool_from_i64(row.get(8)?),
        table_priority: row.get(9)?,
        position: row.get(10)?,
        active: bool_from_i64(row.get(11)?),
        config: serde_json::from_str(&config_text).unwrap_or_else(|_| json!({})),
    })
}

fn insert_builtin_field(
    connection: &rusqlite::Connection,
    uid: &str,
    key: &str,
    label: &str,
    field_type: &str,
    required: bool,
    unique_value: bool,
    searchable: bool,
    sortable: bool,
    table_visible: bool,
    table_priority: i64,
    position: i64,
    config: &Value,
) -> rusqlite::Result<()> {
    connection.execute(
        r#"
        INSERT OR IGNORE INTO aris_fields (
            uid,
            field_key,
            label,
            field_type,
            required,
            unique_value,
            searchable,
            sortable,
            table_visible,
            table_priority,
            position,
            active,
            config_json
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7,
            ?8, ?9, ?10, ?11, 1, ?12
        )
        "#,
        params![
            uid,
            key,
            label,
            field_type,
            if required { 1_i64 } else { 0_i64 },
            if unique_value { 1_i64 } else { 0_i64 },
            if searchable { 1_i64 } else { 0_i64 },
            if sortable { 1_i64 } else { 0_i64 },
            if table_visible { 1_i64 } else { 0_i64 },
            table_priority,
            position,
            config.to_string(),
        ],
    )?;

    Ok(())
}

pub(crate) fn ensure_dynamic_schema(connection: &rusqlite::Connection) -> rusqlite::Result<()> {
    ensure_aris_record_schema(connection)?;

    connection.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS aris_schema_meta (
            id               INTEGER PRIMARY KEY CHECK(id = 1),
            schema_revision  INTEGER NOT NULL DEFAULT 1 CHECK(schema_revision >= 1),
            legacy_migrated  INTEGER NOT NULL DEFAULT 0 CHECK(legacy_migrated IN (0, 1))
        );

        INSERT OR IGNORE INTO aris_schema_meta (
            id,
            schema_revision,
            legacy_migrated
        ) VALUES (1, 1, 0);

        CREATE TABLE IF NOT EXISTS aris_system_fields (
            field_key       TEXT PRIMARY KEY NOT NULL COLLATE NOCASE,
            label           TEXT NOT NULL,
            field_type      TEXT NOT NULL,
            searchable      INTEGER NOT NULL DEFAULT 1 CHECK(searchable IN (0, 1)),
            sortable        INTEGER NOT NULL DEFAULT 0 CHECK(sortable IN (0, 1)),
            table_visible   INTEGER NOT NULL DEFAULT 1 CHECK(table_visible IN (0, 1)),
            table_priority  INTEGER NOT NULL DEFAULT 80,
            position        INTEGER NOT NULL DEFAULT 1000000
        );

        INSERT OR IGNORE INTO aris_system_fields (
            field_key,
            label,
            field_type,
            searchable,
            sortable,
            table_visible,
            table_priority,
            position
        ) VALUES (
            'attachments',
            'Attached Files',
            'attachments',
            1,
            0,
            1,
            80,
            1000000
        );

        CREATE TABLE IF NOT EXISTS aris_fields (
            uid             TEXT PRIMARY KEY NOT NULL,
            field_key       TEXT NOT NULL UNIQUE COLLATE NOCASE,
            label           TEXT NOT NULL,
            field_type      TEXT NOT NULL,
            required        INTEGER NOT NULL DEFAULT 0 CHECK(required IN (0, 1)),
            unique_value    INTEGER NOT NULL DEFAULT 0 CHECK(unique_value IN (0, 1)),
            searchable      INTEGER NOT NULL DEFAULT 1 CHECK(searchable IN (0, 1)),
            sortable        INTEGER NOT NULL DEFAULT 1 CHECK(sortable IN (0, 1)),
            table_visible   INTEGER NOT NULL DEFAULT 1 CHECK(table_visible IN (0, 1)),
            table_priority  INTEGER NOT NULL DEFAULT 50,
            position        INTEGER NOT NULL DEFAULT 0,
            active          INTEGER NOT NULL DEFAULT 1 CHECK(active IN (0, 1)),
            config_json     TEXT NOT NULL DEFAULT '{}'
        );

        CREATE INDEX IF NOT EXISTS idx_aris_fields_active_position
            ON aris_fields(active, position, label COLLATE NOCASE);

        CREATE INDEX IF NOT EXISTS idx_aris_fields_table
            ON aris_fields(active, table_visible, table_priority DESC, position);

        CREATE TABLE IF NOT EXISTS aris_record_values (
            record_uid      TEXT NOT NULL,
            field_uid       TEXT NOT NULL,
            value_text      TEXT,
            value_integer   INTEGER,
            value_real      REAL,
            value_boolean   INTEGER CHECK(value_boolean IS NULL OR value_boolean IN (0, 1)),
            PRIMARY KEY(record_uid, field_uid),
            FOREIGN KEY(record_uid)
                REFERENCES aris_records(uid)
                ON DELETE CASCADE,
            FOREIGN KEY(field_uid)
                REFERENCES aris_fields(uid)
                ON DELETE RESTRICT
        );

        CREATE INDEX IF NOT EXISTS idx_aris_record_values_field_text
            ON aris_record_values(field_uid, value_text COLLATE NOCASE, record_uid);

        CREATE INDEX IF NOT EXISTS idx_aris_record_values_field_integer
            ON aris_record_values(field_uid, value_integer, record_uid);

        CREATE INDEX IF NOT EXISTS idx_aris_record_values_field_real
            ON aris_record_values(field_uid, value_real, record_uid);

        CREATE INDEX IF NOT EXISTS idx_aris_record_values_record
            ON aris_record_values(record_uid, field_uid);

        CREATE TABLE IF NOT EXISTS aris_unique_values (
            field_uid         TEXT NOT NULL,
            normalized_value  TEXT NOT NULL,
            record_uid        TEXT NOT NULL,
            PRIMARY KEY(field_uid, normalized_value),
            UNIQUE(field_uid, record_uid),
            FOREIGN KEY(field_uid)
                REFERENCES aris_fields(uid)
                ON DELETE RESTRICT,
            FOREIGN KEY(record_uid)
                REFERENCES aris_records(uid)
                ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS aris_field_sequences (
            field_uid   TEXT NOT NULL,
            scope_key   TEXT NOT NULL,
            last_value  INTEGER NOT NULL DEFAULT 0 CHECK(last_value >= 0),
            PRIMARY KEY(field_uid, scope_key),
            FOREIGN KEY(field_uid)
                REFERENCES aris_fields(uid)
                ON DELETE RESTRICT
        );
        "#,
    )?;

    let record_count: i64 =
        connection.query_row("SELECT COUNT(*) FROM aris_records", [], |row| row.get(0))?;

    /*
     * Fresh deployments own their structure. ARIS must not invent one.
     *
     * Earlier dynamic builds accidentally inserted seven historical fields
     * into empty databases. Those defaults used fixed UIDs, so when there
     * are no records we can safely remove only those generated defaults
     * without touching Administrator-created fields.
     */
    if record_count == 0 {
        connection.execute(
            r#"
            DELETE FROM aris_field_sequences
            WHERE field_uid IN (
                'aris-field-control-no',
                'aris-field-date',
                'aris-field-office',
                'aris-field-requestor',
                'aris-field-subject',
                'aris-field-route',
                'aris-field-remarks'
            )
            "#,
            [],
        )?;

        connection.execute(
            r#"
            DELETE FROM aris_unique_values
            WHERE field_uid IN (
                'aris-field-control-no',
                'aris-field-date',
                'aris-field-office',
                'aris-field-requestor',
                'aris-field-subject',
                'aris-field-route',
                'aris-field-remarks'
            )
            "#,
            [],
        )?;

        connection.execute(
            r#"
            DELETE FROM aris_record_values
            WHERE field_uid IN (
                'aris-field-control-no',
                'aris-field-date',
                'aris-field-office',
                'aris-field-requestor',
                'aris-field-subject',
                'aris-field-route',
                'aris-field-remarks'
            )
            "#,
            [],
        )?;

        let removed_defaults = connection.execute(
            r#"
            DELETE FROM aris_fields
            WHERE uid IN (
                'aris-field-control-no',
                'aris-field-date',
                'aris-field-office',
                'aris-field-requestor',
                'aris-field-subject',
                'aris-field-route',
                'aris-field-remarks'
            )
            "#,
            [],
        )?;

        if removed_defaults > 0 {
            connection.execute(
                r#"
                UPDATE aris_schema_meta
                SET schema_revision = schema_revision + 1
                WHERE id = 1
                "#,
                [],
            )?;
        }
    }

    let field_count: i64 =
        connection.query_row("SELECT COUNT(*) FROM aris_fields", [], |row| row.get(0))?;

    /*
     * Compatibility only.
     *
     * If actual legacy records exist, expose the historical columns through
     * the dynamic field system so existing deployments migrate safely.
     * A fresh deployment never receives these fields.
     */
    if record_count > 0 && field_count == 0 {
        insert_builtin_field(
            connection,
            "aris-field-control-no",
            "control_no",
            "Control No.",
            "auto_number",
            true,
            true,
            true,
            true,
            true,
            100,
            0,
            &json!({
                "scope": "yearly",
                "prefix": "",
                "padding": 0,
            }),
        )?;

        insert_builtin_field(
            connection,
            "aris-field-date",
            "date",
            "Date",
            "date",
            true,
            false,
            true,
            true,
            true,
            90,
            10,
            &json!({}),
        )?;

        insert_builtin_field(
            connection,
            "aris-field-office",
            "office",
            "Office",
            "text",
            true,
            false,
            true,
            true,
            true,
            80,
            20,
            &json!({}),
        )?;

        insert_builtin_field(
            connection,
            "aris-field-requestor",
            "requestor",
            "Requestor",
            "text",
            true,
            false,
            true,
            true,
            true,
            60,
            30,
            &json!({}),
        )?;

        insert_builtin_field(
            connection,
            "aris-field-subject",
            "subject",
            "Subject",
            "long_text",
            true,
            false,
            true,
            true,
            true,
            95,
            40,
            &json!({}),
        )?;

        insert_builtin_field(
            connection,
            "aris-field-route",
            "routed_to_div",
            "Routed To Div.",
            "text",
            true,
            false,
            true,
            true,
            true,
            50,
            50,
            &json!({}),
        )?;

        insert_builtin_field(
            connection,
            "aris-field-remarks",
            "remarks",
            "Remarks",
            "long_text",
            false,
            false,
            true,
            true,
            false,
            20,
            60,
            &json!({}),
        )?;
    }

    let legacy_migrated: i64 = connection.query_row(
        "SELECT legacy_migrated FROM aris_schema_meta WHERE id = 1",
        [],
        |row| row.get(0),
    )?;

    if legacy_migrated == 0 && record_count > 0 {
        connection.execute_batch(
            r#"
            INSERT OR IGNORE INTO aris_record_values (
                record_uid,
                field_uid,
                value_integer
            )
            SELECT
                r.uid,
                f.uid,
                r.control_no
            FROM aris_records r
            JOIN aris_fields f
              ON f.field_key = 'control_no' COLLATE NOCASE;

            INSERT OR IGNORE INTO aris_record_values (
                record_uid,
                field_uid,
                value_text
            )
            SELECT r.uid, f.uid, r.date
            FROM aris_records r
            JOIN aris_fields f
              ON f.field_key = 'date' COLLATE NOCASE;

            INSERT OR IGNORE INTO aris_record_values (
                record_uid,
                field_uid,
                value_text
            )
            SELECT r.uid, f.uid, r.office
            FROM aris_records r
            JOIN aris_fields f
              ON f.field_key = 'office' COLLATE NOCASE;

            INSERT OR IGNORE INTO aris_record_values (
                record_uid,
                field_uid,
                value_text
            )
            SELECT r.uid, f.uid, r.requestor
            FROM aris_records r
            JOIN aris_fields f
              ON f.field_key = 'requestor' COLLATE NOCASE;

            INSERT OR IGNORE INTO aris_record_values (
                record_uid,
                field_uid,
                value_text
            )
            SELECT r.uid, f.uid, r.subject
            FROM aris_records r
            JOIN aris_fields f
              ON f.field_key = 'subject' COLLATE NOCASE;

            INSERT OR IGNORE INTO aris_record_values (
                record_uid,
                field_uid,
                value_text
            )
            SELECT r.uid, f.uid, r.routed_to_div
            FROM aris_records r
            JOIN aris_fields f
              ON f.field_key = 'routed_to_div' COLLATE NOCASE;

            INSERT OR IGNORE INTO aris_record_values (
                record_uid,
                field_uid,
                value_text
            )
            SELECT r.uid, f.uid, r.remarks
            FROM aris_records r
            JOIN aris_fields f
              ON f.field_key = 'remarks' COLLATE NOCASE;

            /*
             * Seed the dynamic yearly Auto Number sequence from the legacy
             * control-number data. This prevents the first dynamic record from
             * starting at 1 again on an existing deployment.
             */
            INSERT INTO aris_field_sequences (
                field_uid,
                scope_key,
                last_value
            )
            SELECT
                f.uid,
                CAST(r.control_year AS TEXT),
                MAX(r.control_no)
            FROM aris_records r
            JOIN aris_fields f
              ON f.field_key = 'control_no' COLLATE NOCASE
            WHERE r.control_year > 0
            GROUP BY f.uid, r.control_year
            ON CONFLICT(field_uid, scope_key)
            DO UPDATE SET
                last_value = MAX(last_value, excluded.last_value);

            UPDATE aris_schema_meta
            SET legacy_migrated = 1
            WHERE id = 1;
            "#,
        )?;
    } else if legacy_migrated == 0 {
        connection.execute(
            r#"
            UPDATE aris_schema_meta
            SET legacy_migrated = 1
            WHERE id = 1
            "#,
            [],
        )?;
    }

    Ok(())
}

pub(crate) fn schema_revision_db(connection: &rusqlite::Connection) -> rusqlite::Result<u64> {
    ensure_dynamic_schema(connection)?;

    let revision: i64 = connection.query_row(
        "SELECT schema_revision FROM aris_schema_meta WHERE id = 1",
        [],
        |row| row.get(0),
    )?;

    Ok(revision.max(1) as u64)
}

fn bump_schema_revision(connection: &rusqlite::Connection) -> rusqlite::Result<()> {
    connection.execute(
        r#"
        UPDATE aris_schema_meta
        SET schema_revision = schema_revision + 1
        WHERE id = 1
        "#,
        [],
    )?;

    Ok(())
}

pub(crate) fn load_fields_db(
    connection: &rusqlite::Connection,
    include_archived: bool,
) -> rusqlite::Result<Vec<FieldDefinition>> {
    ensure_dynamic_schema(connection)?;

    let sql = if include_archived {
        r#"
        SELECT
            uid,
            field_key,
            label,
            field_type,
            required,
            unique_value,
            searchable,
            sortable,
            table_visible,
            table_priority,
            position,
            active,
            config_json
        FROM aris_fields
        ORDER BY active DESC, position ASC, label COLLATE NOCASE ASC
        "#
    } else {
        r#"
        SELECT
            uid,
            field_key,
            label,
            field_type,
            required,
            unique_value,
            searchable,
            sortable,
            table_visible,
            table_priority,
            position,
            active,
            config_json
        FROM aris_fields
        WHERE active = 1
        ORDER BY position ASC, label COLLATE NOCASE ASC
        "#
    };

    let mut statement = connection.prepare(sql)?;
    let rows = statement.query_map([], row_to_field)?;

    rows.collect::<Result<Vec<_>, _>>()
}

fn row_to_system_field(row: &rusqlite::Row<'_>) -> rusqlite::Result<SystemFieldDefinition> {
    Ok(SystemFieldDefinition {
        key: row.get(0)?,
        label: row.get(1)?,
        field_type: row.get(2)?,
        searchable: row.get::<_, i64>(3)? != 0,
        sortable: row.get::<_, i64>(4)? != 0,
        table_visible: row.get::<_, i64>(5)? != 0,
        table_priority: row.get(6)?,
        position: row.get(7)?,
    })
}

fn load_system_fields_db(
    connection: &rusqlite::Connection,
) -> rusqlite::Result<Vec<SystemFieldDefinition>> {
    ensure_dynamic_schema(connection)?;

    let mut statement = connection.prepare(
        r#"
        SELECT
            field_key,
            label,
            field_type,
            searchable,
            sortable,
            table_visible,
            table_priority,
            position
        FROM aris_system_fields
        ORDER BY position ASC, label COLLATE NOCASE ASC
        "#,
    )?;

    statement
        .query_map([], row_to_system_field)?
        .collect::<Result<Vec<_>, rusqlite::Error>>()
}

pub(crate) fn field_map_by_key(fields: &[FieldDefinition]) -> BTreeMap<String, FieldDefinition> {
    fields
        .iter()
        .cloned()
        .map(|field| (field.key.clone(), field))
        .collect()
}

async fn load_schema(include_archived: bool) -> Result<SchemaResponse, String> {
    let database_result =
        tokio::task::spawn_blocking(move || -> Result<SchemaResponse, SqliteDatabaseError> {
            with_sql_connection(|connection| {
                let revision = schema_revision_db(connection)?;
                let fields = load_fields_db(connection, include_archived)?;
                let system_fields = load_system_fields_db(connection)?;

                Ok(SchemaResponse {
                    revision,
                    fields,
                    system_fields,
                })
            })
        })
        .await;

    match database_result {
        Ok(Ok(schema)) => Ok(schema),
        Ok(Err(error)) => Err(format!("failed to load ARIS record structure: {error}")),
        Err(error) => Err(format!("record structure blocking task failed: {error}")),
    }
}

pub(crate) async fn load_active_schema() -> Result<SchemaResponse, String> {
    load_schema(false).await
}

pub async fn get_record_schema(claims: Claims) -> Response {
    if !claims.can_read_records() {
        return read_denied();
    }

    match load_schema(false).await {
        Ok(schema) => api_json(StatusCode::OK, json!(schema)),
        Err(error) => {
            crate::report_error!(error, "function", "get_record_schema()");

            api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({ "response": "failed to load ARIS record structure" }),
            )
        }
    }
}

pub async fn get_admin_record_schema(claims: Claims) -> Response {
    if !claims.can_manage_accounts() {
        return access_denied();
    }

    match load_schema(true).await {
        Ok(schema) => api_json(StatusCode::OK, json!(schema)),
        Err(error) => {
            crate::report_error!(error, "function", "get_admin_record_schema()");

            api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({ "response": "failed to load ARIS record structure" }),
            )
        }
    }
}

pub async fn create_schema_field(
    claims: Claims,
    Json(request): Json<CreateFieldRequest>,
) -> Response {
    if !claims.can_manage_accounts() {
        return access_denied();
    }

    let key = match normalize_key(&request.key) {
        Ok(value) => value,
        Err(error) => {
            return api_json(StatusCode::BAD_REQUEST, json!({ "response": error }));
        }
    };

    let label = request.label.trim().to_string();

    if label.is_empty() {
        return api_json(
            StatusCode::BAD_REQUEST,
            json!({ "response": "Field label is required." }),
        );
    }

    let field_type = match normalize_type(&request.field_type) {
        Ok(value) => value,
        Err(error) => {
            return api_json(StatusCode::BAD_REQUEST, json!({ "response": error }));
        }
    };

    let config = match validate_config(&field_type, &request.config) {
        Ok(value) => value,
        Err(error) => {
            return api_json(StatusCode::BAD_REQUEST, json!({ "response": error }));
        }
    };

    let uid = Uuid::new_v4().to_string();
    let requested_position = request.position;
    let required = request.required || field_type == "auto_number";
    let unique_value = request.unique_value || field_type == "auto_number";
    let sortable = request.sortable && field_type != "long_text";
    let key_for_db = key.clone();
    let label_for_db = label.clone();
    let field_type_for_db = field_type.clone();
    let config_for_db = config.to_string();
    let uid_for_db = uid.clone();

    let database_result =
        tokio::task::spawn_blocking(move || -> Result<FieldDefinition, SqliteDatabaseError> {
            with_sql_connection(|connection| {
                ensure_dynamic_schema(connection)?;

                let position = match requested_position {
                    Some(value) => value.max(0),
                    None => connection.query_row(
                        r#"
                        SELECT COALESCE(MAX(position), -10) + 10
                        FROM (
                            SELECT position
                            FROM aris_fields
                            WHERE active = 1

                            UNION ALL

                            SELECT position
                            FROM aris_system_fields
                        )
                        "#,
                        [],
                        |row| row.get::<_, i64>(0),
                    )?,
                };

                connection.execute(
                    r#"
                    INSERT INTO aris_fields (
                        uid,
                        field_key,
                        label,
                        field_type,
                        required,
                        unique_value,
                        searchable,
                        sortable,
                        table_visible,
                        table_priority,
                        position,
                        active,
                        config_json
                    ) VALUES (
                        ?1, ?2, ?3, ?4, ?5, ?6, ?7,
                        ?8, ?9, ?10, ?11, 1, ?12
                    )
                    "#,
                    params![
                        uid_for_db,
                        key_for_db,
                        label_for_db,
                        field_type_for_db,
                        if required { 1_i64 } else { 0_i64 },
                        if unique_value { 1_i64 } else { 0_i64 },
                        if request.searchable { 1_i64 } else { 0_i64 },
                        if sortable { 1_i64 } else { 0_i64 },
                        if request.table_visible { 1_i64 } else { 0_i64 },
                        request.table_priority,
                        position,
                        config_for_db,
                    ],
                )?;

                bump_schema_revision(connection)?;

                connection.query_row(
                    r#"
                    SELECT
                        uid,
                        field_key,
                        label,
                        field_type,
                        required,
                        unique_value,
                        searchable,
                        sortable,
                        table_visible,
                        table_priority,
                        position,
                        active,
                        config_json
                    FROM aris_fields
                    WHERE uid = ?1
                    "#,
                    params![uid],
                    row_to_field,
                )
            })
        })
        .await;

    match database_result {
        Ok(Ok(field)) => api_json(StatusCode::CREATED, json!(field)),

        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::SqliteFailure(error, _))))
            if error.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            api_json(
                StatusCode::CONFLICT,
                json!({
                    "response": format!("A field with key '{key}' already exists.")
                }),
            )
        }

        Ok(Err(error)) => {
            crate::report_error!(format!("{error}"), "function", "create_schema_field()");

            api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({ "response": "failed to create ARIS field" }),
            )
        }

        Err(error) => {
            crate::report_error!(format!("{error}"), "function", "create_schema_field()");

            api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({ "response": "failed to create ARIS field" }),
            )
        }
    }
}

fn duplicate_value_exists(
    connection: &rusqlite::Connection,
    field_uid: &str,
) -> rusqlite::Result<bool> {
    let duplicate = connection
        .query_row(
            r#"
            SELECT 1
            FROM (
                SELECT
                    CASE f.field_type
                        WHEN 'integer' THEN CAST(rv.value_integer AS TEXT)
                        WHEN 'auto_number' THEN CAST(rv.value_integer AS TEXT)
                        WHEN 'decimal' THEN CAST(rv.value_real AS TEXT)
                        WHEN 'boolean' THEN CAST(rv.value_boolean AS TEXT)
                        ELSE LOWER(TRIM(COALESCE(rv.value_text, '')))
                    END AS normalized,
                    COUNT(*) AS value_count
                FROM aris_record_values rv
                JOIN aris_fields f
                  ON f.uid = rv.field_uid
                WHERE rv.field_uid = ?1
                GROUP BY normalized
                HAVING normalized <> ''
                   AND value_count > 1
                LIMIT 1
            ) duplicates
            "#,
            params![field_uid],
            |_row| Ok(true),
        )
        .optional()?
        .unwrap_or(false);

    Ok(duplicate)
}

fn rebuild_unique_values_for_field(
    connection: &rusqlite::Connection,
    field: &FieldDefinition,
) -> rusqlite::Result<()> {
    connection.execute(
        "DELETE FROM aris_unique_values WHERE field_uid = ?1",
        params![&field.uid],
    )?;

    if !field.unique_value || field.field_type == "auto_number" {
        return Ok(());
    }

    let normalized_expression = match field.field_type.as_str() {
        "integer" => "CAST(rv.value_integer AS TEXT)",
        "decimal" => "printf('%.12f', rv.value_real)",
        "boolean" => "CAST(rv.value_boolean AS TEXT)",
        _ => "LOWER(TRIM(COALESCE(rv.value_text, '')))",
    };

    let sql = format!(
        r#"
        INSERT INTO aris_unique_values (
            field_uid,
            normalized_value,
            record_uid
        )
        SELECT
            rv.field_uid,
            {normalized_expression},
            rv.record_uid
        FROM aris_record_values rv
        WHERE rv.field_uid = ?1
          AND {normalized_expression} <> ''
        "#
    );

    connection.execute(&sql, params![&field.uid])?;
    Ok(())
}

pub async fn update_system_schema_field(
    claims: Claims,
    Path(key): Path<String>,
    Json(request): Json<UpdateSystemFieldRequest>,
) -> Response {
    if !claims.can_manage_accounts() {
        return access_denied();
    }

    let key = key.trim().to_ascii_lowercase();

    if key != "attachments" {
        return api_json(
            StatusCode::NOT_FOUND,
            json!({ "response": "unknown ARIS system field" }),
        );
    }

    let key_for_db = key.clone();

    let database_result = tokio::task::spawn_blocking(
        move || -> Result<SystemFieldDefinition, SqliteDatabaseError> {
            with_sql_connection(|connection| {
                ensure_dynamic_schema(connection)?;

                let existing = connection.query_row(
                    r#"
                    SELECT
                        field_key,
                        label,
                        field_type,
                        searchable,
                        sortable,
                        table_visible,
                        table_priority,
                        position
                    FROM aris_system_fields
                    WHERE field_key = ?1
                    "#,
                    params![&key_for_db],
                    row_to_system_field,
                )?;

                connection.execute(
                    r#"
                    UPDATE aris_system_fields
                    SET
                        table_visible = ?2,
                        table_priority = ?3,
                        position = ?4
                    WHERE field_key = ?1
                    "#,
                    params![
                        &key_for_db,
                        if request.table_visible.unwrap_or(existing.table_visible) {
                            1_i64
                        } else {
                            0_i64
                        },
                        request.table_priority.unwrap_or(existing.table_priority),
                        request.position.unwrap_or(existing.position).max(0),
                    ],
                )?;

                bump_schema_revision(connection)?;

                connection.query_row(
                    r#"
                    SELECT
                        field_key,
                        label,
                        field_type,
                        searchable,
                        sortable,
                        table_visible,
                        table_priority,
                        position
                    FROM aris_system_fields
                    WHERE field_key = ?1
                    "#,
                    params![&key_for_db],
                    row_to_system_field,
                )
            })
        },
    )
    .await;

    match database_result {
        Ok(Ok(field)) => api_json(
            StatusCode::OK,
            json!({
                "response": "ARIS system field updated",
                "field": field
            }),
        ),

        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::QueryReturnedNoRows))) => api_json(
            StatusCode::NOT_FOUND,
            json!({ "response": "ARIS system field was not found" }),
        ),

        Ok(Err(error)) => {
            crate::report_error!(
                format!("failed to update ARIS system field: {error}"),
                "function",
                "update_system_schema_field()"
            );

            api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({ "response": "failed to update ARIS system field" }),
            )
        }

        Err(error) => {
            crate::report_error!(
                format!("system field update blocking task failed: {error}"),
                "function",
                "update_system_schema_field()"
            );

            api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({ "response": "failed to update ARIS system field" }),
            )
        }
    }
}

pub async fn update_schema_order(
    claims: Claims,
    Json(request): Json<UpdateSchemaOrderRequest>,
) -> Response {
    if !claims.can_manage_accounts() {
        return access_denied();
    }

    let items = request.items;

    let database_result =
        tokio::task::spawn_blocking(move || -> Result<u64, SqliteDatabaseError> {
            with_sql_connection(|connection| {
                ensure_dynamic_schema(connection)?;

                let active_field_uids = {
                    let mut statement = connection.prepare(
                        r#"
                        SELECT uid
                        FROM aris_fields
                        WHERE active = 1
                        "#,
                    )?;

                    statement
                        .query_map([], |row| row.get::<_, String>(0))?
                        .collect::<Result<HashSet<_>, rusqlite::Error>>()?
                };

                let system_field_keys = {
                    let mut statement = connection.prepare(
                        r#"
                        SELECT field_key
                        FROM aris_system_fields
                        "#,
                    )?;

                    statement
                        .query_map([], |row| row.get::<_, String>(0))?
                        .collect::<Result<HashSet<_>, rusqlite::Error>>()?
                };

                if items.len() != active_field_uids.len() + system_field_keys.len() {
                    return Err(rusqlite::Error::InvalidParameterName(
                        "SCHEMA_ORDER_MISMATCH".to_string(),
                    ));
                }

                let mut seen: HashSet<String> = HashSet::new();

                for item in &items {
                    let kind = item.kind.trim().to_ascii_lowercase();
                    let id = item.id.trim();

                    if id.is_empty() {
                        return Err(rusqlite::Error::InvalidParameterName(
                            "SCHEMA_ORDER_INVALID".to_string(),
                        ));
                    }

                    if !seen.insert(format!("{kind}:{id}")) {
                        return Err(rusqlite::Error::InvalidParameterName(
                            "SCHEMA_ORDER_DUPLICATE".to_string(),
                        ));
                    }

                    match kind.as_str() {
                        "field" if active_field_uids.contains(id) => {}
                        "system" if system_field_keys.contains(id) => {}
                        "field" | "system" => {
                            return Err(rusqlite::Error::InvalidParameterName(
                                "SCHEMA_ORDER_MISMATCH".to_string(),
                            ));
                        }
                        _ => {
                            return Err(rusqlite::Error::InvalidParameterName(
                                "SCHEMA_ORDER_INVALID".to_string(),
                            ));
                        }
                    }
                }

                let transaction = connection.unchecked_transaction()?;

                for (index, item) in items.iter().enumerate() {
                    let position = (index as i64).saturating_mul(10);

                    match item.kind.trim().to_ascii_lowercase().as_str() {
                        "field" => {
                            transaction.execute(
                                r#"
                                UPDATE aris_fields
                                SET position = ?2
                                WHERE uid = ?1
                                  AND active = 1
                                "#,
                                params![item.id.trim(), position],
                            )?;
                        }

                        "system" => {
                            transaction.execute(
                                r#"
                                UPDATE aris_system_fields
                                SET position = ?2
                                WHERE field_key = ?1
                                "#,
                                params![item.id.trim(), position],
                            )?;
                        }

                        _ => unreachable!("order kind was already validated"),
                    }
                }

                transaction.execute(
                    r#"
                    UPDATE aris_schema_meta
                    SET schema_revision = schema_revision + 1
                    WHERE id = 1
                    "#,
                    [],
                )?;

                transaction.commit()?;

                schema_revision_db(connection)
            })
        })
        .await;

    match database_result {
        Ok(Ok(revision)) => api_json(
            StatusCode::OK,
            json!({
                "response": "field order updated",
                "revision": revision
            }),
        ),

        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::InvalidParameterName(message))))
            if message == "SCHEMA_ORDER_MISMATCH" =>
        {
            api_json(
                StatusCode::CONFLICT,
                json!({
                    "response": "The record structure changed while ordering fields. Refresh and try again."
                }),
            )
        }

        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::InvalidParameterName(message))))
            if message == "SCHEMA_ORDER_DUPLICATE" =>
        {
            api_json(
                StatusCode::BAD_REQUEST,
                json!({
                    "response": "The field order contains a duplicate item."
                }),
            )
        }

        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::InvalidParameterName(message))))
            if message == "SCHEMA_ORDER_INVALID" =>
        {
            api_json(
                StatusCode::BAD_REQUEST,
                json!({
                    "response": "The field order contains an invalid item."
                }),
            )
        }

        Ok(Err(error)) => {
            crate::report_error!(format!("{error}"), "function", "update_schema_order()");

            api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({ "response": "failed to update field order" }),
            )
        }

        Err(error) => {
            crate::report_error!(format!("{error}"), "function", "update_schema_order()");

            api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({ "response": "failed to update field order" }),
            )
        }
    }
}

pub async fn update_schema_field(
    claims: Claims,
    Path(uid): Path<String>,
    Json(request): Json<UpdateFieldRequest>,
) -> Response {
    if !claims.can_manage_accounts() {
        return access_denied();
    }

    let label = request.label.as_deref().map(str::trim).map(str::to_string);

    if label.as_deref().is_some_and(str::is_empty) {
        return api_json(
            StatusCode::BAD_REQUEST,
            json!({ "response": "Field label cannot be empty." }),
        );
    }

    let uid_for_db = uid.clone();

    let database_result =
        tokio::task::spawn_blocking(move || -> Result<FieldDefinition, SqliteDatabaseError> {
            with_sql_connection(|connection| {
                ensure_dynamic_schema(connection)?;

                let existing = connection.query_row(
                    r#"
                    SELECT
                        uid,
                        field_key,
                        label,
                        field_type,
                        required,
                        unique_value,
                        searchable,
                        sortable,
                        table_visible,
                        table_priority,
                        position,
                        active,
                        config_json
                    FROM aris_fields
                    WHERE uid = ?1
                    "#,
                    params![&uid_for_db],
                    row_to_field,
                )?;

                if request.active == Some(false) {
                    ensure_storage_layout_schema(connection)?;

                    if field_used_by_storage_layout_db(connection, &uid_for_db)? {
                        return Err(rusqlite::Error::InvalidParameterName(
                            "ARIS_STORAGE_LAYOUT_FIELD".to_string(),
                        ));
                    }
                }

                let next_unique = request.unique_value.unwrap_or(existing.unique_value)
                    || existing.field_type == "auto_number";

                if next_unique
                    && !existing.unique_value
                    && existing.field_type != "auto_number"
                    && duplicate_value_exists(connection, &uid_for_db)?
                {
                    return Err(rusqlite::Error::InvalidParameterName(
                        "ARIS_DUPLICATE_VALUES".to_string(),
                    ));
                }

                let next_position = if request.active == Some(true)
                    && !existing.active
                    && request.position.is_none()
                {
                    connection.query_row(
                        r#"
                            SELECT COALESCE(MAX(position), -10) + 10
                            FROM (
                                SELECT position
                                FROM aris_fields
                                WHERE active = 1

                                UNION ALL

                                SELECT position
                                FROM aris_system_fields
                            )
                            "#,
                        [],
                        |row| row.get::<_, i64>(0),
                    )?
                } else {
                    request.position.unwrap_or(existing.position).max(0)
                };

                let next_config = match request.config {
                    Some(config) => {
                        validate_config(&existing.field_type, &config).map_err(|message| {
                            rusqlite::Error::InvalidParameterName(format!("ARIS_CONFIG:{message}"))
                        })?
                    }
                    None => existing.config.clone(),
                };

                let next_sortable = request.sortable.unwrap_or(existing.sortable)
                    && existing.field_type != "long_text";

                connection.execute(
                    r#"
                    UPDATE aris_fields
                    SET
                        label = ?2,
                        required = ?3,
                        unique_value = ?4,
                        searchable = ?5,
                        sortable = ?6,
                        table_visible = ?7,
                        table_priority = ?8,
                        position = ?9,
                        active = ?10,
                        config_json = ?11
                    WHERE uid = ?1
                    "#,
                    params![
                        &uid_for_db,
                        label.unwrap_or(existing.label),
                        if request.required.unwrap_or(existing.required)
                            || existing.field_type == "auto_number"
                        {
                            1_i64
                        } else {
                            0_i64
                        },
                        if next_unique { 1_i64 } else { 0_i64 },
                        if request.searchable.unwrap_or(existing.searchable) {
                            1_i64
                        } else {
                            0_i64
                        },
                        if next_sortable { 1_i64 } else { 0_i64 },
                        if request.table_visible.unwrap_or(existing.table_visible) {
                            1_i64
                        } else {
                            0_i64
                        },
                        request.table_priority.unwrap_or(existing.table_priority),
                        next_position,
                        if request.active.unwrap_or(existing.active) {
                            1_i64
                        } else {
                            0_i64
                        },
                        next_config.to_string(),
                    ],
                )?;

                bump_schema_revision(connection)?;

                let updated = connection.query_row(
                    r#"
                    SELECT
                        uid,
                        field_key,
                        label,
                        field_type,
                        required,
                        unique_value,
                        searchable,
                        sortable,
                        table_visible,
                        table_priority,
                        position,
                        active,
                        config_json
                    FROM aris_fields
                    WHERE uid = ?1
                    "#,
                    params![&uid_for_db],
                    row_to_field,
                )?;

                rebuild_unique_values_for_field(connection, &updated)?;

                Ok(updated)
            })
        })
        .await;

    match database_result {
        Ok(Ok(field)) => api_json(StatusCode::OK, json!(field)),

        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::QueryReturnedNoRows))) => api_json(
            StatusCode::NOT_FOUND,
            json!({ "response": "ARIS field was not found." }),
        ),

        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::InvalidParameterName(message))))
            if message == "ARIS_DUPLICATE_VALUES" =>
        {
            api_json(
                StatusCode::CONFLICT,
                json!({
                    "response": "Unique cannot be enabled because existing records contain duplicate values for this field."
                }),
            )
        }

        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::InvalidParameterName(message))))
            if message.starts_with("ARIS_CONFIG:") =>
        {
            api_json(
                StatusCode::BAD_REQUEST,
                json!({
                    "response": message.trim_start_matches("ARIS_CONFIG:")
                }),
            )
        }

        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::InvalidParameterName(message))))
            if message == "ARIS_STORAGE_LAYOUT_FIELD" =>
        {
            api_json(
                StatusCode::CONFLICT,
                json!({
                    "response": "This field is used by the N1 storage layout. Remove it from the storage layout before archiving it."
                }),
            )
        }

        Ok(Err(error)) => {
            crate::report_error!(format!("{error}"), "function", "update_schema_field()");

            api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({ "response": "failed to update ARIS field" }),
            )
        }

        Err(error) => {
            crate::report_error!(format!("{error}"), "function", "update_schema_field()");

            api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({ "response": "failed to update ARIS field" }),
            )
        }
    }
}
