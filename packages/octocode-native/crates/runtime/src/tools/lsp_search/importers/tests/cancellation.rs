//! The blocking scan worker stops on cancel, deadline and drop.
use super::*;

/// Cancels once `flag` is set, or once `deadline` passes (as a timeout).
struct LiveCancel {
    flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    deadline: Option<std::time::Instant>,
}

impl CancellationCheck for LiveCancel {
    fn check(&self) -> Result<(), String> {
        if self
            .deadline
            .is_some_and(|deadline| std::time::Instant::now() >= deadline)
        {
            return Err("Timeout".into());
        }
        if self.flag.load(std::sync::atomic::Ordering::SeqCst) {
            return Err("Cancelled".into());
        }
        Ok(())
    }
}

/// A worker that reports when it starts, then runs until its stop
/// callback fires (or a safety bound passes), and reports whether it
/// stopped because of the callback.
/// The worker finished and reported `true` within 5 s.
fn assert_worker_stopped(finished: &std::sync::mpsc::Receiver<bool>) {
    assert!(
        finished
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("worker terminated")
    );
}

fn observed_worker(
    started: std::sync::mpsc::Sender<()>,
    finished: std::sync::mpsc::Sender<bool>,
) -> impl FnOnce(&(dyn Fn() -> bool + Sync)) + Send + 'static {
    move |stopped| {
        let _ = started.send(());
        let bound = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !stopped() && std::time::Instant::now() < bound {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let _ = finished.send(stopped());
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_running_worker_stops_when_the_request_is_cancelled_after_launch() {
    let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let cancel = LiveCancel {
        flag: flag.clone(),
        deadline: None,
    };
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (finished_tx, finished_rx) = std::sync::mpsc::channel();
    let canceller = std::thread::spawn(move || {
        started_rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("worker started");
        flag.store(true, std::sync::atomic::Ordering::SeqCst);
    });
    let failure = blocking_cancellable(&cancel, observed_worker(started_tx, finished_tx))
        .await
        .expect_err("cancelled after launch");
    assert_eq!(failure.code, ErrorCode::Cancelled);
    assert_eq!(failure.message, "Cancelled");
    canceller.join().expect("canceller");
    assert!(
        finished_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("worker terminated"),
        "the worker must stop through its stop callback"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_deadline_expiring_during_the_scan_stops_the_worker_as_a_timeout() {
    let cancel = LiveCancel {
        flag: std::sync::Arc::default(),
        deadline: Some(std::time::Instant::now() + std::time::Duration::from_millis(150)),
    };
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (finished_tx, finished_rx) = std::sync::mpsc::channel();
    let failure = blocking_cancellable(&cancel, observed_worker(started_tx, finished_tx))
        .await
        .expect_err("deadline expired");
    started_rx.try_recv().expect("worker had started");
    assert_eq!(failure.code, ErrorCode::Cancelled);
    assert_eq!(failure.message, "Timeout");
    assert_worker_stopped(&finished_rx);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dropping_the_request_stops_the_worker() {
    let (started_tx, _started_rx) = std::sync::mpsc::channel();
    let (finished_tx, finished_rx) = std::sync::mpsc::channel();
    let dropped = tokio::time::timeout(
        std::time::Duration::from_millis(50),
        blocking_cancellable(
            &crate::execution::cancel::NeverCancel,
            observed_worker(started_tx, finished_tx),
        ),
    )
    .await;
    assert!(dropped.is_err(), "request future dropped by the timeout");
    assert_worker_stopped(&finished_rx);
}

/// The real importer scan: cancellation observed after the request
/// started is a cancellation, never a completed candidate list.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_candidate_scan_reports_cancellation_after_launch() {
    struct CancelAfterFirst(std::sync::atomic::AtomicUsize);
    impl CancellationCheck for CancelAfterFirst {
        fn check(&self) -> Result<(), String> {
            if self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                Ok(())
            } else {
                Err("Cancelled".into())
            }
        }
    }
    let root = tempfile::tempdir().expect("fixture");
    for index in 0..200 {
        std::fs::write(
            root.path().join(format!("f{index}.ts")),
            "import { target } from './a';\ntarget();\n",
        )
        .expect("fixture file");
    }
    let root_str = root.path().to_string_lossy().into_owned();
    let policy = |workspace: &Path| crate::tools::test_support::workspace_policy(workspace);
    let scope = Scope::new(root_str.clone(), vec!["*.ts".into()]);
    let failure = candidate_files(
        &scope,
        "target",
        &HashSet::new(),
        &policy(root.path()),
        &CancelAfterFirst(std::sync::atomic::AtomicUsize::new(0)),
    )
    .await
    .expect_err("cancellation is not an empty or complete scan");
    assert_eq!(failure.code, ErrorCode::Cancelled);
    let scope = Scope::new(root_str, vec!["*.ts".into()]);
    let complete = candidate_files(
        &scope,
        "target",
        &HashSet::new(),
        &policy(root.path()),
        &crate::execution::cancel::NeverCancel,
    )
    .await
    .expect("scan")
    .expect("scan succeeded");
    assert_eq!(complete.len(), 200, "every candidate, uncut");
    let Cut::Window(window) = Window::cut(complete, WindowRequest::default()) else {
        panic!("first window");
    };
    assert_eq!(window.files().len(), MAX_CANDIDATE_FILES);
    assert!(window.more(), "more candidates than one window");
}
