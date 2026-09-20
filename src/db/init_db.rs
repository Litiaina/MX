use crate::{
    config::load_config::CONFIG, db::connector::with_sql_connection, util::password::hash_password,
};

pub async fn initialize_sql_db() {
    let initialization_result = tokio::task::spawn_blocking(|| {
        with_sql_connection(|connection| {
            connection.execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS users (
                    uid          TEXT PRIMARY KEY NOT NULL,
                    email        TEXT NOT NULL UNIQUE,
                    password     TEXT NOT NULL DEFAULT '' CHECK(password = ''),
                    password_hash TEXT NOT NULL CHECK(password_hash GLOB '$argon2id$*'),
                    name         TEXT NOT NULL,
                    created_at   INTEGER NOT NULL,
                    access_level INTEGER NOT NULL,
                    totp_secret  TEXT,
                    totp_pending_secret TEXT,
                    totp_pending_created_at INTEGER,
                    auth_version INTEGER NOT NULL DEFAULT 1
                );
                "#,
            )?;

            // MX 1.0 account-security migration. The legacy `password` column
            // is retained only for SQLite compatibility and is always blanked.
            let user_columns = {
                let mut statement = connection.prepare("PRAGMA table_info(users)")?;
                let rows = statement.query_map([], |row| row.get::<_, String>(1))?;
                rows.collect::<Result<Vec<_>, _>>()?
            };

            if !user_columns.iter().any(|column| column == "password_hash") {
                connection.execute("ALTER TABLE users ADD COLUMN password_hash TEXT", [])?;
            }

            if !user_columns.iter().any(|column| column == "auth_version") {
                connection.execute(
                    "ALTER TABLE users ADD COLUMN auth_version INTEGER NOT NULL DEFAULT 1",
                    [],
                )?;
            }

            if !user_columns
                .iter()
                .any(|column| column == "totp_pending_secret")
            {
                connection.execute("ALTER TABLE users ADD COLUMN totp_pending_secret TEXT", [])?;
            }

            if !user_columns
                .iter()
                .any(|column| column == "totp_pending_created_at")
            {
                connection.execute(
                    "ALTER TABLE users ADD COLUMN totp_pending_created_at INTEGER",
                    [],
                )?;
            }

            // Eagerly remove plaintext credentials left by pre-release builds.
            // New and migrated accounts therefore only persist Argon2id hashes.
            let legacy_users = {
                let mut statement = connection.prepare(
                    r#"
                        SELECT uid, password, password_hash
                        FROM users
                        WHERE password <> ''
                    "#,
                )?;
                let rows = statement.query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                })?;
                rows.collect::<Result<Vec<_>, _>>()?
            };

            for (uid, password, existing_hash) in legacy_users {
                let password_hash =
                    match existing_hash.filter(|hash| hash.starts_with("$argon2id$")) {
                        Some(password_hash) => password_hash,
                        None => hash_password(&password).map_err(|error| {
                            rusqlite::Error::ToSqlConversionFailure(Box::new(error))
                        })?,
                    };
                connection.execute(
                    "UPDATE users SET password_hash = ?1, password = '' WHERE uid = ?2",
                    rusqlite::params![password_hash, uid],
                )?;
            }

            connection.execute_batch(
                r#"
                PRAGMA foreign_keys = ON;

                CREATE TABLE IF NOT EXISTS user_recovery_codes (
                    user_uid    TEXT NOT NULL,
                    code_digest TEXT NOT NULL UNIQUE,
                    created_at  INTEGER NOT NULL,
                    PRIMARY KEY (user_uid, code_digest),
                    FOREIGN KEY (user_uid)
                        REFERENCES users(uid)
                        ON DELETE CASCADE
                );

                CREATE INDEX IF NOT EXISTS idx_user_recovery_codes_user
                    ON user_recovery_codes(user_uid);

                CREATE TRIGGER IF NOT EXISTS users_reject_plaintext_password_insert
                BEFORE INSERT ON users
                WHEN NEW.password <> ''
                BEGIN
                    SELECT RAISE(ABORT, 'plaintext passwords are not permitted');
                END;

                CREATE TRIGGER IF NOT EXISTS users_reject_plaintext_password_update
                BEFORE UPDATE OF password ON users
                WHEN NEW.password <> ''
                BEGIN
                    SELECT RAISE(ABORT, 'plaintext passwords are not permitted');
                END;

                CREATE TRIGGER IF NOT EXISTS users_require_password_hash_insert
                BEFORE INSERT ON users
                WHEN NEW.password_hash IS NULL
                    OR NEW.password_hash NOT GLOB '$argon2id$*'
                BEGIN
                    SELECT RAISE(ABORT, 'an Argon2id password_hash is required');
                END;

                CREATE TRIGGER IF NOT EXISTS users_require_password_hash_update
                BEFORE UPDATE OF password_hash ON users
                WHEN NEW.password_hash IS NULL
                    OR NEW.password_hash NOT GLOB '$argon2id$*'
                BEGIN
                    SELECT RAISE(ABORT, 'an Argon2id password_hash is required');
                END;

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
