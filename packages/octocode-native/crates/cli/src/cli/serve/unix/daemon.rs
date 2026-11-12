//! The server: `octocode serve --socket <path>` accepts connections,
//! keeps a runtime per directory and environment, and answers each call.

mod drain;
mod request;

use super::identity::{call_identity, env_digest};
use super::same_dir_form;
pub(super) use super::served::Served;
use super::sockets::remove_stale_sockets;
use super::startup::Startup;
#[cfg(test)]
pub(super) use request::attended;
pub(super) use request::handle;
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
    octocode_engine::memory::MEMORY.sweep_expired_while_running();
    tokio::task::LocalSet::new()
        .run_until(serve(socket.to_owned(), super::startup::announce))
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
    /// Clients waiting for this server to stop ([`Daemon::wait_for_stop`]).
    stopped: drain::Waiters,
    /// When a call's or a waiting client's connection says it is alive.
    pub(super) pace: super::frames::Pace,
    /// The server stopped taking calls ([`serve_on`] left its accept loop).
    leaving: tokio::sync::watch::Sender<bool>,
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
            pace: super::frames::Pace::SERVED,
            leaving: tokio::sync::watch::Sender::new(false),
        }
    }

    /// Completes once the server stops taking calls: a connection that has
    /// sent no request by then has no call to drain.
    pub(super) async fn leaving(&self) {
        let mut leaving = self.leaving.subscribe();
        let _ = leaving.wait_for(|leaving| *leaving).await;
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

/// Claim the socket, `announce` the [`Startup`] outcome (on the startup
/// channel: [`super::startup::announce`]), and serve when this server owns
/// it. Exit code 1 when it cannot serve; 0 after serving, or when another
/// server answers on the socket.
pub(super) async fn serve(socket: PathBuf, announce: impl FnOnce(&Startup) -> bool) -> u8 {
    let claimed = claim(&socket).and_then(|(listener, ours)| {
        listener
            .set_nonblocking(true)
            .and_then(|()| tokio::net::UnixListener::from_std(listener))
            .map(|listener| (listener, ours))
            .map_err(|error| {
                remove_if_ours(&socket, ours);
                Startup::Unavailable(format!("cannot listen on {}: {error}", socket.display()))
            })
    });
    let (listener, ours) = match claimed {
        Ok(claimed) => claimed,
        Err(outcome) => {
            let _ = announce(&outcome);
            return u8::from(outcome != Startup::Existing);
        }
    };
    // Ready only once the client took it: a server whose client is gone
    // leaves before it reads a call.
    if !announce(&Startup::Ready) {
        remove_if_ours(&socket, ours);
        return 1;
    }
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

/// Stop every runtime, then the servers they shared, while the clients
/// waiting for the stop hear that the server is alive; then tell them it
/// stopped.
pub(super) async fn shut_down(daemon: &Daemon) {
    daemon
        .telling_waiters(async {
            let runtimes = daemon.runtimes.borrow_mut().drain();
            for served in runtimes {
                served.runtime.close().await;
            }
            daemon.lsp_pool.clear_all().await;
        })
        .await;
    daemon
        .tell_waiters(&serde_json::json!({ "stopped": true }))
        .await;
    drop(std::mem::take(&mut *daemon.stopped.borrow_mut()));
}

/// Accept connections until nothing has been asked for `idle`, or a
/// [`STOP`](super::sockets::STOP) request arrives. Only a connection that
/// sends a request counts as asking: a probe that closes without one (a
/// starting server looking for stale sockets), or one that stays open and
/// silent, neither renews the server nor keeps it running. Then `leave`
/// runs (the socket goes, so the next call starts another server), the
/// connections that asked nothing close, and the calls taken finish; a
/// call's client that stops taking bytes is gone after the frames' silence
/// ([`super::frames`]).
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
            // A connection is active from its request line on.
            let asked = handle(&daemon, stream, id, || {
                let (active, last) = state.get();
                state.set((active + 1, last));
            })
            .await;
            if asked {
                let (active, _) = state.get();
                state.set((active.saturating_sub(1), std::time::Instant::now()));
            }
        });
    }
    // A connection not yet accepted is refused, so its client runs in
    // process; one that asked nothing closes; the calls taken finish first.
    drop(listener);
    leave();
    daemon.leaving.send_replace(true);
    daemon.tell_draining(state.get().0).await;
    let all = octocode_engine::num::saturating_u32(MAX_CONNECTIONS);
    let _ = daemon.telling_waiters(slots.acquire_many(all)).await;
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

/// Make the socket reachable by its owner only.
pub(super) fn restrict(socket: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(socket, std::fs::Permissions::from_mode(0o600))
}

/// Bind the socket for this server: the listener and the bound file's
/// identity ([`socket_identity`]), restricted to its owner. Otherwise the
/// outcome to announce instead: [`Startup::Existing`] when a server answers
/// on it, or why none can. A stale file nobody answers on is replaced.
///
/// The socket directory's lock is held from bind to restriction, so two
/// servers starting at once never both bind: the one that waited finds the
/// other's socket answering. A bound socket answers (from its backlog) once
/// the lock is released.
pub(super) fn claim(
    socket: &Path,
) -> Result<(std::os::unix::net::UnixListener, Option<(u64, u64)>), Startup> {
    use std::os::unix::net::{UnixListener, UnixStream};
    let failed = |what: &str, error: std::io::Error| {
        Startup::Unavailable(format!("cannot {what} {}: {error}", socket.display()))
    };
    let dir = socket
        .parent()
        .ok_or_else(|| Startup::Unavailable(format!("{} has no directory", socket.display())))?;
    let _locked = lock(dir).map_err(|error| failed("lock the directory of", error))?;
    let listener = match UnixListener::bind(socket) {
        Ok(listener) => listener,
        Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
            if UnixStream::connect(socket).is_ok() {
                return Err(Startup::Existing);
            }
            std::fs::remove_file(socket).map_err(|error| failed("replace the stale", error))?;
            UnixListener::bind(socket).map_err(|error| failed("bind", error))?
        }
        Err(error) => return Err(failed("bind", error)),
    };
    let ours = socket_identity(socket);
    // A socket whose mode is left to the umask is not served.
    if let Err(error) = restrict(socket) {
        remove_if_ours(socket, ours);
        return Err(failed("restrict", error));
    }
    Ok((listener, ours))
}

/// An exclusive lock on `dir`, held until the returned handle drops.
fn lock(dir: &Path) -> std::io::Result<std::fs::File> {
    use std::os::fd::AsRawFd as _;
    let dir = std::fs::File::open(dir)?;
    loop {
        // SAFETY: `dir` is an open descriptor for the whole call.
        if unsafe { libc::flock(dir.as_raw_fd(), libc::LOCK_EX) } == 0 {
            return Ok(dir);
        }
        let error = std::io::Error::last_os_error();
        if error.kind() != std::io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
}
