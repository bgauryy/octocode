//! One pooled `reqwest::Client` per Tokio runtime and key.
//!
//! hyper drives each pooled connection from a task spawned on the runtime that
//! opened it, so once that runtime shuts down (every `#[tokio::test]`) its
//! pooled connections are dead and reuse fails with "dispatch task is gone".
//! Keying by [`tokio::runtime::Id`] keeps pooling across threads and calls
//! within one runtime while never sharing connections across runtimes. The list is small and LRU-evicted, so
//! short-lived runtimes cannot grow it.

use std::sync::{Mutex, MutexGuard, PoisonError};
use tokio::runtime::{Handle, Id};

pub struct RuntimeClients<K> {
    clients: Mutex<Vec<(Id, K, reqwest::Client)>>,
    capacity: usize,
}

impl<K: PartialEq> RuntimeClients<K> {
    pub const fn new(capacity: usize) -> Self {
        Self {
            clients: Mutex::new(Vec::new()),
            capacity,
        }
    }

    /// The client for `key` on the current runtime, built on a miss. Outside
    /// a runtime no connection outlives the call, so each call builds its own.
    /// The build runs outside the lock (it loads TLS roots and the system
    /// proxy); when two callers race on one key, the first insert wins.
    pub fn get<E>(
        &self,
        key: K,
        build: impl FnOnce() -> Result<reqwest::Client, E>,
    ) -> Result<reqwest::Client, E> {
        let Ok(runtime) = Handle::try_current() else {
            return build();
        };
        let id = runtime.id();
        if let Some(client) = Self::cached(&mut self.lock(), id, &key) {
            return Ok(client);
        }
        let client = build()?;
        let mut clients = self.lock();
        if let Some(raced) = Self::cached(&mut clients, id, &key) {
            return Ok(raced);
        }
        if clients.len() >= self.capacity {
            clients.remove(0);
        }
        clients.push((id, key, client.clone()));
        Ok(client)
    }

    fn lock(&self) -> MutexGuard<'_, Vec<(Id, K, reqwest::Client)>> {
        self.clients.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The client cached for `(id, key)`, marked most recently used.
    fn cached(
        clients: &mut Vec<(Id, K, reqwest::Client)>,
        id: Id,
        key: &K,
    ) -> Option<reqwest::Client> {
        let index = clients
            .iter()
            .position(|(cached, cached_key, _)| *cached == id && cached_key == key)?;
        let entry = clients.remove(index);
        let client = entry.2.clone();
        clients.push(entry);
        Some(client)
    }
}

#[cfg(test)]
mod tests;
