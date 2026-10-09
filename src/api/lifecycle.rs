use std::collections::BTreeMap;

use axum::{
    Json,
    extract::Path,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use rusqlite::params;
use serde::Serialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    api::{
        live::publish_live_event,
        modules::{DEFAULT_MODULE_UID, module_can_for_user},
        mx::{records::ensure_record_collaboration_schema, schema::ensure_dynamic_schema},
        notifications::notify_module_readers,
    },
    db::connector::{SqliteDatabaseError, with_sql_connection},
    middleware::auth::Claims,
};

#[derive(Debug, Serialize)]
struct RecordVersionSummary {
    uid: String,
    version: i64,
    event: String,
    actor_uid: String,
    actor_name: String,
    created_at: i64,
}

#[derive(Debug, Serialize)]
struct RecordVersionDetail {
    uid: String,
    record_uid: String,
    module_uid: String,
    version: i64,
    event: String,
    actor_uid: String,
    actor_name: String,
    created_at: i64,
    values: BTreeMap<String, Value>,
    attachments: Vec<Value>,
}

#[derive(Debug, Clone, Serialize)]
struct TrashFieldValue {
    uid: String,
    key: String,
    label: String,
    field_type: String,
    value: Value,
    display_value: String,
    active: bool,

    #[serde(skip)]
    table_visible: bool,

    #[serde(skip)]
    table_priority: i64,

    #[serde(skip)]
    position: i64,
}

#[derive(Debug, Clone, Serialize)]
struct TrashSummaryField {
    label: String,
    value: String,
}

#[derive(Debug, Serialize)]
struct TrashAttachment {
    uid: String,
    file_name: String,
    mime_type: String,
    size: u64,
    field_label: String,
}

#[derive(Debug, Serialize)]
struct TrashRecordListItem {
    uid: String,
    module_uid: String,
    module_name: String,
    singular_name: String,
    deleted_at: i64,
    deleted_by_name: String,
    attachment_count: u64,
    summary_fields: Vec<TrashSummaryField>,
}

#[derive(Debug, Serialize)]
struct TrashRecordDetail {
    uid: String,
    module_uid: String,
    module_name: String,
    singular_name: String,
    deleted_at: i64,
    deleted_by_name: String,
    attachment_count: u64,
    version_count: u64,
    summary_fields: Vec<TrashSummaryField>,
    fields: Vec<TrashFieldValue>,
    attachments: Vec<TrashAttachment>,
}

fn has_column(
    connection: &rusqlite::Connection,
    table: &str,
    column: &str,
) -> rusqlite::Result<bool> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info(?1) WHERE name = ?2)",
            params![table, column],
            |row| row.get::<_, i64>(0),
        )
        .map(|value| value != 0)
}

pub(crate) fn ensure_record_lifecycle_schema(
    connection: &rusqlite::Connection,
) -> rusqlite::Result<()> {
    ensure_dynamic_schema(connection)?;
    if !has_column(connection, "mx_records", "deleted_at")? {
        connection.execute("ALTER TABLE mx_records ADD COLUMN deleted_at INTEGER", [])?;
    }
    if !has_column(connection, "mx_records", "deleted_by")? {
        connection.execute("ALTER TABLE mx_records ADD COLUMN deleted_by TEXT", [])?;
    }
    connection.execute_batch(
        r#"
        CREATE INDEX IF NOT EXISTS idx_mx_records_deleted ON mx_records(deleted_at, module_uid);

        CREATE TABLE IF NOT EXISTS mx_record_versions (
            uid         TEXT PRIMARY KEY NOT NULL,
            record_uid  TEXT NOT NULL,
            module_uid  TEXT NOT NULL,
            version_no  INTEGER NOT NULL,
            event       TEXT NOT NULL,
            actor_uid   TEXT NOT NULL,
            created_at  INTEGER NOT NULL,
            UNIQUE(record_uid, version_no)
        );

        CREATE TABLE IF NOT EXISTS mx_record_version_values (
            version_uid    TEXT NOT NULL,
            field_uid      TEXT NOT NULL,
            value_text     TEXT,
            value_integer  INTEGER,
            value_real     REAL,
            value_boolean  INTEGER,
            PRIMARY KEY(version_uid, field_uid),
            FOREIGN KEY(version_uid) REFERENCES mx_record_versions(uid) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS mx_record_version_attachments (
            version_uid          TEXT NOT NULL,
            attachment_uid       TEXT NOT NULL,
            file_name            TEXT NOT NULL,
            mime_type            TEXT NOT NULL,
            size                 INTEGER NOT NULL,
            object_key           TEXT NOT NULL,
            object_version_id    TEXT,
            attachment_field_uid TEXT,
            PRIMARY KEY(version_uid, attachment_uid),
            FOREIGN KEY(version_uid) REFERENCES mx_record_versions(uid) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_mx_record_versions_record
        ON mx_record_versions(record_uid, version_no DESC);
        "#,
    )
}

pub(crate) fn capture_record_version(
    transaction: &rusqlite::Transaction<'_>,
    record_uid: &str,
    module_uid: &str,
    event: &str,
    actor_uid: &str,
) -> rusqlite::Result<String> {
    let version_uid = Uuid::new_v4().to_string();
    let version_no = transaction.query_row(
        "SELECT COALESCE(MAX(version_no), 0) + 1 FROM mx_record_versions WHERE record_uid = ?1",
        params![record_uid],
        |row| row.get::<_, i64>(0),
    )?;
    transaction.execute(
        "INSERT INTO mx_record_versions(uid,record_uid,module_uid,version_no,event,actor_uid,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7)",
        params![version_uid, record_uid, module_uid, version_no, event, actor_uid, chrono::Utc::now().timestamp_millis()],
    )?;
    transaction.execute(
        r#"INSERT INTO mx_record_version_values(version_uid,field_uid,value_text,value_integer,value_real,value_boolean)
           SELECT ?1,field_uid,value_text,value_integer,value_real,value_boolean FROM mx_record_values WHERE record_uid = ?2"#,
        params![version_uid, record_uid],
    )?;
    transaction.execute(
        r#"INSERT INTO mx_record_version_attachments(version_uid,attachment_uid,file_name,mime_type,size,object_key,object_version_id,attachment_field_uid)
           SELECT ?1,uid,file_name,mime_type,size,object_key,version_id,attachment_field_uid FROM mx_attachments WHERE entry_uid = ?2"#,
        params![version_uid, record_uid],
    )?;
    Ok(version_uid)
}

// Deletion/restoration are revision boundaries too. A draft opened before
// Trash must not silently overwrite the restored record, even for a formerly
// empty field that has no existing field-revision metadata.
fn advance_lifecycle_revision(
    transaction: &rusqlite::Transaction<'_>,
    uid: &str,
    module_uid: &str,
    actor_uid: &str,
) -> rusqlite::Result<i64> {
    let revision = transaction.query_row(
        "UPDATE mx_records SET revision=revision+1 WHERE uid=?1 AND module_uid=?2 RETURNING revision",
        params![uid, module_uid], |row| row.get::<_, i64>(0),
    )?;
    transaction.execute(
        "INSERT INTO mx_record_field_revisions(record_uid,field_uid,revision,updated_at,updated_by)
         SELECT ?1,uid,?3,?4,?5 FROM mx_fields WHERE module_uid=?2
         ON CONFLICT(record_uid,field_uid) DO UPDATE SET
         revision=excluded.revision,updated_at=excluded.updated_at,updated_by=excluded.updated_by",
        params![
            uid,
            module_uid,
            revision,
            chrono::Utc::now().timestamp_millis(),
            actor_uid
        ],
    )?;
    Ok(revision)
}

pub(crate) async fn capture_record_lifecycle_event(
    record_uid: String,
    actor_uid: String,
    event: &'static str,
) {
    let _ = tokio::task::spawn_blocking(move || {
        with_sql_connection(|connection| {
            ensure_record_lifecycle_schema(connection)?;
            let transaction = connection.unchecked_transaction()?;
            let module_uid = transaction.query_row(
                "SELECT module_uid FROM mx_records WHERE uid=?1",
                params![record_uid],
                |row| row.get::<_, String>(0),
            )?;
            capture_record_version(&transaction, &record_uid, &module_uid, event, &actor_uid)?;
            transaction.commit()
        })
    })
    .await;
}

fn api_json(status: StatusCode, body: Value) -> Response {
    (status, Json(body)).into_response()
}

fn can(
    connection: &rusqlite::Connection,
    module_uid: &str,
    claims: &Claims,
    capability: &str,
) -> bool {
    module_can_for_user(
        connection,
        module_uid,
        &claims.uid,
        claims.access_level,
        capability,
    )
    .unwrap_or(false)
}

async fn delete_for_module(claims: Claims, module_uid: String, uid: String) -> Response {
    let actor_uid = claims.uid.clone();
    let permission_claims = claims.clone();
    let event_uid = uid.clone();
    let event_module = module_uid.clone();
    let result = tokio::task::spawn_blocking(move || with_sql_connection(|connection| {
        ensure_record_lifecycle_schema(connection)?;
        ensure_record_collaboration_schema(connection)?;
        if !can(connection, &module_uid, &permission_claims, "delete") {
            return Err(rusqlite::Error::InvalidParameterName("forbidden".into()));
        }
        let transaction = rusqlite::Transaction::new_unchecked(connection, rusqlite::TransactionBehavior::Immediate)?;
        let exists = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM mx_records WHERE uid=?1 AND module_uid=?2 AND deleted_at IS NULL)",
            params![uid, module_uid], |row| row.get::<_, i64>(0),
        )? != 0;
        if !exists { return Err(rusqlite::Error::QueryReturnedNoRows); }
        transaction.execute(
            "UPDATE mx_records SET deleted_at=?2, deleted_by=?3 WHERE uid=?1 AND deleted_at IS NULL",
            params![uid, chrono::Utc::now().timestamp_millis(), actor_uid],
        )?;
        advance_lifecycle_revision(&transaction, &uid, &module_uid, &actor_uid)?;
        capture_record_version(&transaction, &uid, &module_uid, "deleted", &actor_uid)?;
        transaction.commit()
    })).await;
    match result {
        Ok(Ok(())) => {
            publish_live_event(
                "record.deleted",
                Some(&claims.uid),
                json!({"record_uid":event_uid,"module_uid":event_module,"recoverable":true}),
            );
            tokio::spawn(notify_module_readers(
                event_module.clone(),
                claims.uid.clone(),
                "record.deleted".to_string(),
                "Record moved to trash".to_string(),
                "A record in a module you can access was moved to Administrator Trash.".to_string(),
                event_uid.clone(),
                json!({"record_uid":event_uid,"module_uid":event_module,"recoverable":true}),
            ));
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::InvalidParameterName(_)))) => api_json(
            StatusCode::FORBIDDEN,
            json!({"response":"you cannot delete records in this module"}),
        ),
        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::QueryReturnedNoRows))) => api_json(
            StatusCode::NOT_FOUND,
            json!({"response":"record was not found"}),
        ),
        _ => api_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"response":"record could not be moved to trash"}),
        ),
    }
}

pub async fn delete_default_record(claims: Claims, Path(uid): Path<String>) -> Response {
    delete_for_module(claims, DEFAULT_MODULE_UID.to_string(), uid).await
}

pub async fn delete_module_record(
    claims: Claims,
    Path((module_uid, uid)): Path<(String, String)>,
) -> Response {
    delete_for_module(claims, module_uid, uid).await
}

async fn versions_for_module(claims: Claims, module_uid: String, record_uid: String) -> Response {
    let permission_claims = claims;
    let result = tokio::task::spawn_blocking(move || with_sql_connection(|connection| {
        ensure_record_lifecycle_schema(connection)?;
        if !can(connection, &module_uid, &permission_claims, "read") { return Err(rusqlite::Error::InvalidParameterName("forbidden".into())); }
        let mut statement = connection.prepare(
            r#"SELECT v.uid,v.version_no,v.event,v.actor_uid,COALESCE(a.name,v.actor_uid),v.created_at
               FROM mx_record_versions v LEFT JOIN users a ON a.uid=v.actor_uid
               WHERE v.record_uid=?1 AND v.module_uid=?2 ORDER BY v.version_no DESC"#,
        )?;
        statement.query_map(params![record_uid,module_uid], |row| Ok(RecordVersionSummary { uid:row.get(0)?, version:row.get(1)?, event:row.get(2)?, actor_uid:row.get(3)?, actor_name:row.get(4)?, created_at:row.get(5)? }))?.collect::<Result<Vec<_>,_>>()
    })).await;
    match result {
        Ok(Ok(versions)) => api_json(StatusCode::OK, json!({"versions":versions})),
        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::InvalidParameterName(_)))) => api_json(
            StatusCode::FORBIDDEN,
            json!({"response":"module access denied"}),
        ),
        _ => api_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"response":"record history could not be loaded"}),
        ),
    }
}

async fn version_detail_for_module(
    claims: Claims,
    module_uid: String,
    record_uid: String,
    version_uid: String,
) -> Response {
    let permission_claims = claims;
    let result = tokio::task::spawn_blocking(move || {
        with_sql_connection(|connection| {
            ensure_record_lifecycle_schema(connection)?;
            if !can(connection, &module_uid, &permission_claims, "read") {
                return Err(rusqlite::Error::InvalidParameterName("forbidden".into()));
            }
            let (uid, version, event, actor_uid, actor_name, created_at) = connection.query_row(
                r#"SELECT v.uid,v.version_no,v.event,v.actor_uid,COALESCE(a.name,v.actor_uid),v.created_at
                   FROM mx_record_versions v LEFT JOIN users a ON a.uid=v.actor_uid
                   WHERE v.uid=?1 AND v.record_uid=?2 AND v.module_uid=?3"#,
                params![version_uid, record_uid, module_uid],
                |row| Ok((row.get::<_,String>(0)?,row.get::<_,i64>(1)?,row.get::<_,String>(2)?,row.get::<_,String>(3)?,row.get::<_,String>(4)?,row.get::<_,i64>(5)?)),
            )?;
            let mut values = BTreeMap::new();
            let mut statement = connection.prepare(
                r#"SELECT COALESCE(f.module_key,f.field_key),f.field_type,v.value_text,v.value_integer,v.value_real,v.value_boolean
                   FROM mx_record_version_values v JOIN mx_fields f ON f.uid=v.field_uid WHERE v.version_uid=?1 ORDER BY f.position"#,
            )?;
            let mut rows = statement.query(params![uid])?;
            while let Some(row) = rows.next()? {
                let key: String = row.get(0)?;
                let kind: String = row.get(1)?;
                let value = match kind.as_str() {
                    "integer" | "auto_number" => row.get::<_,Option<i64>>(3)?.map_or(Value::Null, |value| json!(value)),
                    "decimal" | "formula" => row.get::<_,Option<f64>>(4)?.map_or(Value::Null, |value| json!(value)),
                    "boolean" => row.get::<_,Option<i64>>(5)?.map_or(Value::Null, |value| json!(value != 0)),
                    _ => row.get::<_,Option<String>>(2)?.map_or(Value::Null, |value| json!(value)),
                };
                values.insert(key, value);
            }
            let mut statement = connection.prepare("SELECT attachment_uid,file_name,mime_type,size,attachment_field_uid FROM mx_record_version_attachments WHERE version_uid=?1 ORDER BY file_name COLLATE NOCASE")?;
            let attachments = statement.query_map(params![uid], |row| Ok(json!({
                "uid":row.get::<_,String>(0)?, "file_name":row.get::<_,String>(1)?,
                "mime_type":row.get::<_,String>(2)?, "size":row.get::<_,i64>(3)?,
                "attachment_field_uid":row.get::<_,Option<String>>(4)?
            })))?.collect::<Result<Vec<_>,_>>()?;
            Ok(RecordVersionDetail { uid, record_uid, module_uid, version, event, actor_uid, actor_name, created_at, values, attachments })
        })
    }).await;
    match result {
        Ok(Ok(version)) => api_json(StatusCode::OK, json!(version)),
        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::InvalidParameterName(_)))) => api_json(
            StatusCode::FORBIDDEN,
            json!({"response":"module access denied"}),
        ),
        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::QueryReturnedNoRows))) => api_json(
            StatusCode::NOT_FOUND,
            json!({"response":"record version was not found"}),
        ),
        _ => api_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"response":"record version could not be loaded"}),
        ),
    }
}

async fn restore_version_for_module(
    claims: Claims,
    module_uid: String,
    record_uid: String,
    version_uid: String,
) -> Response {
    let actor_uid = claims.uid.clone();
    let permission_claims = claims.clone();
    let event_record = record_uid.clone();
    let event_module = module_uid.clone();
    let result = tokio::task::spawn_blocking(move || with_sql_connection(|connection| {
        ensure_record_lifecycle_schema(connection)?;
        if !can(connection, &module_uid, &permission_claims, "update") {
            return Err(rusqlite::Error::InvalidParameterName("forbidden".into()));
        }
        let transaction = connection.unchecked_transaction()?;
        let exists = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM mx_record_versions WHERE uid=?1 AND record_uid=?2 AND module_uid=?3)",
            params![version_uid, record_uid, module_uid], |row| row.get::<_,i64>(0),
        )? != 0;
        if !exists { return Err(rusqlite::Error::QueryReturnedNoRows); }
        transaction.execute("DELETE FROM mx_unique_values WHERE record_uid=?1", params![record_uid])?;
        transaction.execute("DELETE FROM mx_record_values WHERE record_uid=?1", params![record_uid])?;
        transaction.execute(
            r#"INSERT INTO mx_record_values(record_uid,field_uid,value_text,value_integer,value_real,value_boolean)
               SELECT ?1,field_uid,value_text,value_integer,value_real,value_boolean FROM mx_record_version_values WHERE version_uid=?2"#,
            params![record_uid, version_uid],
        )?;
        transaction.execute(
            r#"INSERT INTO mx_unique_values(field_uid,normalized_value,record_uid)
               SELECT f.uid, LOWER(TRIM(CASE f.field_type WHEN 'integer' THEN CAST(v.value_integer AS TEXT) WHEN 'decimal' THEN CAST(v.value_real AS TEXT) WHEN 'formula' THEN CAST(v.value_real AS TEXT) WHEN 'boolean' THEN CAST(v.value_boolean AS TEXT) ELSE COALESCE(v.value_text,'') END)), ?1
               FROM mx_record_version_values v JOIN mx_fields f ON f.uid=v.field_uid
               WHERE v.version_uid=?2 AND f.unique_value=1 AND f.field_type!='auto_number'
                 AND TRIM(CASE f.field_type WHEN 'integer' THEN CAST(v.value_integer AS TEXT) WHEN 'decimal' THEN CAST(v.value_real AS TEXT) WHEN 'formula' THEN CAST(v.value_real AS TEXT) WHEN 'boolean' THEN CAST(v.value_boolean AS TEXT) ELSE COALESCE(v.value_text,'') END)!=''"#,
            params![record_uid, version_uid],
        )?;
        let now = chrono::Utc::now().timestamp_millis();
        let next_revision: i64 = transaction.query_row(
            "UPDATE mx_records SET revision=revision+1 WHERE uid=?1 RETURNING revision",
            params![record_uid],
            |row| row.get(0),
        )?;
        transaction.execute("UPDATE mx_record_field_revisions SET revision=?2,updated_at=?3,updated_by=?4 WHERE record_uid=?1", params![record_uid,next_revision,now,actor_uid])?;
        transaction.execute(
            r#"INSERT OR IGNORE INTO mx_record_field_revisions(record_uid,field_uid,revision,updated_at,updated_by)
               SELECT ?1,field_uid,?3,?4,?5 FROM mx_record_version_values WHERE version_uid=?2"#,
            params![record_uid,version_uid,next_revision,now,actor_uid],
        )?;
        capture_record_version(&transaction,&record_uid,&module_uid,"version_restored",&actor_uid)?;
        transaction.commit()
    })).await;
    match result {
        Ok(Ok(())) => {
            publish_live_event(
                "record.updated",
                Some(&claims.uid),
                json!({"record_uid":event_record,"module_uid":event_module,"version_restored":true}),
            );
            api_json(
                StatusCode::OK,
                json!({"response":"record version restored"}),
            )
        }
        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::InvalidParameterName(_)))) => api_json(
            StatusCode::FORBIDDEN,
            json!({"response":"you cannot restore versions in this module"}),
        ),
        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::QueryReturnedNoRows))) => api_json(
            StatusCode::NOT_FOUND,
            json!({"response":"record version was not found"}),
        ),
        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::SqliteFailure(error, _))))
            if error.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            api_json(
                StatusCode::CONFLICT,
                json!({"response":"this version conflicts with a unique value now used by another record"}),
            )
        }
        _ => api_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"response":"record version could not be restored"}),
        ),
    }
}

pub async fn list_default_record_versions(claims: Claims, Path(uid): Path<String>) -> Response {
    versions_for_module(claims, DEFAULT_MODULE_UID.to_string(), uid).await
}

pub async fn list_module_record_versions(
    claims: Claims,
    Path((module_uid, uid)): Path<(String, String)>,
) -> Response {
    versions_for_module(claims, module_uid, uid).await
}

pub async fn get_default_record_version(
    claims: Claims,
    Path((uid, version_uid)): Path<(String, String)>,
) -> Response {
    version_detail_for_module(claims, DEFAULT_MODULE_UID.to_string(), uid, version_uid).await
}

pub async fn get_module_record_version(
    claims: Claims,
    Path((module_uid, uid, version_uid)): Path<(String, String, String)>,
) -> Response {
    version_detail_for_module(claims, module_uid, uid, version_uid).await
}

pub async fn restore_default_record_version(
    claims: Claims,
    Path((uid, version_uid)): Path<(String, String)>,
) -> Response {
    restore_version_for_module(claims, DEFAULT_MODULE_UID.to_string(), uid, version_uid).await
}

pub async fn restore_module_record_version(
    claims: Claims,
    Path((module_uid, uid, version_uid)): Path<(String, String, String)>,
) -> Response {
    restore_version_for_module(claims, module_uid, uid, version_uid).await
}

fn trash_field_display_value(field_type: &str, config_text: &str, value: &Value) -> String {
    if value.is_null() {
        return String::new();
    }

    match field_type {
        "boolean" => value
            .as_bool()
            .map(|enabled| if enabled { "Yes" } else { "No" }.to_string())
            .unwrap_or_default(),
        "auto_number" => value
            .as_i64()
            .map(|number| {
                let config: Value = serde_json::from_str(config_text).unwrap_or_else(|_| json!({}));
                let prefix = config.get("prefix").and_then(Value::as_str).unwrap_or("");
                let padding = config
                    .get("padding")
                    .and_then(Value::as_u64)
                    .unwrap_or(0)
                    .min(12) as usize;
                if padding == 0 {
                    format!("{prefix}{number}")
                } else {
                    format!("{prefix}{number:0padding$}")
                }
            })
            .unwrap_or_default(),
        _ => value
            .as_str()
            .map(str::trim)
            .map(str::to_string)
            .unwrap_or_else(|| value.to_string()),
    }
}

fn load_trash_fields(
    connection: &rusqlite::Connection,
    record_uid: &str,
    module_uid: &str,
) -> rusqlite::Result<Vec<TrashFieldValue>> {
    let mut statement = connection.prepare(
        r#"
        SELECT
            f.uid,
            COALESCE(f.module_key, f.field_key),
            f.label,
            f.field_type,
            f.config_json,
            f.active,
            f.table_visible,
            f.table_priority,
            f.position,
            rv.value_text,
            rv.value_integer,
            rv.value_real,
            rv.value_boolean
        FROM mx_fields f
        LEFT JOIN mx_record_values rv
          ON rv.field_uid = f.uid
         AND rv.record_uid = ?1
        WHERE f.module_uid = ?2
          AND f.field_type != 'attachments'
        ORDER BY f.position ASC, f.label COLLATE NOCASE ASC
        "#,
    )?;

    statement
        .query_map(params![record_uid, module_uid], |row| {
            let field_type: String = row.get(3)?;
            let config_text: String = row.get(4)?;
            let value = match field_type.as_str() {
                "integer" | "auto_number" => row
                    .get::<_, Option<i64>>(10)?
                    .map_or(Value::Null, |value| json!(value)),
                "decimal" | "formula" => row
                    .get::<_, Option<f64>>(11)?
                    .map_or(Value::Null, |value| json!(value)),
                "boolean" => row
                    .get::<_, Option<i64>>(12)?
                    .map_or(Value::Null, |value| json!(value != 0)),
                _ => row
                    .get::<_, Option<String>>(9)?
                    .map_or(Value::Null, |value| json!(value)),
            };
            let display_value = trash_field_display_value(&field_type, &config_text, &value);

            Ok(TrashFieldValue {
                uid: row.get(0)?,
                key: row.get(1)?,
                label: row.get(2)?,
                field_type,
                value,
                display_value,
                active: row.get::<_, i64>(5)? != 0,
                table_visible: row.get::<_, i64>(6)? != 0,
                table_priority: row.get(7)?,
                position: row.get(8)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()
}

fn truncate_summary_value(value: &str) -> String {
    const MAX_CHARACTERS: usize = 120;
    let mut characters = value.chars();
    let beginning = characters.by_ref().take(MAX_CHARACTERS).collect::<String>();
    if characters.next().is_some() {
        format!("{beginning}…")
    } else {
        beginning
    }
}

fn trash_summary_fields(fields: &[TrashFieldValue]) -> Vec<TrashSummaryField> {
    let mut candidates = fields
        .iter()
        .filter(|field| !field.display_value.trim().is_empty())
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        right
            .active
            .cmp(&left.active)
            .then_with(|| right.table_visible.cmp(&left.table_visible))
            .then_with(|| right.table_priority.cmp(&left.table_priority))
            .then_with(|| left.position.cmp(&right.position))
    });
    candidates
        .into_iter()
        .take(3)
        .map(|field| TrashSummaryField {
            label: field.label.clone(),
            value: truncate_summary_value(&field.display_value),
        })
        .collect()
}

fn load_trash_attachments(
    connection: &rusqlite::Connection,
    record_uid: &str,
) -> rusqlite::Result<Vec<TrashAttachment>> {
    let mut statement = connection.prepare(
        r#"
        SELECT
            attachment.uid,
            attachment.file_name,
            attachment.mime_type,
            attachment.size,
            COALESCE(field.label, attachment.attachment_field_label, 'Attachments')
        FROM mx_attachments attachment
        LEFT JOIN mx_fields field ON field.uid = attachment.attachment_field_uid
        WHERE attachment.entry_uid = ?1
        ORDER BY attachment.file_name COLLATE NOCASE ASC
        "#,
    )?;
    statement
        .query_map(params![record_uid], |row| {
            Ok(TrashAttachment {
                uid: row.get(0)?,
                file_name: row.get(1)?,
                mime_type: row.get(2)?,
                size: row.get::<_, i64>(3)?.max(0) as u64,
                field_label: row.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()
}

fn load_trash_record_detail(
    connection: &rusqlite::Connection,
    uid: &str,
) -> rusqlite::Result<TrashRecordDetail> {
    let (record_uid, module_uid, module_name, singular_name, deleted_at, deleted_by_name) =
        connection.query_row(
            r#"
        SELECT
            record.uid,
            record.module_uid,
            COALESCE(module.name, 'Records'),
            COALESCE(module.singular_name, 'Record'),
            record.deleted_at,
            COALESCE(account.name, record.deleted_by, 'Unknown')
        FROM mx_records record
        LEFT JOIN mx_modules module ON module.uid = record.module_uid
        LEFT JOIN users account ON account.uid = record.deleted_by
        WHERE record.uid = ?1
          AND record.deleted_at IS NOT NULL
        "#,
            params![uid],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        )?;
    let fields = load_trash_fields(connection, &record_uid, &module_uid)?;
    let summary_fields = trash_summary_fields(&fields);
    let attachments = load_trash_attachments(connection, &record_uid)?;
    let attachment_count = attachments.len() as u64;
    let version_count = connection.query_row(
        "SELECT COUNT(*) FROM mx_record_versions WHERE record_uid = ?1",
        params![record_uid],
        |row| row.get::<_, i64>(0),
    )?;

    Ok(TrashRecordDetail {
        uid: record_uid,
        module_uid,
        module_name,
        singular_name,
        deleted_at,
        deleted_by_name,
        attachment_count,
        version_count: version_count.max(0) as u64,
        summary_fields,
        fields,
        attachments,
    })
}

pub async fn list_trash(claims: Claims) -> Response {
    if !claims.can_manage_accounts() {
        return api_json(
            StatusCode::FORBIDDEN,
            json!({"response":"administrator access is required"}),
        );
    }
    let result = tokio::task::spawn_blocking(move || {
        with_sql_connection(|connection| {
            ensure_record_lifecycle_schema(connection)?;
            let rows = {
                let mut statement = connection.prepare(
                    r#"SELECT r.uid,r.module_uid,COALESCE(m.name,'Records'),COALESCE(m.singular_name,'Record'),r.deleted_at,COALESCE(a.name,r.deleted_by,'Unknown'),
                       (SELECT COUNT(*) FROM mx_attachments f WHERE f.entry_uid=r.uid)
                       FROM mx_records r LEFT JOIN mx_modules m ON m.uid=r.module_uid LEFT JOIN users a ON a.uid=r.deleted_by
                       WHERE r.deleted_at IS NOT NULL ORDER BY r.deleted_at DESC LIMIT 500"#,
                )?;
                statement
                    .query_map([], |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, i64>(4)?,
                            row.get::<_, String>(5)?,
                            row.get::<_, i64>(6)?,
                        ))
                    })?
                    .collect::<Result<Vec<_>, _>>()?
            };

            rows.into_iter()
                .map(
                    |(
                        uid,
                        module_uid,
                        module_name,
                        singular_name,
                        deleted_at,
                        deleted_by_name,
                        attachment_count,
                    )| {
                        let fields = load_trash_fields(connection, &uid, &module_uid)?;
                        Ok(TrashRecordListItem {
                            uid,
                            module_uid,
                            module_name,
                            singular_name,
                            deleted_at,
                            deleted_by_name,
                            attachment_count: attachment_count.max(0) as u64,
                            summary_fields: trash_summary_fields(&fields),
                        })
                    },
                )
                .collect::<Result<Vec<_>, rusqlite::Error>>()
        })
    })
    .await;
    match result {
        Ok(Ok(records)) => api_json(StatusCode::OK, json!({"records":records})),
        _ => api_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"response":"trash could not be loaded"}),
        ),
    }
}

pub async fn get_trashed_record(claims: Claims, Path(uid): Path<String>) -> Response {
    if !claims.can_manage_accounts() {
        return api_json(
            StatusCode::FORBIDDEN,
            json!({"response":"administrator access is required"}),
        );
    }
    let result = tokio::task::spawn_blocking(move || {
        with_sql_connection(|connection| {
            ensure_record_lifecycle_schema(connection)?;
            load_trash_record_detail(connection, &uid)
        })
    })
    .await;
    match result {
        Ok(Ok(record)) => api_json(StatusCode::OK, json!({"record":record})),
        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::QueryReturnedNoRows))) => api_json(
            StatusCode::NOT_FOUND,
            json!({"response":"trashed record was not found"}),
        ),
        _ => api_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"response":"trashed record could not be loaded"}),
        ),
    }
}

pub async fn restore_trashed_record(claims: Claims, Path(uid): Path<String>) -> Response {
    if !claims.can_manage_accounts() {
        return api_json(
            StatusCode::FORBIDDEN,
            json!({"response":"administrator access is required"}),
        );
    }
    let actor_uid = claims.uid.clone();
    let event_uid = uid.clone();
    let result = tokio::task::spawn_blocking(move || {
        with_sql_connection(|connection| {
            ensure_record_lifecycle_schema(connection)?;
            ensure_record_collaboration_schema(connection)?;
            let transaction = rusqlite::Transaction::new_unchecked(
                connection,
                rusqlite::TransactionBehavior::Immediate,
            )?;
            let module_uid = transaction.query_row(
                "SELECT module_uid FROM mx_records WHERE uid=?1 AND deleted_at IS NOT NULL",
                params![uid],
                |row| row.get::<_, String>(0),
            )?;
            transaction.execute(
                "UPDATE mx_records SET deleted_at=NULL,deleted_by=NULL WHERE uid=?1",
                params![uid],
            )?;
            advance_lifecycle_revision(&transaction, &uid, &module_uid, &actor_uid)?;
            capture_record_version(&transaction, &uid, &module_uid, "restored", &actor_uid)?;
            transaction.commit()?;
            Ok(module_uid)
        })
    })
    .await;
    match result {
        Ok(Ok(module_uid)) => {
            publish_live_event(
                "record.restored",
                Some(&claims.uid),
                json!({"record_uid":event_uid,"module_uid":module_uid}),
            );
            tokio::spawn(notify_module_readers(
                module_uid.clone(),
                claims.uid.clone(),
                "record.restored".to_string(),
                "Record restored".to_string(),
                "An administrator restored a record from Trash.".to_string(),
                event_uid.clone(),
                json!({"record_uid":event_uid,"module_uid":module_uid}),
            ));
            api_json(
                StatusCode::OK,
                json!({"response":"record restored","uid":event_uid}),
            )
        }
        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::QueryReturnedNoRows))) => api_json(
            StatusCode::NOT_FOUND,
            json!({"response":"trashed record was not found"}),
        ),
        _ => api_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"response":"record could not be restored"}),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_schema_adds_soft_delete_and_versions() {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        ensure_record_lifecycle_schema(&connection).unwrap();
        assert!(has_column(&connection, "mx_records", "deleted_at").unwrap());
        let exists: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='mx_record_versions')",[],|row|row.get(0)).unwrap();
        assert!(exists);
    }

    #[test]
    fn version_snapshot_copies_values_and_attachment_versions() {
        let mut connection = rusqlite::Connection::open_in_memory().unwrap();
        ensure_record_lifecycle_schema(&connection).unwrap();
        connection.execute("INSERT INTO mx_records(uid,control_year,control_no,date,office,requestor,subject,routed_to_div,remarks,module_uid) VALUES ('record-1',0,1,'','','','','','',?1)",params![DEFAULT_MODULE_UID]).unwrap();
        connection.execute("INSERT INTO mx_attachments(uid,entry_uid,file_name,mime_type,size,object_key,version_id,created_at) VALUES ('attachment-1','record-1','test.pdf','application/pdf',4,'records/test.pdf','n1-version',1)",[]).unwrap();
        let transaction = connection.transaction().unwrap();
        capture_record_version(
            &transaction,
            "record-1",
            DEFAULT_MODULE_UID,
            "created",
            "actor-1",
        )
        .unwrap();
        transaction.commit().unwrap();
        let captured: (String, Option<String>) = connection
            .query_row(
                "SELECT file_name,object_version_id FROM mx_record_version_attachments",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            captured,
            ("test.pdf".to_string(), Some("n1-version".to_string()))
        );
    }

    #[test]
    fn trash_and_restore_invalidate_all_old_drafts_including_empty_fields() {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        ensure_record_lifecycle_schema(&connection).unwrap();
        ensure_record_collaboration_schema(&connection).unwrap();
        connection.execute("INSERT INTO mx_records(uid,control_year,control_no,date,office,requestor,subject,routed_to_div,remarks,module_uid) VALUES ('boundary-record',0,1,'','','','','','',?1)", params![DEFAULT_MODULE_UID]).unwrap();
        connection.execute("INSERT INTO mx_fields(uid,field_key,label,field_type,module_uid,module_key) VALUES ('empty-field','empty','Empty','text',?1,'empty')", params![DEFAULT_MODULE_UID]).unwrap();
        let transaction = connection.unchecked_transaction().unwrap();
        assert_eq!(
            advance_lifecycle_revision(
                &transaction,
                "boundary-record",
                DEFAULT_MODULE_UID,
                "deleting-user"
            )
            .unwrap(),
            2
        );
        assert_eq!(
            advance_lifecycle_revision(
                &transaction,
                "boundary-record",
                DEFAULT_MODULE_UID,
                "restoring-admin"
            )
            .unwrap(),
            3
        );
        transaction.commit().unwrap();
        let field: (i64, String) = connection.query_row("SELECT revision,updated_by FROM mx_record_field_revisions WHERE record_uid='boundary-record' AND field_uid='empty-field'", [], |row| Ok((row.get(0)?, row.get(1)?))).unwrap();
        assert_eq!(field, (3, "restoring-admin".into()));
        assert!(
            field.0 > 1,
            "A revision-1 draft must conflict after restoration"
        );
        // The revision boundary is atomic with the lifecycle transaction.
        {
            let transaction = connection.unchecked_transaction().unwrap();
            advance_lifecycle_revision(
                &transaction,
                "boundary-record",
                DEFAULT_MODULE_UID,
                "rolled-back",
            )
            .unwrap();
        }
        assert_eq!(
            connection
                .query_row(
                    "SELECT revision FROM mx_records WHERE uid='boundary-record'",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            3
        );
    }

    #[test]
    fn trash_summary_uses_configured_record_fields_instead_of_the_uid() {
        let fields = vec![
            TrashFieldValue {
                uid: "subject-field".into(),
                key: "subject".into(),
                label: "Subject".into(),
                field_type: "long_text".into(),
                value: json!("Building maintenance request"),
                display_value: "Building maintenance request".into(),
                active: true,
                table_visible: true,
                table_priority: 95,
                position: 4,
            },
            TrashFieldValue {
                uid: "office-field".into(),
                key: "office".into(),
                label: "Office".into(),
                field_type: "text".into(),
                value: json!("MCH"),
                display_value: "MCH".into(),
                active: true,
                table_visible: true,
                table_priority: 70,
                position: 2,
            },
        ];

        let summary = trash_summary_fields(&fields);

        assert_eq!(summary.len(), 2);
        assert_eq!(summary[0].label, "Subject");
        assert_eq!(summary[0].value, "Building maintenance request");
        assert_eq!(summary[1].label, "Office");
    }

    #[test]
    fn trash_auto_number_display_preserves_the_configured_identity() {
        assert_eq!(
            trash_field_display_value(
                "auto_number",
                r#"{"prefix":"CASE-","padding":5}"#,
                &json!(42),
            ),
            "CASE-00042"
        );
    }
}
