use std::{
    error::Error,
    fmt::{Display, Formatter},
    sync::LazyLock,
    time::Duration,
};

use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::Connection;

use crate::config::load_config::CONFIG;

pub type SqlitePool = Pool<SqliteConnectionManager>;
pub type SqlitePooledConnection = PooledConnection<SqliteConnectionManager>;

#[derive(Debug)]
pub enum SqliteDatabaseError {
    Pool(r2d2::Error),
    Sqlite(rusqlite::Error),
}

impl Display for SqliteDatabaseError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pool(error) => write!(formatter, "SQLite connection pool error: {}", error),
            Self::Sqlite(error) => write!(formatter, "SQLite database error: {}", error),
        }
    }
}

impl Error for SqliteDatabaseError {}

impl From<r2d2::Error> for SqliteDatabaseError {
    fn from(error: r2d2::Error) -> Self {
        Self::Pool(error)
    }
}

impl From<rusqlite::Error> for SqliteDatabaseError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sqlite(error)
    }
}

pub static SQL_DB: LazyLock<SqlitePool> = LazyLock::new(|| {
    let database_config = &CONFIG.database;
    let database_path = database_config.db_path.clone();
    let busy_timeout = Duration::from_millis(database_config.busy_timeout_milliseconds);

    let bootstrap_connection = Connection::open(&database_path).unwrap_or_else(|error| {
        crate::fatal_error!(
            format!(
                "failed to open SQLite database '{}' while enabling WAL mode: {}",
                database_path, error
            ),
            "static",
            "SQL_DB"
        )
    });

    bootstrap_connection
        .busy_timeout(busy_timeout)
        .unwrap_or_else(|error| {
            crate::fatal_error!(
                format!(
                    "failed to configure SQLite busy timeout for database '{}': {}",
                    database_path, error
                ),
                "static",
                "SQL_DB"
            )
        });

    bootstrap_connection
        .pragma_update(None, "journal_mode", "WAL")
        .unwrap_or_else(|error| {
            crate::fatal_error!(
                format!(
                    "failed to enable WAL mode for SQLite database '{}': {}",
                    database_path, error
                ),
                "static",
                "SQL_DB"
            )
        });

    drop(bootstrap_connection);

    let manager = SqliteConnectionManager::file(&database_path).with_init(move |connection| {
        connection.pragma_update(None, "synchronous", "NORMAL")?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.busy_timeout(busy_timeout)?;

        Ok(())
    });

    Pool::builder()
        .max_size(database_config.pool_max_size)
        .min_idle(Some(database_config.pool_min_idle))
        .connection_timeout(Duration::from_secs(
            database_config.pool_connection_timeout_seconds,
        ))
        .test_on_check_out(true)
        .build(manager)
        .unwrap_or_else(|error| {
            crate::fatal_error!(
                format!(
                    "failed to create SQLite connection pool for database '{}': {}",
                    database_path, error
                ),
                "static",
                "SQL_DB"
            )
        })
});

pub fn get_sql_connection() -> Result<SqlitePooledConnection, SqliteDatabaseError> {
    SQL_DB.get().map_err(SqliteDatabaseError::Pool)
}

pub fn with_sql_connection<T, F>(operation: F) -> Result<T, SqliteDatabaseError>
where
    F: FnOnce(&Connection) -> rusqlite::Result<T>,
{
    let connection = get_sql_connection()?;
    operation(&connection).map_err(SqliteDatabaseError::Sqlite)
}
