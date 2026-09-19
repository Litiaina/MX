use rusqlite::{Connection, OptionalExtension};

fn object_exists(connection: &Connection, object_type: &str, name: &str) -> rusqlite::Result<bool> {
    connection
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = ?1 AND name = ?2 LIMIT 1",
            (object_type, name),
            |_| Ok(true),
        )
        .optional()
        .map(|value| value.unwrap_or(false))
}

/// One-way compatibility migration from the former product/database prefix to MX.
///
/// New installations never create the legacy names. Existing installations are
/// renamed in place so record UIDs, attachment metadata, audit history, dashboard
/// configuration, storage-layout state, and backup metadata remain intact.
pub(crate) fn migrate_legacy_schema(connection: &Connection) -> rusqlite::Result<()> {
    const TABLES: &[(&str, &str)] = &[
        ("aris_control_sequence", "mx_control_sequence"),
        ("aris_records", "mx_records"),
        ("aris_attachments", "mx_attachments"),
        ("aris_attachments_old_scope", "mx_attachments_old_scope"),
        ("aris_revision", "mx_revision"),
        ("aris_schema_meta", "mx_schema_meta"),
        ("aris_system_fields", "mx_system_fields"),
        ("aris_fields", "mx_fields"),
        ("aris_record_values", "mx_record_values"),
        ("aris_unique_values", "mx_unique_values"),
        ("aris_field_sequences", "mx_field_sequences"),
        ("aris_attachment_fields", "mx_attachment_fields"),
        ("aris_storage_layout_meta", "mx_storage_layout_meta"),
        ("aris_storage_layout_folders", "mx_storage_layout_folders"),
        ("aris_record_storage", "mx_record_storage"),
        ("aris_dashboard_config", "mx_dashboard_config"),
        ("aris_audit_log", "mx_audit_log"),
        ("aris_backups", "mx_backups"),
    ];

    let mut has_legacy = false;
    for (legacy, _) in TABLES {
        if object_exists(connection, "table", legacy)? {
            has_legacy = true;
            break;
        }
    }

    if !has_legacy {
        return Ok(());
    }

    connection.execute_batch("PRAGMA legacy_alter_table = OFF;")?;

    for (legacy, current) in TABLES {
        let legacy_exists = object_exists(connection, "table", legacy)?;
        if !legacy_exists {
            continue;
        }

        if object_exists(connection, "table", current)? {
            return Err(rusqlite::Error::InvalidParameterName(format!(
                "MX migration conflict: both '{legacy}' and '{current}' exist"
            )));
        }

        connection.execute_batch(&format!(
            "ALTER TABLE \"{legacy}\" RENAME TO \"{current}\";"
        ))?;
    }

    // SQLite keeps index/trigger object names when a table is renamed. Remove
    // the old names so the normal MX schema initializers can recreate them with
    // the MX prefix. Table data is untouched.
    connection.execute_batch(
        r#"
        DROP INDEX IF EXISTS idx_aris_records_date;
        DROP INDEX IF EXISTS idx_aris_records_office;
        DROP INDEX IF EXISTS idx_aris_records_requestor;
        DROP INDEX IF EXISTS idx_aris_records_office_date;
        DROP INDEX IF EXISTS idx_aris_records_route_date;
        DROP INDEX IF EXISTS idx_aris_records_requestor_date;
        DROP INDEX IF EXISTS idx_aris_attachments_entry_uid;
        DROP INDEX IF EXISTS idx_aris_attachments_file_name;
        DROP INDEX IF EXISTS idx_aris_attachments_field;
        DROP INDEX IF EXISTS idx_aris_fields_active_position;
        DROP INDEX IF EXISTS idx_aris_fields_table;
        DROP INDEX IF EXISTS idx_aris_record_values_field_text;
        DROP INDEX IF EXISTS idx_aris_record_values_field_integer;
        DROP INDEX IF EXISTS idx_aris_record_values_field_real;
        DROP INDEX IF EXISTS idx_aris_record_values_record;
        DROP INDEX IF EXISTS idx_aris_record_storage_directory;
        DROP INDEX IF EXISTS idx_aris_audit_created_at;
        DROP INDEX IF EXISTS idx_aris_audit_actor_uid;
        DROP INDEX IF EXISTS idx_aris_audit_action;
        DROP INDEX IF EXISTS idx_aris_audit_target;
        DROP INDEX IF EXISTS idx_aris_backups_created_at;

        DROP TRIGGER IF EXISTS trg_aris_records_revision_insert;
        DROP TRIGGER IF EXISTS trg_aris_records_revision_update;
        DROP TRIGGER IF EXISTS trg_aris_records_revision_delete;
        DROP TRIGGER IF EXISTS trg_aris_attachments_revision_insert;
        DROP TRIGGER IF EXISTS trg_aris_attachments_revision_update;
        DROP TRIGGER IF EXISTS trg_aris_attachments_revision_delete;
        DROP TRIGGER IF EXISTS aris_audit_log_no_update;
        DROP TRIGGER IF EXISTS aris_audit_log_no_delete;
        "#,
    )?;

    Ok(())
}
