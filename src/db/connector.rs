use std::{
    collections::VecDeque,
    error::Error,
    fmt::{Display, Formatter},
    sync::{Arc, Condvar, LazyLock, Mutex},
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

type WriterWaiter = Arc<(Mutex<bool>, Condvar)>;

// WAL supports concurrent readers but only one writer. Do not let waiting
// application writers occupy every connection, race SQLite's busy handler and
// starve an older save. Admit writes FIFO BEFORE checking out a connection.
// A per-waiter signal avoids waking hundreds of threads for every commit.
#[derive(Default)]
struct WriteQueue(Mutex<VecDeque<WriterWaiter>>);

struct WriteTurn<'a>(&'a WriteQueue);

impl WriteQueue {
    fn enter(&self) -> WriteTurn<'_> {
        let waiter = Arc::new((Mutex::new(false), Condvar::new()));
        {
            let mut queue = self.0.lock().unwrap_or_else(|e| e.into_inner());
            if queue.is_empty() {
                *waiter.0.lock().unwrap_or_else(|e| e.into_inner()) = true;
            }
            queue.push_back(waiter.clone());
        }
        let mut ready = waiter.0.lock().unwrap_or_else(|e| e.into_inner());
        while !*ready {
            ready = waiter.1.wait(ready).unwrap_or_else(|e| e.into_inner());
        }
        WriteTurn(self)
    }
}

impl Drop for WriteTurn<'_> {
    fn drop(&mut self) {
        let mut queue = self.0.0.lock().unwrap_or_else(|e| e.into_inner());
        queue.pop_front();
        if let Some(next) = queue.front() {
            *next.0.lock().unwrap_or_else(|e| e.into_inner()) = true;
            next.1.notify_one();
        }
    }
}

static WRITERS: LazyLock<WriteQueue> = LazyLock::new(WriteQueue::default);
static WRITE_CONNECTION: Mutex<Option<Connection>> = Mutex::new(None);

/// For a synchronous database-only mutation. Never await, do network/file I/O,
/// or recursively call this function while holding the turn. Transactions and
/// business validation remain the caller's responsibility; reads use the
/// ordinary pool and continue concurrently.
pub fn with_sql_write<T, F>(operation: F) -> Result<T, SqliteDatabaseError>
where
    F: FnOnce(&Connection) -> rusqlite::Result<T>,
{
    let _turn = WRITERS.enter();
    let mut writer = WRITE_CONNECTION.lock().unwrap_or_else(|e| e.into_inner());
    if writer.is_none() {
        let connection = Connection::open(&CONFIG.database.db_path)?;
        connection.pragma_update(None, "synchronous", "NORMAL")?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.busy_timeout(Duration::from_millis(
            CONFIG.database.busy_timeout_milliseconds,
        ))?;
        // One bounded writer cache, rather than increasing every reader's
        // memory. Retain index pages across fan-out batches on rotational disks.
        connection.pragma_update(None, "cache_size", -16384)?;
        *writer = Some(connection);
    }
    operation(writer.as_ref().expect("writer initialized")).map_err(SqliteDatabaseError::Sqlite)
}

#[cfg(test)]
mod writer_tests {
    use super::*;

    #[test]
    fn writer_queue_admits_in_order_and_releases_after_unwind() {
        let queue = Arc::new(WriteQueue::default());
        let first = queue.enter();
        let order = Arc::new(Mutex::new(Vec::new()));
        let mut threads = Vec::new();
        for i in 0..100 {
            let q = queue.clone();
            let order = order.clone();
            threads.push(std::thread::spawn(move || {
                let _turn = q.enter();
                order.lock().unwrap().push(i);
            }));
            while queue.0.lock().unwrap().len() != i + 2 {
                std::thread::yield_now();
            }
        }
        drop(first);
        for thread in threads {
            thread.join().unwrap();
        }
        assert_eq!(*order.lock().unwrap(), (0..100).collect::<Vec<_>>());
        let q = queue.clone();
        assert!(
            std::thread::spawn(move || {
                let _turn = q.enter();
                panic!("injected write failure");
            })
            .join()
            .is_err()
        );
        let _after = queue.enter();
        assert_eq!(queue.0.lock().unwrap().len(), 1);
    }
}
