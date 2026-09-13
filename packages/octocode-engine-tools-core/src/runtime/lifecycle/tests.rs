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
            runtime
                .execute_blocking("one".into(), move |context| {
                    entered.notify_one();
                    while context.check().is_ok() {
                        std::thread::yield_now();
                    }
                    cleaned.store(true, Ordering::SeqCst);
                    Ok(())
                })
                .await
        })
    };
    entered.notified().await;
    assert_eq!(
        runtime.execute_blocking("one".into(), |_| Ok(())).await,
        Err(ExecutionError::DuplicateRequest)
    );
    runtime.close().await;
    assert!(cleaned.load(Ordering::SeqCst));
    assert_eq!(runtime.active_requests(), 0);
    assert_eq!(job.await.expect("join"), Err(ExecutionError::Cancelled));
    assert_eq!(
        runtime.execute_blocking("later".into(), |_| Ok(())).await,
        Err(ExecutionError::Closed)
    );
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
            runtime
                .execute_blocking("drop".into(), move |context| {
                    entered.notify_one();
                    while context.check().is_ok() {
                        std::thread::yield_now();
                    }
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
        runtime
            .execute_blocking("timeout".into(), |context| {
                while context.check().is_ok() {
                    std::thread::yield_now();
                }
                Ok(())
            })
            .await,
        Err(ExecutionError::Timeout)
    );
    assert_eq!(runtime.active_requests(), 0);
    assert_eq!(
        runtime
            .execute_blocking::<(), _>("panic".into(), |_| panic!("synthetic worker panic"))
            .await,
        Err(ExecutionError::WorkerFailed)
    );
    assert_eq!(
        runtime.execute_blocking("ok".into(), |_| Ok(7)).await,
        Ok(7)
    );
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
            runtime
                .execute_blocking("running".into(), move |context| {
                    entered.notify_one();
                    while context.check().is_ok() {
                        std::thread::yield_now();
                    }
                    Ok(())
                })
                .await
        })
    };
    entered.notified().await;
    let queued = {
        let runtime = runtime.clone();
        tokio::spawn(async move {
            runtime
                .execute_blocking("queued".into(), |_| {
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
    assert_eq!(
        runtime
            .execute_blocking("overflow".into(), |_| Ok(()))
            .await,
        Err(ExecutionError::Busy)
    );
    assert!(runtime.cancel("queued"));
    assert_eq!(
        queued.await.expect("join"),
        Err::<(), _>(ExecutionError::Cancelled)
    );
    assert!(runtime.cancel("running"));
    assert_eq!(first.await.expect("join"), Err(ExecutionError::Cancelled));
    runtime.close().await;
}
