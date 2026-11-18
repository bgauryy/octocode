use super::*;

#[test]
fn idle_store_turns_are_not_retained() {
    let root = tempfile::tempdir().unwrap();
    for index in 0..256 {
        let path = root.path().join(index.to_string());
        let ticket = Turn::of(&path);
        let weak = Arc::downgrade(ticket.turn.as_ref().unwrap());
        drop(ticket.take(|| false).unwrap());
        assert!(weak.upgrade().is_none(), "idle slot must be freed");
        assert!(!TURNS.lock().unwrap().contains_key(&path));
    }
}

#[test]
fn cancellation_and_concurrent_ticket_drops_free_the_slot() {
    let root = tempfile::tempdir().unwrap();
    let held = Turn::of(root.path()).take(|| false).unwrap();
    let cancelled = Turn::of(root.path());
    assert!(cancelled.take(|| true).is_none());
    let a = Turn::of(root.path());
    let b = Turn::of(root.path());
    let weak = Arc::downgrade(a.turn.as_ref().unwrap());
    drop(held);
    let barrier = std::sync::Barrier::new(2);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            barrier.wait();
            drop(a);
        });
        scope.spawn(|| {
            barrier.wait();
            drop(b);
        });
    });
    assert!(weak.upgrade().is_none());
    assert!(!TURNS.lock().unwrap().contains_key(root.path()));
}

#[test]
fn waiting_and_arriving_writers_share_one_turn() {
    let root = tempfile::tempdir().unwrap();
    let held = Turn::of(root.path()).take(|| false).unwrap();
    let waiter = Turn::of(root.path());
    drop(held);
    // A waiting ticket must keep the slot alive across the holder's
    // release, before a third writer discovers that directory.
    let arriving = Turn::of(root.path());
    assert!(Arc::ptr_eq(
        waiter.turn.as_ref().unwrap(),
        arriving.turn.as_ref().unwrap()
    ));
    let held = arriving.take(|| false).unwrap();
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::scope(|scope| {
        scope.spawn(move || {
            let _held = waiter.take(|| false).unwrap();
            sender.send(()).unwrap();
        });
        assert!(receiver.try_recv().is_err(), "holder serializes waiters");
        drop(held);
        receiver.recv().unwrap();
    });
    assert!(!TURNS.lock().unwrap().contains_key(root.path()));
}
