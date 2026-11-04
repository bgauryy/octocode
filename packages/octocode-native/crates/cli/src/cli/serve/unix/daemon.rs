//! The server: `octocode serve --socket <path>` accepts connections,
//! keeps a runtime per directory and environment, and answers each call.

mod request;

use super::identity::{call_identity, env_digest};
use super::same_dir_form;
pub(super) use super::served::Served;
use super::sockets::remove_stale_sockets;
pub(super) use request::handle;
#[cfg(test)]
pub(super) use request::unless_left;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The server exits after this long without a request. This is what bounds
/// the servers a user has: demand keeps one alive, and one nobody asks
/// (an older build, a finished session) leaves by itself, while a server
/// with calls or recent calls is never stopped by another server starting.
/// Its language servers leave earlier, on the pool's own idle timeout.
pub(super) const IDLE_EXIT: std::time::Duration = std::time::Duration::from_secs(600);

/// Largest request line a client may send; longer lines are refused unbuffered.
pub(super) const MAX_REQUEST_BYTES: u64 = 16 * 1024 * 1024;

/// Connections served at once; a connection over the cap is closed at accept.
pub(super) const MAX_CONNECTIONS: usize = 64;

/// Working directories (by environment) a server keeps a runtime for.
pub(super) const MAX_RUNTIMES: usize = 8;

/// Pause after a failed accept (for example EMFILE) so the loop cannot spin.
pub(super) const ACCEPT_BACKOFF: std::time::Duration = std::time::Duration::from_millis(100);

/// `octocode serve --socket <path>`: answer requests until idle.
pub(in crate::cli) async fn run(socket: &Path) -> u8 {
    tokio::task::LocalSet::new()
        .run_until(serve(socket.to_owned()))
        .await
}

/// Values by key, at most `cap` of them; the least recently used leaves
/// first.
pub(super) struct Recent<K, V> {
    pub(super) cap: usize,
    /// Least recently used first.
    pub(super) entries: Vec<(K, V)>,
}

impl<K: PartialEq, V> Recent<K, V> {
    pub(super) fn new(cap: usize) -> Self {
        Self {
            cap,
            entries: Vec::new(),
        }
    }

    /// The value at `key`, now the most recently used.
    pub(super) fn get(&mut self, key: &K) -> Option<&V> {
        let at = self.entries.iter().position(|(k, _)| k == key)?;
        let entry = self.entries.remove(at);
        self.entries.push(entry);
        self.entries.last().map(|(_, value)| value)
    }

    /// Put `value` at `key`, replacing any value there; the evicted value
    /// when the cap is passed.
    pub(super) fn insert(&mut self, key: K, value: V) -> Option<V> {
        self.entries.retain(|(k, _)| *k != key);
        self.entries.push((key, value));
        (self.entries.len() > self.cap).then(|| self.entries.remove(0).1)
    }

    pub(super) fn drain(&mut self) -> Vec<V> {
        self.entries.drain(..).map(|(_, value)| value).collect()
    }
}

/// How a server builds a runtime:
/// `ToolRuntime::from_host_with_lsp_pool`, which a test slows down.
pub(super) type Build =
    fn(
        octocode_native::runtime::HostOptions,
        std::sync::Arc<octocode_engine::lsp::pool::LspClientPool>,
    )
        -> Result<octocode_native::runtime::ToolRuntime, octocode_native::runtime::RuntimeError>;

/// A kept runtime's working directory and the [`env_digest`] of the
/// environment it runs with.
pub(super) type RuntimeKey = (PathBuf, String);

/// A server's state: one language-server pool, and the runtimes of the
/// directories and environments it serves.
pub(super) struct Daemon {
    /// The socket this server answers on; its modification time is when it
    /// was last asked ([`touch`]), readable by anything that lists the
    /// socket directory.
    pub(super) socket: PathBuf,
    pub(super) lsp_pool: std::sync::Arc<octocode_engine::lsp::pool::LspClientPool>,
    pub(super) runtimes: std::cell::RefCell<Recent<RuntimeKey, std::rc::Rc<Served>>>,
    /// One build per key at a time: a call that waited finds it built.
    pub(super) building: octocode_io::flight::KeyedFlight<RuntimeKey, ()>,
    pub(super) build: Build,
    /// A [`STOP`](super::sockets::STOP) request arrived.
    pub(super) stop: tokio::sync::Notify,
    stopped: std::cell::RefCell<Vec<tokio::net::unix::OwnedWriteHalf>>,
}

impl Daemon {
    pub(super) fn new(socket: PathBuf) -> Self {
        Self::with_build(
            socket,
            octocode_native::runtime::ToolRuntime::from_host_with_lsp_pool,
        )
    }

    pub(super) fn with_build(socket: PathBuf, build: Build) -> Self {
        Self {
            socket,
            lsp_pool: std::sync::Arc::default(),
            runtimes: std::cell::RefCell::new(Recent::new(MAX_RUNTIMES)),
            building: octocode_io::flight::KeyedFlight::transient(),
            build,
            stop: tokio::sync::Notify::new(),
            stopped: std::cell::RefCell::default(),
        }
    }

    /// The runtime for a call from `cwd` with `env` under `identity`: the
    /// kept one when its identity is the call's, else a new one built as a
    /// CLI started in `cwd` with `env` would be. `Err` (the reason) when
    /// even that one differs.
    pub(super) async fn runtime_for(
        &self,
        cwd: &Path,
        env: BTreeMap<String, String>,
        identity: &str,
    ) -> Result<std::rc::Rc<Served>, String> {
        let key = (same_dir_form(cwd), env_digest(&env));
        let ticket = self.building.join(key.clone());
        let _turn = ticket.turn().await;
        if let Some(served) = self.runtimes.borrow_mut().get(&key)
            && served.identity == identity
        {
            return Ok(std::rc::Rc::clone(served));
        }
        let host = octocode_native::runtime::HostOptions {
            cwd: Some(key.0.clone()),
            env: Some(env.clone()),
            ..crate::cli::cli_host()
        };
        let (build, pool) = (self.build, std::sync::Arc::clone(&self.lsp_pool));
        // Off this thread, which serves every connection: a build reads the
        // config files and may run the daily cache sweep.
        let not_started = "its runtime for this directory did not start";
        let runtime = tokio::task::spawn_blocking(move || build(host, pool))
            .await
            .map_err(|_| not_started.to_owned())?
            .map_err(|error| format!("{not_started}: {}", error.message))?;
        let built = call_identity(runtime.config(), &env);
        if built != identity {
            return Err("it runs under another configuration than this call".into());
        }
        let served = std::rc::Rc::new(Served {
            runtime,
            identity: built,
        });
        // An evicted runtime ends when its last call returns; the servers
        // it used stay in the shared pool.
        drop(
            self.runtimes
                .borrow_mut()
                .insert(key, std::rc::Rc::clone(&served)),
        );
        Ok(served)
    }
}

pub(super) async fn serve(socket: PathBuf) -> u8 {
    use std::os::unix::fs::PermissionsExt;
    let Some(listener) = bind(&socket) else {
        return 0;
    };
    let ours = socket_identity(&socket);
    let _ = std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600));
    remove_stale_sockets(&socket);
    let daemon = std::rc::Rc::new(Daemon::new(socket.clone()));
    // Remove the socket only while it is still ours.
    serve_on(listener, &daemon, IDLE_EXIT, || {
        remove_if_ours(&socket, ours);
    })
    .await;
    shut_down(&daemon).await;
    0
}

/// Stop every runtime, then the servers they shared.
pub(super) async fn shut_down(daemon: &Daemon) {
    let runtimes = daemon.runtimes.borrow_mut().drain();
    for served in runtimes {
        served.runtime.close().await;
    }
    daemon.lsp_pool.clear_all().await;
    use tokio::io::AsyncWriteExt;
    let waiters = std::mem::take(&mut *daemon.stopped.borrow_mut());
    for mut waiter in waiters {
        let _ = waiter.write_all(b"{\"stopped\":true}\n").await;
    }
}

/// Accept connections until nothing has been asked for `idle`, or a
/// [`STOP`](super::sockets::STOP) request arrives. Only a connection that
/// sends a request counts as asking: a probe that closes without one (a
/// starting server looking for stale sockets) does not renew the server.
/// Then `leave` runs (the socket goes, so the next call starts another
/// server) and the calls taken finish.
pub(super) async fn serve_on(
    listener: tokio::net::UnixListener,
    daemon: &std::rc::Rc<Daemon>,
    idle: std::time::Duration,
    leave: impl FnOnce(),
) {
    use std::rc::Rc;
    use std::sync::Arc;
    let state = Rc::new(std::cell::Cell::new((0usize, std::time::Instant::now())));
    let slots = Arc::new(tokio::sync::Semaphore::new(MAX_CONNECTIONS));
    let poll = idle.min(std::time::Duration::from_secs(15));
    // A fixed schedule: connections arriving between checks do not move it.
    let mut check = tokio::time::interval_at(tokio::time::Instant::now() + poll, poll);
    let mut next_id = 0u64;
    loop {
        let accepted = tokio::select! {
            accepted = listener.accept() => accepted,
            () = daemon.stop.notified() => break,
            _ = check.tick() => {
                let (active, last) = state.get();
                if active == 0 && last.elapsed() >= idle {
                    break;
                }
                continue;
            }
        };
        let Ok((stream, _)) = accepted else {
            tokio::time::sleep(ACCEPT_BACKOFF).await;
            continue;
        };
        // Over the cap the connection is dropped here, so its client sees EOF.
        let Ok(permit) = Arc::clone(&slots).try_acquire_owned() else {
            continue;
        };
        next_id += 1;
        let id = format!("serve-{next_id}");
        let (daemon, state) = (Rc::clone(daemon), Rc::clone(&state));
        tokio::task::spawn_local(async move {
            let _permit = permit;
            let (active, last) = state.get();
            state.set((active + 1, last));
            let asked = handle(&daemon, stream, id).await;
            let (active, last) = state.get();
            let last = if asked {
                std::time::Instant::now()
            } else {
                last
            };
            state.set((active.saturating_sub(1), last));
        });
    }
    // A connection not yet accepted is refused, so its client runs in
    // process; the ones taken finish first.
    drop(listener);
    leave();
    let all = u32::try_from(MAX_CONNECTIONS).unwrap_or(u32::MAX);
    let _ = slots.acquire_many(all).await;
}

/// The socket file's identity, `(device, inode)`, taken right after bind.
pub(super) fn socket_identity(path: &Path) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    std::fs::symlink_metadata(path)
        .ok()
        .map(|meta| (meta.dev(), meta.ino()))
}

/// Remove `path` only while it is still the socket this server bound: a
/// successor that rebound the path keeps its live socket.
pub(super) fn remove_if_ours(path: &Path, ours: Option<(u64, u64)>) {
    if ours.is_some() && socket_identity(path) == ours {
        let _ = std::fs::remove_file(path);
    }
}

/// Bind the socket, replacing a stale file no server answers on. `None`
/// when another server already owns it.
pub(super) fn bind(socket: &Path) -> Option<tokio::net::UnixListener> {
    match tokio::net::UnixListener::bind(socket) {
        Ok(listener) => Some(listener),
        Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
            if std::os::unix::net::UnixStream::connect(socket).is_ok() {
                return None;
            }
            std::fs::remove_file(socket).ok()?;
            tokio::net::UnixListener::bind(socket).ok()
        }
        Err(_) => None,
    }
}
