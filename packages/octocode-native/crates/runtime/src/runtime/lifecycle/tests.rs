use super::*;
use std::sync::atomic::{AtomicBool, Ordering};

fn runtime() -> RequestRuntime {
    RequestRuntime::new(RuntimeLimits {
        concurrency: 1,
        pending: 2,
        ..RuntimeLimits::default()
    })
    .expect("valid limits")
}

#[tokio::test]
async fn admission_is_cancellable_before_first_poll_and_bounds_unstarted_work() {
    let runtime = runtime();
    let admission = runtime.admit("early".into()).expect("admitted");
    assert!(runtime.cancel("early"));
    let entered = Arc::new(AtomicBool::new(false));
    let marker = entered.clone();
    assert_eq!(
        runtime
            .execute_blocking_admitted(admission, move |_| {
                marker.store(true, Ordering::SeqCst);
                Ok(())
            })
            .await,
        Err(ExecutionError::Cancelled)
    );
    assert!(!entered.load(Ordering::SeqCst));
    assert_eq!(runtime.active_requests(), 0);
    let first = runtime.admit("one".into()).expect("one");
    let second = runtime.admit("two".into()).expect("two");
    assert!(matches!(
        runtime.admit("three".into()),
        Err(ExecutionError::Busy)
    ));
    drop(first);
    drop(second);
    assert_eq!(runtime.active_requests(), 0);
}

/// Mirrors the production entry: admission is synchronous, then the admitted
/// work runs through `execute_blocking_admitted` (see `ToolRuntime::execute`).
async fn run<T, F>(runtime: &RequestRuntime, request_id: &str, work: F) -> Result<T, ExecutionError>
where
    T: Send + 'static,
    F: FnOnce(ExecutionContext) -> Result<T, ExecutionError> + Send + 'static,
{
    let admission = runtime.admit(request_id.into())?;
    runtime.execute_blocking_admitted(admission, work).await
}

fn spin_until_stopped(context: &ExecutionContext) {
    while context.check().is_ok() {
        std::thread::yield_now();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn caller_drop_while_queued_releases_admission_without_running_work() {
    let runtime = runtime();
    let entered = Arc::new(Notify::new());
    let running = {
        let runtime = runtime.clone();
        let entered = entered.clone();
        tokio::spawn(async move {
            run(&runtime, "running", move |context| {
                entered.notify_one();
                spin_until_stopped(&context);
                Ok(())
            })
            .await
        })
    };
    entered.notified().await;
    let admission = runtime.admit("dropped".into()).expect("admitted");
    let mut work = Box::pin(
        runtime.execute_blocking_admitted(admission, |_| -> Result<(), _> {
            panic!("dropped caller must not execute")
        }),
    );
    assert!(futures_util::poll!(&mut work).is_pending());
    assert_eq!(runtime.active_requests(), 2);
    drop(work);
    assert_eq!(runtime.active_requests(), 1);
    assert!(runtime.cancel("running"));
    assert_eq!(running.await.expect("join"), Err(ExecutionError::Cancelled));
    assert_eq!(run(&runtime, "next", |_| Ok(7)).await, Ok(7));
    assert_eq!(runtime.active_requests(), 0);
}

#[tokio::test]
async fn admission_from_another_runtime_is_rejected() {
    let owner = runtime();
    let other = runtime();
    let admission = owner.admit("foreign".into()).expect("admitted");
    assert_eq!(
        other.execute_blocking_admitted(admission, |_| Ok(())).await,
        Err(ExecutionError::WorkerFailed)
    );
    assert_eq!(owner.active_requests(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn close_joins_workers_and_rejects_future_requests() {
    let runtime = runtime();
    let entered = Arc::new(Notify::new());
    let cleaned = Arc::new(AtomicBool::new(false));
    let job = {
        let runtime = runtime.clone();
        let entered = entered.clone();
        let cleaned = cleaned.clone();
        tokio::spawn(async move {
            run(&runtime, "one", move |context| {
                entered.notify_one();
                spin_until_stopped(&context);
                cleaned.store(true, Ordering::SeqCst);
                Ok(())
            })
            .await
        })
    };
    entered.notified().await;
    assert!(matches!(
        runtime.admit("one".into()),
        Err(ExecutionError::DuplicateRequest)
    ));
    runtime.close().await;
    assert!(cleaned.load(Ordering::SeqCst));
    assert_eq!(runtime.active_requests(), 0);
    assert_eq!(job.await.expect("join"), Err(ExecutionError::Cancelled));
    assert!(matches!(
        runtime.admit("later".into()),
        Err(ExecutionError::Closed)
    ));
    runtime.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dropping_caller_cancels_and_releases_blocking_work() {
    let runtime = runtime();
    let entered = Arc::new(Notify::new());
    let finished = Arc::new(Notify::new());
    let job = {
        let runtime = runtime.clone();
        let entered = entered.clone();
        let finished = finished.clone();
        tokio::spawn(async move {
            run(&runtime, "drop", move |context| {
                entered.notify_one();
                spin_until_stopped(&context);
                finished.notify_one();
                Ok(())
            })
            .await
        })
    };
    entered.notified().await;
    job.abort();
    let _ = job.await;
    tokio::time::timeout(Duration::from_secs(1), finished.notified())
        .await
        .expect("dropped caller must cancel");
    runtime.close().await;
    assert_eq!(runtime.active_requests(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn timeout_is_observed_before_return_and_worker_panic_releases_slot() {
    let runtime = RequestRuntime::new(RuntimeLimits {
        timeout: Duration::from_millis(20),
        ..RuntimeLimits::default()
    })
    .expect("valid limits");
    assert_eq!(
        run(&runtime, "timeout", |context| {
            spin_until_stopped(&context);
            Ok(())
        })
        .await,
        Err(ExecutionError::Timeout)
    );
    assert_eq!(runtime.active_requests(), 0);
    assert_eq!(
        run::<(), _>(&runtime, "panic", |_| panic!("synthetic worker panic")).await,
        Err(ExecutionError::WorkerFailed)
    );
    assert_eq!(runtime.active_requests(), 0);
    assert_eq!(run(&runtime, "ok", |_| Ok(7)).await, Ok(7));
    assert_eq!(runtime.active_requests(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn queue_is_bounded_and_queued_cancellation_never_runs_work() {
    let runtime = runtime();
    let entered = Arc::new(Notify::new());
    let first = {
        let runtime = runtime.clone();
        let entered = entered.clone();
        tokio::spawn(async move {
            run(&runtime, "running", move |context| {
                entered.notify_one();
                spin_until_stopped(&context);
                Ok(())
            })
            .await
        })
    };
    entered.notified().await;
    let queued = {
        let runtime = runtime.clone();
        tokio::spawn(async move {
            run(&runtime, "queued", |_| -> Result<(), _> {
                panic!("cancelled queue must not execute")
            })
            .await
        })
    };
    tokio::time::timeout(Duration::from_secs(1), async {
        while runtime.active_requests() != 2 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("queued registration");
    assert!(matches!(
        runtime.admit("overflow".into()),
        Err(ExecutionError::Busy)
    ));
    assert!(runtime.cancel("queued"));
    assert_eq!(queued.await.expect("join"), Err(ExecutionError::Cancelled));
    assert!(runtime.cancel("running"));
    assert_eq!(first.await.expect("join"), Err(ExecutionError::Cancelled));
    runtime.close().await;
}
