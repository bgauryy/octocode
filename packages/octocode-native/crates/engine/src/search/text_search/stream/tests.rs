thread_local! {
    /// The share a test gives the stream it starts on this thread.
    pub(in crate::search::text_search) static SHARE: std::cell::Cell<Option<usize>> =
        const { std::cell::Cell::new(None) };
}

use super::*;
use std::sync::{Condvar, Mutex};

/// Workers idle because the window is full (the head is slow) widen it,
/// once until the walker hands out more, and not past what was committed.
#[test]
fn workers_idle_behind_a_full_window_widen_it_once_per_hand_out() {
    let commit = Mutex::new(Commit {
        next: 24,
        handed: 32,
        window: 8,
        base: 8,
        ..Commit::empty()
    });
    let (committed, work) = (Condvar::new(), Condvar::new());
    let state = CollectState::new();
    let window = || commit.lock().expect("commit").window;
    std::thread::scope(|scope| {
        let idle = (0..3)
            .map(|_| scope.spawn(|| take_candidate((&commit, &committed, &work), &state)))
            .collect::<Vec<_>>();
        while window() == 8 {
            std::thread::yield_now();
        }
        // The other idle workers wait: one widening per hand-out.
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert_eq!(window(), 16);
        // The walker hands out: the next stall widens again, to what
        // was committed and no further.
        {
            let mut ahead = commit.lock().expect("commit");
            ahead.handed = ahead.next + 16;
            ahead.widened = false;
        }
        work.notify_all();
        while window() == 16 {
            std::thread::yield_now();
        }
        assert_eq!(window(), 24);
        commit.lock().expect("commit").walk_ended = true;
        work.notify_all();
        for worker in idle {
            assert!(worker.join().expect("worker").is_none());
        }
    });
}

/// One slow file at the head can outlast the time the workers need for
/// any fixed window: a stream that read far widens past it, up to what it
/// committed, so the workers keep searching behind the slow file.
#[test]
fn a_stream_that_read_far_widens_without_a_fixed_ceiling() {
    let commit = Mutex::new(Commit {
        next: 5000,
        handed: 5000 + 768,
        window: 768,
        base: 24,
        ..Commit::empty()
    });
    let (committed, work) = (Condvar::new(), Condvar::new());
    let state = CollectState::new();
    std::thread::scope(|scope| {
        let idle = scope.spawn(|| take_candidate((&commit, &committed, &work), &state));
        while commit.lock().expect("commit").window == 768 {
            std::thread::yield_now();
        }
        assert_eq!(commit.lock().expect("commit").window, 1536);
        commit.lock().expect("commit").walk_ended = true;
        work.notify_all();
        assert!(idle.join().expect("worker").is_none());
    });
}

/// A stream that has read little widens nothing: its window stays the
/// start, so a page filled early reads no further past its end.
#[test]
fn a_stream_that_read_little_keeps_its_window() {
    let commit = Mutex::new(Commit {
        next: 4,
        handed: 12,
        window: 8,
        base: 8,
        ..Commit::empty()
    });
    let (committed, work) = (Condvar::new(), Condvar::new());
    let state = CollectState::new();
    std::thread::scope(|scope| {
        let idle = scope.spawn(|| take_candidate((&commit, &committed, &work), &state));
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert_eq!(commit.lock().expect("commit").window, 8);
        commit.lock().expect("commit").walk_ended = true;
        work.notify_all();
        assert!(idle.join().expect("worker").is_none());
    });
}

/// Run `f` with the stream it starts holding `share` cores.
pub(in crate::search::text_search) fn with_share<R>(share: usize, f: impl FnOnce() -> R) -> R {
    SHARE.with(|cell| cell.set(Some(share)));
    let result = f();
    SHARE.with(|cell| cell.set(None));
    result
}
