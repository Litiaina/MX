use std::{
    collections::VecDeque,
    error::Error,
    fmt::{Display, Formatter},
    sync::{Arc, Condvar, LazyLock, Mutex},
    time::{Duration, Instant},
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
        super::checkpoint::configure(connection)?;
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
    let started = Instant::now();
    let connection = get_sql_connection()?;
    super::checkpoint::configure(&connection)?;
    let changes = connection.total_changes();
    let admitted = Instant::now();
    let result = operation(&connection).map_err(SqliteDatabaseError::Sqlite);
    super::checkpoint::changed(&connection, changes);
    let execution = admitted.elapsed();
    let waiting = admitted.duration_since(started);
    if waiting >= Duration::from_secs(1) || execution >= Duration::from_secs(1) {
        // Static Rust operation names only: never log SQL, tokens or field values.
        tracing::warn!(
            operation = std::any::type_name::<F>(),
            pool_wait_ms = waiting.as_millis() as u64,
            execution_ms = execution.as_millis() as u64,
            "Slow SQLite pooled operation"
        );
    }
    result
}

type WriterWaiter = Arc<(Mutex<bool>, Condvar)>;

struct QueuedWriter {
    delivery: bool,
    waiter: WriterWaiter,
}

// WAL supports concurrent readers but only one writer. Do not let waiting
// application writers occupy every connection, race SQLite's busy handler and
// starve an older save. Admit writes FIFO BEFORE checking out a connection.
// A per-waiter signal avoids waking hundreds of threads for every commit.
#[derive(Default)]
struct WriterState {
    pending: VecDeque<QueuedWriter>,
    foreground_turns: usize,
}

#[derive(Default)]
struct WriteQueue(Mutex<WriterState>);

// Reserve progress for delivery without paying a 100-recipient transaction
// after EVERY foreground commit on rotational storage. This is a scheduling
// bound, not a change to durability, transaction size or endpoint deadlines.
const FOREGROUND_TURNS_PER_DELIVERY: usize = 4;

struct WriteTurn<'a>(&'a WriteQueue);

impl WriteQueue {
    fn enter(&self) -> WriteTurn<'_> {
        self.enter_kind(false)
    }

    fn enter_kind(&self, delivery: bool) -> WriteTurn<'_> {
        let waiter = Arc::new((Mutex::new(false), Condvar::new()));
        {
            let mut queue = self.0.lock().unwrap_or_else(|e| e.into_inner());
            if queue.pending.is_empty() {
                *waiter.0.lock().unwrap_or_else(|e| e.into_inner()) = true;
            }
            queue.pending.push_back(QueuedWriter {
                delivery,
                waiter: waiter.clone(),
            });
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
        let previous = queue.pending.pop_front();
        // Foreground writes remain FIFO relative to one another. Interleave
        // bounded delivery slices: placing them behind all 100 active
        // clients on every pass lets notification debt grow without bound.
        // Conversely a delivery backlog must never starve foreground saves.
        if let Some(previous) = previous {
            queue.foreground_turns = if previous.delivery {
                0
            } else {
                (queue.foreground_turns + 1).min(FOREGROUND_TURNS_PER_DELIVERY)
            };
            let prefer_delivery = queue.foreground_turns >= FOREGROUND_TURNS_PER_DELIVERY;
            if let Some(index) = queue
                .pending
                .iter()
                .position(|entry| entry.delivery == prefer_delivery)
            {
                let next = queue.pending.remove(index).expect("queued writer exists");
                queue.pending.push_front(next);
            }
        }
        if let Some(next) = queue.pending.front() {
            *next.waiter.0.lock().unwrap_or_else(|e| e.into_inner()) = true;
            next.waiter.1.notify_one();
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
    with_write_admission(operation, false)
}

/// Only for the outbox's bounded, atomic notification delivery slice. Shares
/// the same connection and durable transactions; neither lane may monopolize it.
pub(crate) fn with_sql_delivery<T, F>(operation: F) -> Result<T, SqliteDatabaseError>
where
    F: FnOnce(&Connection) -> rusqlite::Result<T>,
{
    with_write_admission(operation, true)
}

fn with_write_admission<T, F>(operation: F, delivery: bool) -> Result<T, SqliteDatabaseError>
where
    F: FnOnce(&Connection) -> rusqlite::Result<T>,
{
    let started = Instant::now();
    let _turn = if delivery {
        WRITERS.enter_kind(true)
    } else {
        WRITERS.enter()
    };
    let admitted = Instant::now();
    let mut writer = WRITE_CONNECTION.lock().unwrap_or_else(|e| e.into_inner());
    if writer.is_none() {
        let connection = Connection::open(&CONFIG.database.db_path)?;
        super::checkpoint::configure(&connection)?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.busy_timeout(Duration::from_millis(
            CONFIG.database.busy_timeout_milliseconds,
        ))?;
        // One bounded writer cache, rather than increasing every reader's
        // memory. Retain index pages across fan-out batches on rotational disks.
        connection.pragma_update(None, "cache_size", -16384)?;
        // Reuse hot parameterized statements, not cached authorization/results.
        // This bounded metadata cache avoids reparsing each recipient insert.
        connection.set_prepared_statement_cache_capacity(64);
        *writer = Some(connection);
    }
    let connection = writer.as_ref().expect("writer initialized");
    super::checkpoint::configure(connection)?;
    let changes = connection.total_changes();
    let result = operation(connection).map_err(SqliteDatabaseError::Sqlite);
    super::checkpoint::changed(connection, changes);
    let execution = admitted.elapsed();
    let waiting = admitted.duration_since(started);
    if waiting >= Duration::from_secs(1) || execution >= Duration::from_secs(1) {
        tracing::warn!(
            operation = std::any::type_name::<F>(),
            queue_wait_ms = waiting.as_millis() as u64,
            execution_ms = execution.as_millis() as u64,
            "Slow SQLite serialized mutation"
        );
    }
    result
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
            while queue.0.lock().unwrap().pending.len() != i + 2 {
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
        assert_eq!(queue.0.lock().unwrap().pending.len(), 1);
    }

    #[test]
    fn delivery_interleaves_without_starving_foreground_or_reordering_either_lane() {
        let queue = Arc::new(WriteQueue::default());
        let first = queue.enter();
        let order = Arc::new(Mutex::new(Vec::new()));
        let mut threads = Vec::new();
        // A delivery slice queued after a foreground burst must not wait for
        // that whole burst. Nor may delivery repeatedly preempt foreground
        // writes: allow at most four foreground turns before each ready slice.
        for (index, (name, delivery)) in [
            ("foreground-1", false),
            ("foreground-2", false),
            ("foreground-3", false),
            ("foreground-4", false),
            ("foreground-5", false),
            ("delivery-1", true),
            ("delivery-2", true),
        ]
        .into_iter()
        .enumerate()
        {
            let queue_for_thread = queue.clone();
            let order = order.clone();
            threads.push(std::thread::spawn(move || {
                let _turn = queue_for_thread.enter_kind(delivery);
                order.lock().unwrap().push(name);
            }));
            while queue.0.lock().unwrap().pending.len() != index + 2 {
                std::thread::yield_now();
            }
        }
        drop(first);
        for thread in threads {
            thread.join().unwrap();
        }
        assert_eq!(
            *order.lock().unwrap(),
            [
                "foreground-1",
                "foreground-2",
                "foreground-3",
                "delivery-1",
                "foreground-4",
                "foreground-5",
                "delivery-2"
            ]
        );
        assert!(queue.0.lock().unwrap().pending.is_empty());
    }
}
