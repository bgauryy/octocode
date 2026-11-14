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
    /// The build runs outside the lock (it loads TLS roots and the system
    /// proxy); when two callers race on one key, the first insert wins.
    pub(crate) fn get<E>(
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
mod tests {
    use super::RuntimeClients;
    use std::cell::Cell;
    use std::convert::Infallible;

    /// A client that skips the system proxy lookup: these tests count and
    /// order builds, and that lookup is a real system service (on macOS a
    /// configuration-daemon session that first scans the executable's
    /// directory) whose time follows the machine's load.
    fn client() -> reqwest::Client {
        reqwest::Client::builder()
            .no_proxy()
            .build()
            .expect("client")
    }

    fn counted(builds: &Cell<usize>) -> impl FnOnce() -> Result<reqwest::Client, Infallible> + '_ {
        || {
            builds.set(builds.get() + 1);
            Ok(client())
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

    /// A build in progress (TLS roots, the system proxy lookup) holds no
    /// lock: from inside it, the pool is free and a get for another key on
    /// the same runtime builds and returns.
    #[test]
    fn a_build_in_progress_does_not_block_other_keys() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("runtime");
        let _guard = runtime.enter();
        let clients = RuntimeClients::new(8);
        let builds = Cell::new(0);
        clients
            .get(1, || {
                assert!(
                    clients.clients.try_lock().is_ok(),
                    "a build holds the pool's lock"
                );
                clients.get(2, counted(&builds)).expect("other key");
                Ok::<_, Infallible>(client())
            })
            .expect("slow client");
        assert_eq!(builds.get(), 1, "the other key built during the slow build");
        assert_eq!(clients.clients.lock().expect("clients").len(), 2);
    }

    /// Two builds that race for one key keep one client: the second
    /// returns the first one's.
    #[test]
    fn racing_builds_of_one_key_keep_one_client() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("runtime");
        let _guard = runtime.enter();
        let clients = RuntimeClients::new(8);
        let builds = Cell::new(0);
        clients
            .get(1, || {
                // Another caller finishes its build of the same key first.
                clients.get(1, counted(&builds)).expect("inner client");
                builds.set(builds.get() + 1);
                Ok::<_, Infallible>(client())
            })
            .expect("outer client");
        assert_eq!(builds.get(), 2);
        assert_eq!(clients.clients.lock().expect("clients").len(), 1);
    }

    #[test]
    fn the_oldest_client_is_evicted_past_capacity() {
        let clients = RuntimeClients::new(2);
        let builds = Cell::new(0);
        on_runtime(&clients, &builds, &[1, 2, 3, 2, 1]);
        assert_eq!(builds.get(), 4, "key 1 was evicted by key 3");
    }
}
