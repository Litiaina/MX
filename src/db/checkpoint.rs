//! Keep database-page checkpoint I/O out of a foreground writer's FIFO turn.
//! FULL synchronous commits still sync the WAL before acknowledging a save.
use rusqlite::{Connection, OpenFlags};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[derive(Default)]
struct State {
    ready: AtomicBool,
    stop: AtomicBool,
    generation: AtomicU64,
    completed_generation: AtomicU64,
    incomplete_passes: AtomicU64,
}

static STATE: OnceLock<Arc<State>> = OnceLock::new();
static WORKER: OnceLock<Worker> = OnceLock::new();

struct Worker {
    state: Arc<State>,
    thread: JoinHandle<()>,
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.state.stop.store(true, Ordering::Release);
        self.thread.thread().unpark();
    }
}

pub(super) fn configure(connection: &Connection) -> rusqlite::Result<()> {
    let managed = STATE
        .get()
        .is_some_and(|state| state.ready.load(Ordering::Acquire));
    configure_connection(connection, managed)
}

fn configure_connection(connection: &Connection, managed: bool) -> rusqlite::Result<()> {
    // Unlike NORMAL, FULL syncs each committed WAL transaction, independently
    // of checkpoint timing. Never acknowledge a relaxed-durability benchmark.
    connection.pragma_update(None, "synchronous", "FULL")?;
    connection.pragma_update(None, "wal_autocheckpoint", if managed { 0 } else { 1000 })?;
    // Recycle, rather than retain, a temporary large high-water WAL allocation.
    // This is not a hard bound while a long-lived reader pins WAL frames.
    connection.pragma_update(None, "journal_size_limit", 16 * 1024 * 1024)?;
    Ok(())
}

pub(super) fn changed(connection: &Connection, previous_changes: u64) {
    if connection.total_changes() != previous_changes {
        if let Some(state) = STATE.get() {
            state.generation.fetch_add(1, Ordering::Release);
        }
    }
}

pub(crate) fn start() {
    WORKER.get_or_init(|| {
        let state = STATE.get_or_init(|| Arc::new(State::default())).clone();
        // The server initializes schema first. The worker must never create a
        // different/empty database after a deployment-path mistake.
        spawn(
            crate::config::load_config::CONFIG
                .database
                .db_path
                .clone()
                .into(),
            state,
        )
        .unwrap_or_else(|error| {
            crate::fatal_error!(
                format!("failed to start SQLite checkpoint worker: {error}"),
                "database",
                "checkpoint"
            )
        })
    });
}

fn spawn(path: PathBuf, state: Arc<State>) -> std::io::Result<Worker> {
    let thread_state = state.clone();
    let thread = thread::Builder::new().name("mx-checkpoint".into()).spawn(move || {
        let state = thread_state;
        while !state.stop.load(Ordering::Acquire) {
            let work = (|| -> rusqlite::Result<()> {
                let connection = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
                configure_connection(&connection, true)?;
                connection.busy_timeout(Duration::ZERO)?;
                state.ready.store(true, Ordering::Release);
                let mut checked = None;
                let mut last_warning = Instant::now() - Duration::from_secs(60);
                while !state.stop.load(Ordering::Acquire) {
                    let generation = state.generation.load(Ordering::Acquire);
                    if checked != Some(generation) {
                        // PASSIVE does not wait for readers or seize the writer
                        // turn. Retry incomplete work even without a new commit.
                        let (busy, frames, copied): (i64, i64, i64) = connection.query_row(
                            "PRAGMA wal_checkpoint(PASSIVE)", [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                        )?;
                        if busy == 0 && copied == frames {
                            checked = Some(generation);
                            state.completed_generation.store(generation, Ordering::Release);
                        } else {
                            state.incomplete_passes.fetch_add(1, Ordering::Release);
                            if last_warning.elapsed() >= Duration::from_secs(60) {
                                tracing::warn!(busy, wal_frames=frames, checkpointed_frames=copied, "SQLite checkpoint deferred by active work; WAL retained");
                                last_warning = Instant::now();
                            }
                        }
                    }
                    thread::park_timeout(Duration::from_millis(250));
                }
                Ok(())
            })();
            state.ready.store(false, Ordering::Release);
            if let Err(error) = work {
                // Subsequent foreground checkouts restore ordinary automatic
                // checkpoints until the independent worker is ready again.
                tracing::error!(%error, "SQLite background checkpoint failed; automatic checkpoint fallback enabled");
                thread::park_timeout(Duration::from_secs(1));
            }
        }
    })?;
    Ok(Worker { state, thread })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkpoint_policy_never_relaxes_commit_sync_and_has_a_fallback() {
        let c = Connection::open_in_memory().unwrap();
        for (managed, checkpoint) in [(false, 1000), (true, 0), (false, 1000)] {
            configure_connection(&c, managed).unwrap();
            assert_eq!(
                c.query_row("PRAGMA synchronous", [], |r| r.get::<_, i64>(0))
                    .unwrap(),
                2
            );
            assert_eq!(
                c.query_row("PRAGMA wal_autocheckpoint", [], |r| r.get::<_, i64>(0))
                    .unwrap(),
                checkpoint
            );
        }
    }

    #[test]
    fn passive_checkpoint_yields_to_readers_and_retries_without_new_writes() {
        let path = std::env::temp_dir().join(format!("mx-checkpoint-{}.db", uuid::Uuid::new_v4()));
        let c = Connection::open(&path).unwrap();
        c.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE saved(value TEXT);")
            .unwrap();
        configure_connection(&c, true).unwrap();
        c.execute("INSERT INTO saved VALUES('before')", []).unwrap();
        c.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)").unwrap();
        let reader = Connection::open(&path).unwrap();
        reader.execute_batch("BEGIN; SELECT * FROM saved;").unwrap();
        c.execute("INSERT INTO saved VALUES('acknowledged')", [])
            .unwrap();
        let state = Arc::new(State::default());
        let worker = spawn(path.clone(), state.clone()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while state.incomplete_passes.load(Ordering::Acquire) == 0 {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(5));
        }
        // An independent active reader cannot prevent another durable commit.
        c.execute("INSERT INTO saved VALUES('concurrent')", [])
            .unwrap();
        state.generation.fetch_add(1, Ordering::Release);
        reader.execute_batch("ROLLBACK").unwrap();
        drop(reader);
        while state.completed_generation.load(Ordering::Acquire) < 1 {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(
            c.query_row("SELECT count(*) FROM saved", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            3
        );
        state.stop.store(true, Ordering::Release);
        worker.thread.thread().unpark();
        // Keep the join handle owned until it has stopped before removing files.
        while !worker.thread.is_finished() {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(5));
        }
        drop(worker);
        drop(c);
        std::fs::remove_file(path).unwrap();
    }
}
