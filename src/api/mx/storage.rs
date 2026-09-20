use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    api::live::publish_live_event,
    api::mx::schema::ensure_dynamic_schema,
    db::connector::{SqliteDatabaseError, with_sql_connection},
    middleware::auth::Claims,
};

const STORAGE_ROOT: &str = "records";
const MAX_FOLDER_FIELDS: usize = 8;

#[derive(Debug, Clone, Serialize)]
pub struct StorageLayoutField {
    pub uid: String,
    pub key: String,
    pub label: String,
    pub field_type: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct StorageLayoutResponse {
    pub revision: u64,
    pub root: String,
    pub folder_fields: Vec<StorageLayoutField>,
    pub file_prefix_field: Option<StorageLayoutField>,
    pub frozen_records: u64,
    pub preview: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateStorageLayoutRequest {
    #[serde(default)]
    pub folder_field_uids: Vec<String>,

    #[serde(default)]
    pub file_prefix_field_uid: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ResolvedRecordStorage {
    pub directory_path: String,
    pub file_prefix: Option<String>,
    pub layout_revision: u64,
}

#[derive(Debug)]
pub enum StorageResolutionError {
    NotFound,
    MissingFieldValue(String),
    Database,
}

#[derive(Debug)]
enum StorageResolutionOutcome {
    Ready(ResolvedRecordStorage),
    MissingFieldValue(String),
    NotFound,
}

fn api_json(status: StatusCode, body: Value) -> Response {
    (status, Json(body)).into_response()
}

fn access_denied() -> Response {
    api_json(
        StatusCode::FORBIDDEN,
        json!({
            "response": "administrator access is required to configure the N1 storage layout"
        }),
    )
}

pub(crate) fn ensure_storage_layout_schema(
    connection: &rusqlite::Connection,
) -> rusqlite::Result<()> {
    ensure_dynamic_schema(connection)?;

    connection.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS mx_storage_layout_meta (
            id                     INTEGER PRIMARY KEY NOT NULL CHECK(id = 1),
            revision               INTEGER NOT NULL DEFAULT 0 CHECK(revision >= 0),
            file_prefix_field_uid  TEXT,
            FOREIGN KEY(file_prefix_field_uid)
                REFERENCES mx_fields(uid)
                ON DELETE RESTRICT
        );

        INSERT OR IGNORE INTO mx_storage_layout_meta (
            id,
            revision,
            file_prefix_field_uid
        ) VALUES (1, 0, NULL);

        CREATE TABLE IF NOT EXISTS mx_storage_layout_folders (
            position   INTEGER PRIMARY KEY NOT NULL CHECK(position >= 0),
            field_uid  TEXT NOT NULL UNIQUE,
            FOREIGN KEY(field_uid)
                REFERENCES mx_fields(uid)
                ON DELETE RESTRICT
        );

        CREATE TABLE IF NOT EXISTS mx_record_storage (
            record_uid       TEXT PRIMARY KEY NOT NULL,
            directory_path   TEXT NOT NULL,
            file_prefix      TEXT,
            layout_revision  INTEGER NOT NULL DEFAULT 0 CHECK(layout_revision >= 0),
            created_at       INTEGER NOT NULL,
            FOREIGN KEY(record_uid)
                REFERENCES mx_records(uid)
                ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_mx_record_storage_directory
            ON mx_record_storage(directory_path);
        "#,
    )
}

pub(crate) fn field_used_by_storage_layout_db(
    connection: &rusqlite::Connection,
    field_uid: &str,
) -> rusqlite::Result<bool> {
    ensure_storage_layout_schema(connection)?;

    let used = connection.query_row(
        r#"
        SELECT EXISTS(
            SELECT 1
            FROM mx_storage_layout_folders
            WHERE field_uid = ?1

            UNION ALL

            SELECT 1
            FROM mx_storage_layout_meta
            WHERE id = 1
              AND file_prefix_field_uid = ?1
        )
        "#,
        params![field_uid],
        |row| row.get::<_, i64>(0),
    )?;

    Ok(used != 0)
}

fn load_field(
    connection: &rusqlite::Connection,
    uid: &str,
) -> rusqlite::Result<StorageLayoutField> {
    connection.query_row(
        r#"
        SELECT
            uid,
            field_key,
            label,
            field_type
        FROM mx_fields
        WHERE uid = ?1
          AND active = 1
        "#,
        params![uid],
        |row| {
            Ok(StorageLayoutField {
                uid: row.get(0)?,
                key: row.get(1)?,
                label: row.get(2)?,
                field_type: row.get(3)?,
            })
        },
    )
}

fn load_layout_db(connection: &rusqlite::Connection) -> rusqlite::Result<StorageLayoutResponse> {
    ensure_storage_layout_schema(connection)?;

    let (revision, prefix_uid): (i64, Option<String>) = connection.query_row(
        r#"
        SELECT revision, file_prefix_field_uid
        FROM mx_storage_layout_meta
        WHERE id = 1
        "#,
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;

    let mut statement = connection.prepare(
        r#"
        SELECT f.uid, f.field_key, f.label, f.field_type
        FROM mx_storage_layout_folders layout
        JOIN mx_fields f
          ON f.uid = layout.field_uid
        WHERE f.active = 1
        ORDER BY layout.position ASC
        "#,
    )?;

    let folder_fields = statement
        .query_map([], |row| {
            Ok(StorageLayoutField {
                uid: row.get(0)?,
                key: row.get(1)?,
                label: row.get(2)?,
                field_type: row.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, rusqlite::Error>>()?;

    let file_prefix_field = match prefix_uid {
        Some(uid) => load_field(connection, &uid).optional()?,
        None => None,
    };

    let frozen_records: i64 =
        connection.query_row("SELECT COUNT(*) FROM mx_record_storage", [], |row| {
            row.get(0)
        })?;

    let mut preview_parts = vec![STORAGE_ROOT.to_string()];

    for field in &folder_fields {
        preview_parts.push(format!("<{}>", field.label));
    }

    let file_name = match &file_prefix_field {
        Some(field) => format!("<{}>__filename.ext", field.label),
        None => "filename.ext".to_string(),
    };

    preview_parts.push(file_name);

    Ok(StorageLayoutResponse {
        revision: revision.max(0) as u64,
        root: STORAGE_ROOT.to_string(),
        folder_fields,
        file_prefix_field,
        frozen_records: frozen_records.max(0) as u64,
        preview: preview_parts.join("/"),
    })
}

pub async fn get_storage_layout(claims: Claims) -> Response {
    if !claims.can_manage_accounts() {
        return access_denied();
    }

    let result = tokio::task::spawn_blocking(
        move || -> Result<StorageLayoutResponse, SqliteDatabaseError> {
            with_sql_connection(|connection| load_layout_db(connection))
        },
    )
    .await;

    match result {
        Ok(Ok(layout)) => api_json(StatusCode::OK, json!(layout)),

        Ok(Err(error)) => {
            crate::report_error!(
                format!("failed to load MX N1 storage layout: {error}"),
                "function",
                "get_storage_layout()"
            );

            api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({ "response": "failed to load N1 storage layout" }),
            )
        }

        Err(error) => {
            crate::report_error!(
                format!("storage-layout blocking task failed: {error}"),
                "function",
                "get_storage_layout()"
            );

            api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({ "response": "failed to load N1 storage layout" }),
            )
        }
    }
}

pub async fn update_storage_layout(
    claims: Claims,
    Json(request): Json<UpdateStorageLayoutRequest>,
) -> Response {
    if !claims.can_manage_accounts() {
        return access_denied();
    }

    if request.folder_field_uids.len() > MAX_FOLDER_FIELDS {
        return api_json(
            StatusCode::BAD_REQUEST,
            json!({
                "response": format!(
                    "N1 storage layout supports at most {MAX_FOLDER_FIELDS} folder fields."
                )
            }),
        );
    }

    let mut unique_uids = Vec::<String>::new();

    for uid in request.folder_field_uids {
        let uid = uid.trim().to_string();

        if uid.is_empty() {
            continue;
        }

        if unique_uids.iter().any(|existing| existing == &uid) {
            return api_json(
                StatusCode::BAD_REQUEST,
                json!({ "response": "The same field cannot be used twice in the N1 folder chain." }),
            );
        }

        unique_uids.push(uid);
    }

    let prefix_uid = request
        .file_prefix_field_uid
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    let database_result = tokio::task::spawn_blocking(
        move || -> Result<StorageLayoutResponse, SqliteDatabaseError> {
            with_sql_connection(|connection| {
                ensure_storage_layout_schema(connection)?;

                for uid in &unique_uids {
                    let field = load_field(connection, uid).map_err(|_| {
                        rusqlite::Error::InvalidParameterName(
                            "MX_STORAGE_FIELD_NOT_FOUND".to_string(),
                        )
                    })?;
                    if field.field_type == "attachments" {
                        return Err(rusqlite::Error::InvalidParameterName(
                            "MX_STORAGE_ATTACHMENT_FIELD".to_string(),
                        ));
                    }
                }

                if let Some(uid) = prefix_uid.as_deref() {
                    let field = load_field(connection, uid).map_err(|_| {
                        rusqlite::Error::InvalidParameterName(
                            "MX_STORAGE_FIELD_NOT_FOUND".to_string(),
                        )
                    })?;
                    if field.field_type == "attachments" {
                        return Err(rusqlite::Error::InvalidParameterName(
                            "MX_STORAGE_ATTACHMENT_FIELD".to_string(),
                        ));
                    }
                }

                let transaction = connection.unchecked_transaction()?;

                transaction.execute("DELETE FROM mx_storage_layout_folders", [])?;

                for (position, uid) in unique_uids.iter().enumerate() {
                    transaction.execute(
                        r#"
                        INSERT INTO mx_storage_layout_folders (
                            position,
                            field_uid
                        ) VALUES (?1, ?2)
                        "#,
                        params![position as i64, uid],
                    )?;
                }

                transaction.execute(
                    r#"
                    UPDATE mx_storage_layout_meta
                    SET
                        revision = revision + 1,
                        file_prefix_field_uid = ?1
                    WHERE id = 1
                    "#,
                    params![prefix_uid],
                )?;

                transaction.commit()?;

                load_layout_db(connection)
            })
        },
    )
    .await;

    match database_result {
        Ok(Ok(layout)) => {
            publish_live_event(
                "storage.updated",
                Some(&claims.uid),
                json!({"revision": layout.revision}),
            );
            api_json(
                StatusCode::OK,
                json!({
                    "response": "N1 storage layout updated",
                    "layout": layout
                }),
            )
        }

        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::InvalidParameterName(message))))
            if message == "MX_STORAGE_FIELD_NOT_FOUND" =>
        {
            api_json(
                StatusCode::BAD_REQUEST,
                json!({
                    "response": "One or more selected N1 storage fields no longer exist or are archived."
                }),
            )
        }

        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::InvalidParameterName(message))))
            if message == "MX_STORAGE_ATTACHMENT_FIELD" =>
        {
            api_json(
                StatusCode::BAD_REQUEST,
                json!({
                    "response": "File Attachment fields cannot be used as N1 base folders or filename prefixes. Their folder is appended automatically when files are uploaded."
                }),
            )
        }

        Ok(Err(error)) => {
            crate::report_error!(
                format!("failed to update MX N1 storage layout: {error}"),
                "function",
                "update_storage_layout()"
            );

            api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({ "response": "failed to update N1 storage layout" }),
            )
        }

        Err(error) => {
            crate::report_error!(
                format!("storage-layout update blocking task failed: {error}"),
                "function",
                "update_storage_layout()"
            );

            api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({ "response": "failed to update N1 storage layout" }),
            )
        }
    }
}

fn sanitize_component(value: &str, fallback: &str) -> String {
    let mut output = String::with_capacity(value.len());

    for character in value.trim().chars() {
        if character.is_ascii_alphanumeric()
            || character == '.'
            || character == '-'
            || character == '_'
        {
            output.push(character);
        } else if character.is_whitespace() {
            output.push('_');
        } else {
            output.push('_');
        }
    }

    while output.contains("__") {
        output = output.replace("__", "_");
    }

    let mut output = output
        .trim_matches(|character| character == '.' || character == '_' || character == '-')
        .to_string();

    if output.len() > 96 {
        output.truncate(96);
        output = output
            .trim_matches(|character| character == '.' || character == '_' || character == '-')
            .to_string();
    }

    if output.is_empty() {
        fallback.to_string()
    } else {
        output
    }
}

fn render_record_field_value(
    connection: &rusqlite::Connection,
    record_uid: &str,
    field_uid: &str,
) -> rusqlite::Result<Option<(String, String)>> {
    connection
        .query_row(
            r#"
            SELECT
                f.label,
                f.field_type,
                f.config_json,
                rv.value_text,
                rv.value_integer,
                rv.value_real,
                rv.value_boolean
            FROM mx_fields f
            LEFT JOIN mx_record_values rv
              ON rv.field_uid = f.uid
             AND rv.record_uid = ?1
            WHERE f.uid = ?2
              AND f.active = 1
            "#,
            params![record_uid, field_uid],
            |row| {
                let label: String = row.get(0)?;
                let field_type: String = row.get(1)?;
                let config_text: String = row.get(2)?;
                let text: Option<String> = row.get(3)?;
                let integer: Option<i64> = row.get(4)?;
                let real: Option<f64> = row.get(5)?;
                let boolean: Option<i64> = row.get(6)?;

                let rendered = match field_type.as_str() {
                    "integer" => integer.map(|value| value.to_string()),

                    "auto_number" => integer.map(|value| {
                        let config: Value =
                            serde_json::from_str(&config_text).unwrap_or_else(|_| json!({}));

                        let prefix = config.get("prefix").and_then(Value::as_str).unwrap_or("");

                        let padding = config
                            .get("padding")
                            .and_then(Value::as_u64)
                            .unwrap_or(0)
                            .min(12) as usize;

                        if padding > 0 {
                            format!("{prefix}{:0width$}", value, width = padding)
                        } else {
                            format!("{prefix}{value}")
                        }
                    }),

                    "decimal" => real.map(|value| value.to_string()),

                    "boolean" => boolean.map(|value| {
                        if value != 0 {
                            "Yes".to_string()
                        } else {
                            "No".to_string()
                        }
                    }),

                    _ => text.map(|value| value.trim().to_string()),
                }
                .filter(|value| !value.trim().is_empty());

                Ok(rendered.map(|value| (label, value)))
            },
        )
        .optional()
        .map(|value| value.flatten())
}

fn resolve_record_storage_db(
    connection: &rusqlite::Connection,
    record_uid: &str,
) -> rusqlite::Result<StorageResolutionOutcome> {
    ensure_storage_layout_schema(connection)?;

    let record_exists = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM mx_records WHERE uid = ?1)",
        params![record_uid],
        |row| row.get::<_, i64>(0),
    )? != 0;

    if !record_exists {
        return Ok(StorageResolutionOutcome::NotFound);
    }

    if let Some(existing) = connection
        .query_row(
            r#"
            SELECT directory_path, file_prefix, layout_revision
            FROM mx_record_storage
            WHERE record_uid = ?1
            "#,
            params![record_uid],
            |row| {
                Ok(ResolvedRecordStorage {
                    directory_path: row.get(0)?,
                    file_prefix: row.get(1)?,
                    layout_revision: row.get::<_, i64>(2)?.max(0) as u64,
                })
            },
        )
        .optional()?
    {
        return Ok(StorageResolutionOutcome::Ready(existing));
    }

    /*
     * Preserve an already-used legacy directory when a record predates the
     * configurable layout. This prevents one record's files from being split
     * across two N1 namespaces merely because MX was upgraded.
     */
    if let Some(existing_object_key) = connection
        .query_row(
            r#"
            SELECT object_key
            FROM mx_attachments
            WHERE entry_uid = ?1
            ORDER BY rowid ASC
            LIMIT 1
            "#,
            params![record_uid],
            |row| row.get::<_, String>(0),
        )
        .optional()?
    {
        let directory_path = existing_object_key
            .rsplit_once('/')
            .map(|(directory, _)| directory.to_string())
            .unwrap_or_else(|| STORAGE_ROOT.to_string());

        let (layout_revision, _): (i64, Option<String>) = connection.query_row(
            r#"
            SELECT revision, file_prefix_field_uid
            FROM mx_storage_layout_meta
            WHERE id = 1
            "#,
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;

        connection.execute(
            r#"
            INSERT OR IGNORE INTO mx_record_storage (
                record_uid,
                directory_path,
                file_prefix,
                layout_revision,
                created_at
            ) VALUES (?1, ?2, NULL, ?3, CAST(STRFTIME('%s', 'now') AS INTEGER))
            "#,
            params![record_uid, &directory_path, layout_revision.max(0)],
        )?;

        return Ok(StorageResolutionOutcome::Ready(ResolvedRecordStorage {
            directory_path,
            file_prefix: None,
            layout_revision: layout_revision.max(0) as u64,
        }));
    }

    let (layout_revision, prefix_uid): (i64, Option<String>) = connection.query_row(
        r#"
        SELECT revision, file_prefix_field_uid
        FROM mx_storage_layout_meta
        WHERE id = 1
        "#,
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;

    let folder_uids = {
        let mut statement = connection.prepare(
            r#"
            SELECT field_uid
            FROM mx_storage_layout_folders
            ORDER BY position ASC
            "#,
        )?;

        statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, rusqlite::Error>>()?
    };

    /*
     * No custom layout yet: preserve the safe stable fallback instead of
     * risking collisions in the N1 root.
     */
    if folder_uids.is_empty() && prefix_uid.is_none() {
        let directory_path = format!(
            "{STORAGE_ROOT}/{}",
            sanitize_component(record_uid, "record")
        );

        connection.execute(
            r#"
            INSERT OR IGNORE INTO mx_record_storage (
                record_uid,
                directory_path,
                file_prefix,
                layout_revision,
                created_at
            ) VALUES (?1, ?2, NULL, ?3, CAST(STRFTIME('%s', 'now') AS INTEGER))
            "#,
            params![record_uid, &directory_path, layout_revision.max(0)],
        )?;

        return Ok(StorageResolutionOutcome::Ready(ResolvedRecordStorage {
            directory_path,
            file_prefix: None,
            layout_revision: layout_revision.max(0) as u64,
        }));
    }

    let mut components = vec![STORAGE_ROOT.to_string()];

    for field_uid in &folder_uids {
        let value = render_record_field_value(connection, record_uid, field_uid)?;

        let Some((label, value)) = value else {
            let label = load_field(connection, field_uid)
                .map(|field| field.label)
                .unwrap_or_else(|_| field_uid.clone());

            return Ok(StorageResolutionOutcome::MissingFieldValue(label));
        };

        components.push(sanitize_component(&value, &label));
    }

    let file_prefix = match prefix_uid.as_deref() {
        Some(field_uid) => {
            let value = render_record_field_value(connection, record_uid, field_uid)?;

            let Some((_label, value)) = value else {
                let label = load_field(connection, field_uid)
                    .map(|field| field.label)
                    .unwrap_or_else(|_| field_uid.to_string());

                return Ok(StorageResolutionOutcome::MissingFieldValue(label));
            };

            Some(sanitize_component(&value, "record"))
        }
        None => None,
    };

    let resolved = ResolvedRecordStorage {
        directory_path: components.join("/"),
        file_prefix,
        layout_revision: layout_revision.max(0) as u64,
    };

    connection.execute(
        r#"
        INSERT OR IGNORE INTO mx_record_storage (
            record_uid,
            directory_path,
            file_prefix,
            layout_revision,
            created_at
        ) VALUES (?1, ?2, ?3, ?4, CAST(STRFTIME('%s', 'now') AS INTEGER))
        "#,
        params![
            record_uid,
            &resolved.directory_path,
            &resolved.file_prefix,
            resolved.layout_revision as i64,
        ],
    )?;

    /* Another worker may have frozen the path first. Always return DB truth. */
    let frozen = connection.query_row(
        r#"
        SELECT directory_path, file_prefix, layout_revision
        FROM mx_record_storage
        WHERE record_uid = ?1
        "#,
        params![record_uid],
        |row| {
            Ok(ResolvedRecordStorage {
                directory_path: row.get(0)?,
                file_prefix: row.get(1)?,
                layout_revision: row.get::<_, i64>(2)?.max(0) as u64,
            })
        },
    )?;

    Ok(StorageResolutionOutcome::Ready(frozen))
}

pub(crate) async fn resolve_record_storage(
    record_uid: String,
) -> Result<ResolvedRecordStorage, StorageResolutionError> {
    let database_result = tokio::task::spawn_blocking(
        move || -> Result<StorageResolutionOutcome, SqliteDatabaseError> {
            with_sql_connection(|connection| resolve_record_storage_db(connection, &record_uid))
        },
    )
    .await;

    match database_result {
        Ok(Ok(StorageResolutionOutcome::Ready(storage))) => Ok(storage),
        Ok(Ok(StorageResolutionOutcome::MissingFieldValue(label))) => {
            Err(StorageResolutionError::MissingFieldValue(label))
        }
        Ok(Ok(StorageResolutionOutcome::NotFound)) => Err(StorageResolutionError::NotFound),
        Ok(Err(_)) | Err(_) => Err(StorageResolutionError::Database),
    }
}
