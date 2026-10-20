//! Application schema is migrated before serving requests, never by read paths.
use rusqlite::Connection;
use std::sync::OnceLock;

// Bind readiness to the actual SQLite filename, not a process-wide boolean:
// isolated/in-memory databases must still initialize their own schema.
static INITIALIZED_DATABASE: OnceLock<String> = OnceLock::new();

pub(crate) fn initialized(connection: &Connection) -> bool {
    INITIALIZED_DATABASE
        .get()
        .is_some_and(|path| connection.path() == Some(path.as_str()))
}

pub(crate) fn initialize(connection: &Connection) -> rusqlite::Result<()> {
    use crate::api;
    api::modules::ensure_module_schema(connection)?;
    api::mx::schema::ensure_dynamic_schema(connection)?;
    api::mx::records::ensure_record_collaboration_schema(connection)?;
    api::lifecycle::ensure_record_lifecycle_schema(connection)?;
    api::mx::relationships::ensure(connection)?;
    api::mx::attachment_fields::ensure_attachment_fields_schema(connection)?;
    api::mx::storage::ensure_storage_layout_schema(connection)?;
    api::account::handler::ensure_profile_photo_schema(connection)?;
    api::collaboration::ensure_collaboration_schema(connection)?;
    api::notifications::ensure_notification_schema(connection)?;
    api::preferences::ensure_preferences_schema(connection)?;
    api::audit::ensure_audit_schema(connection)?;
    api::operations::ensure_schema(connection)?;
    api::mx::imports::ensure(connection)?;
    api::mx::privacy::ensure(connection)?;
    api::drive::ensure_drive_schema(connection)?;
    api::reports::ensure_reporting_schema(connection)?;
    api::backup::ensure_backup_schema(connection)?;
    api::deployment::ensure_deployment_schema(connection)?;
    // Publish only after EVERY initializer succeeds. A failed migration cannot
    // expose partially initialized tables to HTTP handlers or background jobs.
    if let Some(path) = connection.path().filter(|path| !path.is_empty()) {
        let _ = INITIALIZED_DATABASE.set(path.to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::OpenFlags;
    use std::time::{Duration, Instant};

    #[test]
    fn initialized_reads_do_not_compete_with_a_writer_at_large_record_counts() {
        let path = std::env::temp_dir().join(format!("mx-schema-{}.db", uuid::Uuid::new_v4()));
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "PRAGMA journal_mode=WAL;
            CREATE TABLE users(uid TEXT PRIMARY KEY,access_level INTEGER NOT NULL,name TEXT NOT NULL DEFAULT '',email TEXT NOT NULL DEFAULT '');",
            )
            .unwrap();
        initialize(&connection).unwrap();
        connection.execute_batch("BEGIN IMMEDIATE;
            INSERT INTO mx_modules(uid,slug,name,singular_name,created_at,updated_at)
            VALUES('large','large','Large','Record',0,0);
            WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<50000)
            INSERT INTO mx_records(uid,control_year,control_no,date,office,requestor,subject,routed_to_div,remarks,module_uid)
            SELECT 'r-'||x,0,x,'','','','','','','large' FROM n;
            COMMIT;").unwrap();
        // A real independent writer holds SQLite's WAL write lock. Pure reads
        // must still succeed, including every previously writeful initializer.
        connection.execute_batch("BEGIN IMMEDIATE").unwrap();
        let started = Instant::now();
        let threads: Vec<_> = (0..16)
            .map(|_| {
                let path = path.clone();
                std::thread::spawn(move || {
                    let reader =
                        Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
                            .unwrap();
                    reader.busy_timeout(Duration::from_millis(250)).unwrap();
                    reader.execute_batch("PRAGMA query_only=ON").unwrap();
                    assert!(initialized(&reader));
                    initialize(&reader).unwrap();
                    let count: i64 = reader
                        .query_row("SELECT count(*) FROM mx_records", [], |r| r.get(0))
                        .unwrap();
                    assert_eq!(count, 50000);
                    crate::api::mx::schema::load_module_fields_db(&reader, "large", false).unwrap();
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
        assert!(started.elapsed() < Duration::from_secs(10));
        connection.execute_batch("ROLLBACK").unwrap();
        let isolated = Connection::open_in_memory().unwrap();
        assert!(
            !initialized(&isolated),
            "Readiness must never leak into another database"
        );
        drop(connection);
        std::fs::remove_file(&path).unwrap();
        for suffix in ["-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
        }
    }
}
