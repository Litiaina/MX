use crate::{config::load_config::CONFIG, db::connector::with_sql_connection};

pub async fn initialize_sql_db() {
    let initialization_result = tokio::task::spawn_blocking(|| {
        with_sql_connection(|connection| {
            connection.execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS users (
                    uid          TEXT PRIMARY KEY NOT NULL,
                    email        TEXT NOT NULL UNIQUE,
                    password     TEXT NOT NULL,
                    name         TEXT NOT NULL,
                    created_at   INTEGER NOT NULL,
                    access_level INTEGER NOT NULL,
                    totp_secret  TEXT
                );
                "#,
            )?;

            connection.execute_batch(
                r#"
                PRAGMA foreign_keys = ON;

                CREATE TABLE IF NOT EXISTS dgs_control_sequence (
                    year        INTEGER PRIMARY KEY NOT NULL,
                    last_value  INTEGER NOT NULL DEFAULT 0
                                CHECK(last_value >= 0)
                );

                CREATE TABLE IF NOT EXISTS dgs_entries (
                    uid             TEXT PRIMARY KEY NOT NULL,

                    control_year    INTEGER NOT NULL,
                    control_no      INTEGER NOT NULL
                                    CHECK(control_no > 0),

                    date            TEXT NOT NULL,
                    office          TEXT NOT NULL,
                    requestor       TEXT NOT NULL,
                    subject         TEXT NOT NULL,
                    routed_to_div   TEXT NOT NULL,
                    remarks         TEXT NOT NULL,

                    UNIQUE(control_year, control_no)
                );

                CREATE TABLE IF NOT EXISTS dgs_attachments (
                    uid         TEXT PRIMARY KEY NOT NULL,
                    entry_uid   TEXT NOT NULL,
                    file_name   TEXT NOT NULL COLLATE NOCASE,
                    mime_type   TEXT NOT NULL,
                    size        INTEGER NOT NULL CHECK(size >= 0),
                    object_key  TEXT NOT NULL UNIQUE,
                    version_id  TEXT,

                    FOREIGN KEY (entry_uid)
                        REFERENCES dgs_entries(uid)
                        ON DELETE CASCADE,

                    UNIQUE(entry_uid, file_name)
                );

                CREATE INDEX IF NOT EXISTS idx_dgs_entries_date
                    ON dgs_entries(date);

                CREATE INDEX IF NOT EXISTS idx_dgs_entries_control
                    ON dgs_entries(control_year, control_no);

                CREATE INDEX IF NOT EXISTS idx_dgs_entries_office
                    ON dgs_entries(office);

                CREATE INDEX IF NOT EXISTS idx_dgs_entries_requestor
                    ON dgs_entries(requestor);

                CREATE INDEX IF NOT EXISTS idx_dgs_entries_routed
                    ON dgs_entries(routed_to_div);

                CREATE INDEX IF NOT EXISTS idx_dgs_attachments_entry_uid
                    ON dgs_attachments(entry_uid);

                CREATE INDEX IF NOT EXISTS idx_dgs_attachments_file_name
                    ON dgs_attachments(file_name);

                INSERT INTO dgs_control_sequence(year, last_value)
                SELECT control_year, MAX(control_no)
                FROM dgs_entries
                GROUP BY control_year
                ON CONFLICT(year) DO UPDATE SET
                    last_value = MAX(
                        dgs_control_sequence.last_value,
                        excluded.last_value
                    );
                "#,
            )?;

            let journal_mode: String =
                connection.query_row("PRAGMA journal_mode;", [], |row| row.get(0))?;

            Ok(journal_mode)
        })
    })
    .await;

    match initialization_result {
        Ok(Ok(journal_mode)) if journal_mode.eq_ignore_ascii_case("wal") => {
            tracing::info!(
                "SQLite database '{}' initialized in WAL mode with {} pooled connections.",
                CONFIG.database.db_path,
                CONFIG.database.pool_max_size
            );
        }

        Ok(Ok(journal_mode)) => {
            crate::fatal_error!(
                format!(
                    "SQLite database '{}' initialized with unexpected journal mode '{}'; expected 'wal'",
                    CONFIG.database.db_path, journal_mode
                ),
                "function",
                "initialize_sql_db()"
            );
        }

        Ok(Err(error)) => {
            crate::fatal_error!(
                format!(
                    "failed to initialize SQLite database '{}': {}",
                    CONFIG.database.db_path, error
                ),
                "function",
                "initialize_sql_db()"
            );
        }

        Err(error) => {
            crate::fatal_error!(
                format!(
                    "SQLite database initialization blocking task failed: {}",
                    error
                ),
                "function",
                "initialize_sql_db()"
            );
        }
    }
}
