//! The one keyed single-flight primitive: requests for one key take turns
//! on a shared async slot, so the first does the work and later ones reuse
//! what it left in the slot (a remembered answer, a shared failure, or
//! nothing but the ordering).
use std::collections::BTreeMap;
use std::mem::ManuallyDrop;
use std::sync::{Arc, Mutex, PoisonError};

/// Per-key turn-taking over a slot of state `S`.
///
/// The flight is transient: it forgets a key as soon as no request holds or
/// awaits it, so whatever the slot carried is shared only within one
/// concurrent wave.
pub struct KeyedFlight<K, S> {
    slots: Mutex<BTreeMap<K, Arc<tokio::sync::Mutex<S>>>>,
}

impl<K, S> KeyedFlight<K, S> {
    pub const fn transient() -> Self {
        Self {
            slots: Mutex::new(BTreeMap::new()),
        }
    }
}

impl<K: Ord + Clone, S: Default> KeyedFlight<K, S> {
    /// Join the flight for `key`; await [`Ticket::turn`] to take its turn.
    pub fn join(&self, key: K) -> Ticket<'_, K, S> {
        let mut slots = self.slots.lock().unwrap_or_else(PoisonError::into_inner);
        let slot = Arc::clone(slots.entry(key.clone()).or_default());
        Ticket {
            flight: self,
            key,
            slot: ManuallyDrop::new(slot),
        }
    }

    /// Keys currently in flight: what the tests observe of forgetting; no
    /// caller needs it.
    #[cfg(test)]
    fn len(&self) -> usize {
        self.slots
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }

    #[cfg(test)]
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// One request's place in a key's flight. Dropping it (on completion,
/// cancellation, or panic) forgets a transient key nobody else holds.
pub struct Ticket<'a, K: Ord, S> {
    flight: &'a KeyedFlight<K, S>,
    key: K,
    /// Released by [`Drop`] inside the map lock, never after it.
    slot: ManuallyDrop<Arc<tokio::sync::Mutex<S>>>,
}

impl<K: Ord, S> Ticket<'_, K, S> {
    /// Wait for this request's turn and the slot's state.
    pub async fn turn(&self) -> tokio::sync::MutexGuard<'_, S> {
        self.slot.lock().await
    }
}

impl<K: Ord, S> Drop for Ticket<'_, K, S> {
    fn drop(&mut self) {
        // A ticket releases its slot inside the map lock, so the release
        // and the idle check are one step: of the last tickets dropping at
        // once, the final one always sees only the map's hold (checking
        // before releasing let each see the other and keep a stale slot for
        // the next wave).
        let mut slots = self
            .flight
            .slots
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        // SAFETY: `slot` is dropped exactly once, here, and never read again.
        unsafe { ManuallyDrop::drop(&mut self.slot) };
        if slots
            .get(&self.key)
            .is_some_and(|slot| Arc::strong_count(slot) == 1)
        {
            slots.remove(&self.key);
        }
    }
}

#[cfg(test)]
mod tests;
