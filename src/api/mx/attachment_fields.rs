use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::api::mx::schema::{FieldDefinition, ensure_dynamic_schema};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentFieldDefinition {
    pub uid: String,
    pub key: String,
    pub label: String,
    pub storage_name: String,
    pub description: String,
    pub required: bool,
    pub multiple: bool,
    pub max_files: Option<u64>,
    pub position: i64,
    pub active: bool,
}

fn clean_storage_name(value: &str) -> String {
    let mut output = String::with_capacity(value.len());

    for character in value.trim().chars() {
        if character.is_ascii_alphanumeric()
            || character == '.'
            || character == '-'
            || character == '_'
        {
            output.push(character);
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
        "Attachments".to_string()
    } else {
        output
    }
}

fn column_exists(
    connection: &rusqlite::Connection,
    table: &str,
    column: &str,
) -> rusqlite::Result<bool> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let mut rows = statement.query([])?;

    while let Some(row) = rows.next()? {
        let name: String = row.get(1)?;
        if name.eq_ignore_ascii_case(column) {
            return Ok(true);
        }
    }

    Ok(false)
}

pub(crate) fn ensure_attachment_fields_schema(
    connection: &rusqlite::Connection,
) -> rusqlite::Result<()> {
    // File attachments are ordinary dynamic MX fields whose field_type is
    // "attachments". The attachment metadata columns remain on
    // mx_attachments so N1 objects can be associated with the exact field.
    ensure_dynamic_schema(connection)?;

    if !column_exists(connection, "mx_attachments", "attachment_field_uid")? {
        connection.execute(
            "ALTER TABLE mx_attachments ADD COLUMN attachment_field_uid TEXT",
            [],
        )?;
    }

    if !column_exists(connection, "mx_attachments", "attachment_field_label")? {
        connection.execute(
            "ALTER TABLE mx_attachments ADD COLUMN attachment_field_label TEXT NOT NULL DEFAULT ''",
            [],
        )?;
    }

    if !column_exists(
        connection,
        "mx_attachments",
        "attachment_field_storage_name",
    )? {
        connection.execute(
            "ALTER TABLE mx_attachments ADD COLUMN attachment_field_storage_name TEXT NOT NULL DEFAULT ''",
            [],
        )?;
    }

    if !column_exists(connection, "mx_attachments", "created_at")? {
        connection.execute(
            "ALTER TABLE mx_attachments ADD COLUMN created_at INTEGER NOT NULL DEFAULT 0",
            [],
        )?;
        connection.execute(
            "UPDATE mx_attachments SET created_at = CAST(STRFTIME('%s','now') AS INTEGER) * 1000 WHERE created_at = 0",
            [],
        )?;
    }

    // Earlier MX builds made filenames unique across the entire record.
    // File Attachment is now a normal dynamic field, so the same filename may
    // legitimately exist in two different attachment fields.
    let table_sql: Option<String> = connection
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name='mx_attachments'",
            [],
            |row| row.get(0),
        )
        .optional()?;

    if table_sql
        .as_deref()
        .is_some_and(|sql| sql.contains("UNIQUE(entry_uid, file_name)"))
    {
        connection.execute_batch(
            r#"
            ALTER TABLE mx_attachments RENAME TO mx_attachments_old_scope;

            CREATE TABLE mx_attachments (
                uid                           TEXT PRIMARY KEY NOT NULL,
                entry_uid                     TEXT NOT NULL,
                file_name                     TEXT NOT NULL COLLATE NOCASE,
                mime_type                     TEXT NOT NULL,
                size                          INTEGER NOT NULL CHECK(size >= 0),
                object_key                    TEXT NOT NULL UNIQUE,
                version_id                    TEXT,
                attachment_field_uid          TEXT,
                attachment_field_label        TEXT NOT NULL DEFAULT '',
                attachment_field_storage_name TEXT NOT NULL DEFAULT '',
                created_at                    INTEGER NOT NULL DEFAULT 0,
                FOREIGN KEY (entry_uid)
                    REFERENCES mx_records(uid)
                    ON DELETE CASCADE,
                UNIQUE(entry_uid, attachment_field_uid, file_name)
            );

            INSERT INTO mx_attachments (
                uid,
                entry_uid,
                file_name,
                mime_type,
                size,
                object_key,
                version_id,
                attachment_field_uid,
                attachment_field_label,
                attachment_field_storage_name,
                created_at
            )
            SELECT
                uid,
                entry_uid,
                file_name,
                mime_type,
                size,
                object_key,
                version_id,
                attachment_field_uid,
                attachment_field_label,
                attachment_field_storage_name,
                created_at
            FROM mx_attachments_old_scope;

            DROP TABLE mx_attachments_old_scope;
            "#,
        )?;
    }

    connection.execute_batch(
        r#"
        CREATE INDEX IF NOT EXISTS idx_mx_attachments_entry_uid
            ON mx_attachments(entry_uid);

        CREATE INDEX IF NOT EXISTS idx_mx_attachments_file_name
            ON mx_attachments(file_name COLLATE NOCASE);

        CREATE INDEX IF NOT EXISTS idx_mx_attachments_field
            ON mx_attachments(attachment_field_uid, entry_uid);
        "#,
    )?;

    Ok(())
}

fn field_from_definition(field: FieldDefinition) -> AttachmentFieldDefinition {
    let configured_storage_name = field
        .config
        .get("storage_name")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| field.label.clone());

    let description = field
        .config
        .get("description")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_string();

    let multiple = field
        .config
        .get("multiple")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true);

    let max_files = if multiple {
        field
            .config
            .get("max_files")
            .and_then(serde_json::Value::as_u64)
            .filter(|value| *value > 0)
    } else {
        Some(1)
    };

    AttachmentFieldDefinition {
        uid: field.uid,
        key: field.key,
        label: field.label,
        storage_name: clean_storage_name(&configured_storage_name),
        description,
        required: field.required,
        multiple,
        max_files,
        position: field.position,
        active: field.active,
    }
}

fn row_to_field(row: &rusqlite::Row<'_>) -> rusqlite::Result<FieldDefinition> {
    let config_text: String = row.get(12)?;

    Ok(FieldDefinition {
        uid: row.get(0)?,
        key: row.get(1)?,
        label: row.get(2)?,
        field_type: row.get(3)?,
        required: row.get::<_, i64>(4)? != 0,
        unique_value: row.get::<_, i64>(5)? != 0,
        searchable: row.get::<_, i64>(6)? != 0,
        sortable: row.get::<_, i64>(7)? != 0,
        table_visible: row.get::<_, i64>(8)? != 0,
        table_priority: row.get(9)?,
        position: row.get(10)?,
        active: row.get::<_, i64>(11)? != 0,
        config: serde_json::from_str(&config_text).unwrap_or_else(|_| serde_json::json!({})),
    })
}

pub(crate) fn load_attachment_field_db(
    connection: &rusqlite::Connection,
    uid: &str,
    require_active: bool,
) -> rusqlite::Result<Option<AttachmentFieldDefinition>> {
    ensure_attachment_fields_schema(connection)?;

    let sql = if require_active {
        r#"
        SELECT uid, field_key, label, field_type, required, unique_value,
               searchable, sortable, table_visible, table_priority, position,
               active, config_json
        FROM mx_fields
        WHERE uid = ?1
          AND field_type = 'attachments'
          AND active = 1
        "#
    } else {
        r#"
        SELECT uid, field_key, label, field_type, required, unique_value,
               searchable, sortable, table_visible, table_priority, position,
               active, config_json
        FROM mx_fields
        WHERE uid = ?1
          AND field_type = 'attachments'
        "#
    };

    connection
        .query_row(sql, params![uid], row_to_field)
        .optional()
        .map(|field| field.map(field_from_definition))
}
