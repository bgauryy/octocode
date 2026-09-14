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

struct AsyncResource(Arc<AtomicBool>);
impl Drop for AsyncResource {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn async_cancellation_drops_owned_resources_before_close_returns() {
    let runtime = runtime();
    let entered = Arc::new(Notify::new());
    let cleaned = Arc::new(AtomicBool::new(false));
    let job = {
        let runtime = runtime.clone();
        let entered = entered.clone();
        let cleaned = cleaned.clone();
        tokio::spawn(async move {
            runtime
                .execute_async("network".into(), move |_| async move {
                    let _resource = AsyncResource(cleaned);
                    entered.notify_one();
                    std::future::pending::<Result<(), ExecutionError>>().await
                })
                .await
        })
    };
    entered.notified().await;
    assert_eq!(
        runtime.execute_blocking("network".into(), |_| Ok(())).await,
        Err(ExecutionError::DuplicateRequest)
    );
    runtime.close().await;
    assert!(cleaned.load(Ordering::SeqCst));
    assert_eq!(runtime.active_requests(), 0);
    assert_eq!(job.await.expect("join"), Err(ExecutionError::Cancelled));
}

#[tokio::test]
async fn async_timeout_and_caller_drop_release_admission() {
    let runtime = RequestRuntime::new(RuntimeLimits {
        timeout: Duration::from_millis(10),
        ..RuntimeLimits::default()
    })
    .expect("limits");
    assert_eq!(
        runtime
            .execute_async("timeout".into(), |_| std::future::pending::<
                Result<(), ExecutionError>,
            >())
            .await,
        Err(ExecutionError::Timeout)
    );
    assert_eq!(runtime.active_requests(), 0);
    let mut work = Box::pin(runtime.execute_async("dropped".into(), |_| {
        std::future::pending::<Result<(), ExecutionError>>()
    }));
    assert!(futures_util::poll!(&mut work).is_pending());
    assert_eq!(runtime.active_requests(), 1);
    drop(work);
    assert_eq!(runtime.active_requests(), 0);
    assert_eq!(
        runtime
            .execute_async("next".into(), |_| async { Ok(7) })
            .await,
        Ok(7)
    );
}

#[tokio::test]
async fn async_panic_is_typed_and_releases_admission() {
    let runtime = runtime();
    let result = runtime
        .execute_async::<(), _, _>("panic".into(), |_| async {
            panic!("synthetic async panic")
        })
        .await;
    assert_eq!(result, Err(ExecutionError::WorkerFailed));
    assert_eq!(runtime.active_requests(), 0);
    assert_eq!(
        runtime
            .execute_async("next".into(), |_| async { Ok(1) })
            .await,
        Ok(1)
    );
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
