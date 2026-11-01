//! The one keyed single-flight primitive: requests for one key take turns
//! on a shared async slot, so the first does the work and later ones reuse
//! what it left in the slot (a remembered answer, a shared failure, or
//! nothing but the ordering).
use std::collections::BTreeMap;
use std::mem::ManuallyDrop;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

/// How often a parked [`Ticket::blocking_turn_until`] rechecks its stop.
const STOP_TICK: Duration = Duration::from_millis(25);

/// Per-key turn-taking over a slot of state `S`.
///
/// A [`KeyedFlight::transient`] flight forgets a key as soon as no request
/// holds or awaits it, so whatever the slot carried is shared only within
/// one concurrent wave. A [`KeyedFlight::retained`] flight keeps idle slots
/// (their state outlives the wave) and drops idle ones once more than `cap`
/// keys are known.
pub struct KeyedFlight<K, S> {
    slots: Mutex<BTreeMap<K, Arc<tokio::sync::Mutex<S>>>>,
    /// `None`: transient. `Some(cap)`: retained, idle slots pruned past `cap`.
    retain: Option<usize>,
}

impl<K, S> KeyedFlight<K, S> {
    pub const fn transient() -> Self {
        Self {
            slots: Mutex::new(BTreeMap::new()),
            retain: None,
        }
    }

    pub const fn retained(cap: usize) -> Self {
        Self {
            slots: Mutex::new(BTreeMap::new()),
            retain: Some(cap),
        }
    }
}

impl<K: Ord + Clone, S: Default> KeyedFlight<K, S> {
    /// Join the flight for `key`; await [`Ticket::turn`] to take its turn.
    pub fn join(&self, key: K) -> Ticket<'_, K, S> {
        let mut slots = self.slots.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(cap) = self.retain
            && slots.len() > cap
        {
            slots.retain(|_, slot| Arc::strong_count(slot) > 1);
        }
        let slot = Arc::clone(slots.entry(key.clone()).or_default());
        Ticket {
            flight: self,
            key,
            slot: ManuallyDrop::new(slot),
        }
    }

    /// Forget idle retained results without splitting a flight still in progress.
    pub fn clear_idle(&self) {
        self.slots
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|_, slot| Arc::strong_count(slot) > 1);
    }

    /// Keys currently known (in flight, or retained): what the tests
    /// observe of forgetting and pruning; no caller needs it.
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

impl<K: Ord + Clone, V: Clone> KeyedFlight<K, Option<V>> {
    /// The value of `key` made once and remembered by the caller's memo:
    /// `lookup` reads the memo, `compute` makes the value and remembers it.
    /// A miss joins `key`'s flight with a blocking turn that ends with
    /// `stop`'s error once it fails ([`Ticket::blocking_turn_until`]). The
    /// turn takes the value a concurrent holder left in the slot, or one
    /// that reached the memo between the lookup and the join, and computes
    /// only when neither has it. For synchronous callers only: the turn
    /// blocks this thread.
    pub fn memoized_blocking<E>(
        &self,
        key: K,
        lookup: impl Fn(&K) -> Option<V>,
        compute: impl FnOnce(K) -> V,
        stop: impl Fn() -> Result<(), E>,
    ) -> Result<V, E> {
        if let Some(hit) = lookup(&key) {
            return Ok(hit);
        }
        let ticket = self.join(key.clone());
        let mut slot = ticket.blocking_turn_until(stop)?;
        if let Some(value) = slot.as_ref() {
            return Ok(value.clone());
        }
        if let Some(hit) = lookup(&key) {
            return Ok(hit);
        }
        let value = compute(key);
        *slot = Some(value.clone());
        Ok(value)
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

    /// [`Self::turn`] for a synchronous caller: parks this thread until the
    /// turn comes ([`Self::blocking_turn_until`] that never stops).
    pub fn blocking_turn(&self) -> tokio::sync::MutexGuard<'_, S> {
        let Ok(guard) = self.blocking_turn_until(|| Ok::<(), std::convert::Infallible>(()));
        guard
    }

    /// [`Self::turn`] for a synchronous caller: parks this thread until the
    /// turn comes, or returns `stop`'s error once it fails (checked every
    /// [`STOP_TICK`] while waiting), so a cancelled waiter does not wait for
    /// the holder. Unlike tokio's `blocking_lock` it never panics on a thread
    /// inside a runtime (a `block_on`, an entered handle). The lock is polled
    /// outside coop budgeting: on a thread whose budget is spent a budgeted
    /// poll stays pending even when nobody holds the turn (parking forever
    /// under `Runtime::block_on`, spinning forever under `Handle::block_on`).
    ///
    /// Invariant: a flight joined with a blocking turn is joined ONLY with
    /// blocking turns, and every holder releases its turn without awaiting.
    /// A parked thread cannot drive its runtime, so an async holder awaiting
    /// on that same (current-thread) runtime would never release the turn:
    /// one flight never mixes the two kinds of turn. (Its users, all held
    /// in synchronous code only: the declarations cache, astSearch's shared
    /// match scans, localFetch's large-source preparation and the
    /// whole-source line redaction, the last two through
    /// [`KeyedFlight::memoized_blocking`].)
    pub fn blocking_turn_until<E>(
        &self,
        stop: impl Fn() -> Result<(), E>,
    ) -> Result<tokio::sync::MutexGuard<'_, S>, E> {
        struct Unpark(std::thread::Thread);
        impl std::task::Wake for Unpark {
            fn wake(self: Arc<Self>) {
                self.0.unpark();
            }
        }
        let waker = std::task::Waker::from(Arc::new(Unpark(std::thread::current())));
        let mut context = std::task::Context::from_waker(&waker);
        let mut lock = std::pin::pin!(tokio::task::coop::unconstrained(self.slot.lock()));
        loop {
            if let std::task::Poll::Ready(guard) =
                std::future::Future::poll(lock.as_mut(), &mut context)
            {
                return Ok(guard);
            }
            stop()?;
            std::thread::park_timeout(STOP_TICK);
        }
    }
}

impl<K: Ord, S> Drop for Ticket<'_, K, S> {
    fn drop(&mut self) {
        // A transient ticket releases its slot inside the map lock, so the
        // release and the idle check are one step: of the last tickets
        // dropping at once, the final one always sees only the map's hold
        // (checking before releasing let each see the other and keep a
        // stale slot for the next wave).
        let slots = self.flight.retain.is_none().then(|| {
            self.flight
                .slots
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
        });
        // SAFETY: `slot` is dropped exactly once, here, and never read again.
        unsafe { ManuallyDrop::drop(&mut self.slot) };
        if let Some(mut slots) = slots
            && slots
                .get(&self.key)
                .is_some_and(|slot| Arc::strong_count(slot) == 1)
        {
            slots.remove(&self.key);
        }
    }
}

#[cfg(test)]
mod tests;
