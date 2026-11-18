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
mod tests;
