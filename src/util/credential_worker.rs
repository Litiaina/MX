//! Password verification is intentionally memory-hard. Keep it off the shared
//! blocking pool: a login burst otherwise spreads large allocator arenas over
//! hundreds of blocked SQL workers, retaining far more RAM than active hashes.
use std::sync::{Arc, LazyLock, Mutex};
use tokio::sync::{mpsc, oneshot};

const WORKERS: usize = 8;
type Work = Box<dyn FnOnce() + Send + 'static>;
static QUEUE: LazyLock<mpsc::Sender<Work>> = LazyLock::new(|| {
    let (send, receive) = mpsc::channel::<Work>(128);
    let receive = Arc::new(Mutex::new(receive));
    for index in 0..WORKERS {
        let receive = receive.clone();
        std::thread::Builder::new()
            .name(format!("mx-password-{index}"))
            .spawn(move || {
                loop {
                    let work = receive
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .blocking_recv();
                    let Some(work) = work else {
                        break;
                    };
                    // Release the receiver lock before executing CPU/SQL work. In debug
                    // builds a failed task must not silently remove a worker forever.
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work));
                }
            })
            .expect("create bounded MX password worker");
    }
    send
});

pub(crate) async fn run<T: Send + 'static>(
    operation: impl FnOnce() -> T + Send + 'static,
) -> Result<T, &'static str> {
    let (send, receive) = oneshot::channel();
    QUEUE
        .send(Box::new(move || {
            if !send.is_closed() {
                let _ = send.send(operation());
            }
        }))
        .await
        .map_err(|_| "password worker queue unavailable")?;
    receive.await.map_err(|_| "password task did not complete")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::BTreeSet,
        sync::atomic::{AtomicUsize, Ordering},
        time::Duration,
    };
    #[tokio::test]
    async fn login_bursts_use_a_fixed_number_of_named_workers() {
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let jobs = (0..100).map(|_| {
            let active = active.clone();
            let peak = peak.clone();
            run(move || {
                let n = active.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(n, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(2));
                active.fetch_sub(1, Ordering::SeqCst);
                std::thread::current().name().unwrap().to_owned()
            })
        });
        let threads = futures::future::join_all(jobs)
            .await
            .into_iter()
            .map(Result::unwrap)
            .collect::<BTreeSet<_>>();
        assert!(threads.len() <= WORKERS);
        assert!(threads.iter().all(|n| n.starts_with("mx-password-")));
        assert!(peak.load(Ordering::SeqCst) <= WORKERS);
        assert_eq!(active.load(Ordering::SeqCst), 0);
    }
}
