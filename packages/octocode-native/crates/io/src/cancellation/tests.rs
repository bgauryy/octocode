use super::*;

#[test]
fn a_cancel_through_any_clone_reaches_every_check() {
    let signal = Cancellation::new();
    let worker = signal.clone();
    assert!(!worker.is_cancelled());
    signal.clone().cancel();
    assert!(signal.is_cancelled() && worker.is_cancelled());
}

#[tokio::test]
async fn an_await_wakes_when_another_task_cancels() {
    let signal = Cancellation::new();
    let waiting = tokio::spawn({
        let signal = signal.clone();
        async move {
            signal.cancelled().await;
            signal.is_cancelled()
        }
    });
    tokio::task::yield_now().await;
    signal.cancel();
    assert!(waiting.await.expect("the waiter ran"));
}

#[test]
fn a_drop_guard_cancels_when_dropped() {
    let signal = Cancellation::new();
    let guard = signal.clone().drop_guard();
    assert!(!signal.is_cancelled());
    drop(guard);
    assert!(signal.is_cancelled());
}
