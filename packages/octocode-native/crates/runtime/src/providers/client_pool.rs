//! One pooled `reqwest::Client` per Tokio runtime and key.
//!
//! hyper drives each pooled connection from a task spawned on the runtime that
//! opened it, so once that runtime shuts down (every `#[tokio::test]`, or a
//! closed N-API `NativeRuntime`) its pooled connections are dead and reuse
//! fails with "dispatch task is gone". Keying by [`tokio::runtime::Id`] keeps
//! pooling across threads and calls within one runtime while never sharing
//! connections across runtimes. The list is small and LRU-evicted, so
//! short-lived runtimes cannot grow it.

use std::sync::{Mutex, PoisonError};
use tokio::runtime::{Handle, Id};

pub(crate) struct RuntimeClients<K> {
    clients: Mutex<Vec<(Id, K, reqwest::Client)>>,
    capacity: usize,
}

impl<K: PartialEq> RuntimeClients<K> {
    pub(crate) const fn new(capacity: usize) -> Self {
        Self {
            clients: Mutex::new(Vec::new()),
            capacity,
        }
    }

    /// The client for `key` on the current runtime, built on a miss. Outside
    /// a runtime no connection outlives the call, so each call builds its own.
    pub(crate) fn get<E>(
        &self,
        key: K,
        build: impl FnOnce() -> Result<reqwest::Client, E>,
    ) -> Result<reqwest::Client, E> {
        let Ok(runtime) = Handle::try_current() else {
            return build();
        };
        let id = runtime.id();
        let mut clients = self.clients.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(index) = clients
            .iter()
            .position(|(cached, cached_key, _)| *cached == id && *cached_key == key)
        {
            let entry = clients.remove(index);
            let client = entry.2.clone();
            clients.push(entry);
            return Ok(client);
        }
        let client = build()?;
        if clients.len() >= self.capacity {
            clients.remove(0);
        }
        clients.push((id, key, client.clone()));
        Ok(client)
    }
}

#[cfg(test)]
mod tests {
    use super::RuntimeClients;
    use std::cell::Cell;
    use std::convert::Infallible;

    fn counted(builds: &Cell<usize>) -> impl FnOnce() -> Result<reqwest::Client, Infallible> + '_ {
        || {
            builds.set(builds.get() + 1);
            Ok(reqwest::Client::new())
        }
    }

    fn on_runtime(clients: &RuntimeClients<u8>, builds: &Cell<usize>, keys: &[u8]) {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("runtime");
        let _guard = runtime.enter();
        for key in keys {
            clients.get(*key, counted(builds)).expect("client");
        }
    }

    #[test]
    fn a_runtime_reuses_its_client_and_a_new_runtime_builds_its_own() {
        let clients = RuntimeClients::new(8);
        let builds = Cell::new(0);
        on_runtime(&clients, &builds, &[1, 1, 1]);
        assert_eq!(builds.get(), 1, "one build per key on one runtime");
        on_runtime(&clients, &builds, &[1, 2, 2]);
        assert_eq!(builds.get(), 3, "a new runtime never reuses a dead pool");
    }

    #[test]
    fn outside_a_runtime_every_call_builds() {
        let clients = RuntimeClients::new(8);
        let builds = Cell::new(0);
        for _ in 0..2 {
            clients.get(1, counted(&builds)).expect("client");
        }
        assert_eq!(builds.get(), 2);
    }

    #[test]
    fn the_oldest_client_is_evicted_past_capacity() {
        let clients = RuntimeClients::new(2);
        let builds = Cell::new(0);
        on_runtime(&clients, &builds, &[1, 2, 3, 2, 1]);
        assert_eq!(builds.get(), 4, "key 1 was evicted by key 3");
    }
}
