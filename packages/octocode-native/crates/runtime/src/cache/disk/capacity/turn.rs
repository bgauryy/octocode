//! Process-local ordering for writers of one disk store. Only holders and
//! waiters retain a slot; an idle store leaves no registry entry.
use super::{CANCEL_SLICE, Path, PathBuf};
use std::collections::HashMap;
use std::sync::{Arc, Condvar, LazyLock, Mutex, PoisonError};

type Registry = HashMap<PathBuf, Arc<Turn>>;
static TURNS: LazyLock<Mutex<Registry>> = LazyLock::new(Default::default);

#[derive(Default)]
pub(super) struct Turn {
    taken: Mutex<bool>,
    freed: Condvar,
}

pub(super) struct Ticket {
    path: PathBuf,
    turn: Option<Arc<Turn>>,
}

/// Holds its ticket through the entire store write, after the file lock drops.
pub(in crate::cache) struct TurnHeld(Ticket);

impl Drop for TurnHeld {
    fn drop(&mut self) {
        if let Some(turn) = &self.0.turn {
            *turn.taken.lock().unwrap_or_else(PoisonError::into_inner) = false;
            turn.freed.notify_one();
        }
    }
}

impl Turn {
    pub(super) fn of(dir: &Path) -> Ticket {
        let mut turns = TURNS.lock().unwrap_or_else(PoisonError::into_inner);
        Ticket {
            path: dir.to_path_buf(),
            turn: Some(Arc::clone(turns.entry(dir.to_path_buf()).or_default())),
        }
    }
}

impl Ticket {
    /// Wait for the previous writer, checking cancellation at each wakeup.
    pub(super) fn take(self, stopped: impl Fn() -> bool) -> Option<TurnHeld> {
        let turn = self.turn.as_ref()?;
        let mut taken = turn.taken.lock().unwrap_or_else(PoisonError::into_inner);
        while *taken {
            if stopped() {
                return None;
            }
            taken = turn
                .freed
                .wait_timeout(taken, CANCEL_SLICE)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
        *taken = true;
        drop(taken);
        Some(TurnHeld(self))
    }
}

impl Drop for Ticket {
    fn drop(&mut self) {
        let mut turns = TURNS.lock().unwrap_or_else(PoisonError::into_inner);
        // Release and check under the same lock: simultaneous final waiters
        // must not each see the other's reference and leave an idle slot.
        drop(self.turn.take());
        if turns
            .get(&self.path)
            .is_some_and(|turn| Arc::strong_count(turn) == 1)
        {
            turns.remove(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
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
}
