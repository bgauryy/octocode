use super::*;
use std::sync::atomic::{AtomicBool, Ordering};

fn runtime() -> RequestRuntime {
    RequestRuntime::new(RequestLimits {
        pending: 2,
        ..RequestLimits::default()
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
    let runtime = RequestRuntime::new(RequestLimits {
        timeout: Duration::from_millis(20),
        ..RequestLimits::default()
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
    // The panic and the call after it get the default budget: a loaded host
    // must not turn the 20 ms timeout above into their outcome.
    let runtime = RequestRuntime::new(RequestLimits::default()).expect("valid limits");
    assert_eq!(
        run::<(), _>(&runtime, "panic", |_| panic!("synthetic worker panic")).await,
        Err(ExecutionError::WorkerFailed)
    );
    assert_eq!(runtime.active_requests(), 0);
    assert_eq!(run(&runtime, "ok", |_| Ok(7)).await, Ok(7));
    assert_eq!(runtime.active_requests(), 0);
}

/// M4: a host timeout too large for a deadline is an invalid limit at
/// construction, never a panic when a request is admitted.
#[test]
fn unrepresentable_timeout_is_rejected_not_a_panic_at_admission() {
    for secs in [u64::MAX, u64::MAX / 2, MAX_TIMEOUT.as_secs() + 1] {
        let built = RequestRuntime::new(RequestLimits {
            timeout: Duration::from_secs(secs),
            ..RequestLimits::default()
        });
        assert!(
            matches!(built, Err(ExecutionError::InvalidLimits)),
            "{secs}s must be rejected"
        );
    }
    let longest = RequestRuntime::new(RequestLimits {
        timeout: MAX_TIMEOUT,
        ..RequestLimits::default()
    })
    .expect("the longest timeout is valid");
    assert!(longest.admit("one".into()).is_ok());
}

/// Every admitted request runs at once, up to the pending bound: no run
/// queue makes an independent call wait for another to finish. Each
/// request's work waits until all of them have started, which only
/// completes when none is queued.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_admitted_request_runs_at_once() {
    const CALLS: usize = 16;
    let runtime = RequestRuntime::new(RequestLimits {
        pending: CALLS,
        timeout: Duration::from_secs(20),
    })
    .expect("valid limits");
    let started = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let names = (0..CALLS)
        .map(|call| format!("call-{call}"))
        .collect::<Vec<_>>();
    let calls = names.iter().enumerate().map(|(call, name)| {
        let started = started.clone();
        run(&runtime, name, move |context| {
            started.fetch_add(1, Ordering::SeqCst);
            while started.load(Ordering::SeqCst) < CALLS {
                context.check()?;
                std::thread::yield_now();
            }
            Ok(call)
        })
    });
    let done = futures_util::future::join_all(calls).await;
    assert_eq!(done, (0..CALLS).map(Ok).collect::<Vec<_>>());
    assert_eq!(runtime.active_requests(), 0);
}

/// Admitted work runs on a blocking thread in its caller's notice scope, so
/// a warm server relays what a call's row work raises to that call alone.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn admitted_work_raises_its_notices_in_the_callers_scope() {
    let runtime = runtime();
    let (call, bystander) = (
        octocode_io::notice::Scope::default(),
        octocode_io::notice::Scope::default(),
    );
    call.run(run(&runtime, "noticed", |_| {
        octocode_io::notice::raise("lifecycle-test-row", "lifecycle-test-row: raised");
        Ok(())
    }))
    .await
    .expect("ran");
    let of = |scope: &octocode_io::notice::Scope| {
        scope
            .notices()
            .into_iter()
            .filter(|text| text.starts_with("lifecycle-test-row"))
            .count()
    };
    assert_eq!(of(&call), 1, "the call's own notice");
    assert_eq!(of(&bystander), 0, "never another call's");
}
