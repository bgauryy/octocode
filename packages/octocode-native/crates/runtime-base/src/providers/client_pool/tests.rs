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
