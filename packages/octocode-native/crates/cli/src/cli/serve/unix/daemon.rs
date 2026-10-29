//! The server: `octocode serve --socket <path>` accepts connections,
//! keeps a runtime per directory and environment, and answers each call.

use super::identity::{call_identity, env_digest};
pub(super) use super::served::Served;
use super::served::recheck_provider;
use super::sockets::{prune_siblings, touch};
use super::{read_line_bounded, same_dir_form};
use crate::cli::Printed;
use crate::cli::serve::runs_on_server;
use octocode_native::id::ToolId;
use serde_json::Value;
use serde_json::json;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The server exits after this long without a request.
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

/// A server's state: its socket, one language-server pool, and the
/// runtimes of the directories and environments it serves.
pub(super) struct Daemon {
    /// The socket this server answers on; its modification time is the
    /// last request ([`touch`]).
    pub(super) socket: PathBuf,
    pub(super) lsp_pool: std::sync::Arc<octocode_engine::lsp::pool::LspClientPool>,
    pub(super) runtimes: std::cell::RefCell<Recent<RuntimeKey, std::rc::Rc<Served>>>,
    /// One build per key at a time: a call that waited finds it built.
    pub(super) building: octocode_io::flight::KeyedFlight<RuntimeKey, ()>,
    pub(super) build: Build,
    /// A [`STOP`](super::sockets::STOP) request arrived.
    pub(super) stop: tokio::sync::Notify,
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
        tool: &str,
    ) -> Result<std::rc::Rc<Served>, String> {
        let key = (same_dir_form(cwd), env_digest(&env));
        let ticket = self.building.join(key.clone());
        let _turn = ticket.turn().await;
        if let Some(served) = self.runtimes.borrow_mut().get(&key)
            && served.identity == identity
            && !recheck_provider(tool, served.provider_failed(), served.built.elapsed())
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
        let runtime = runtime.check_clasify_provider().await;
        let served = std::rc::Rc::new(Served {
            runtime,
            identity: built,
            built: std::time::Instant::now(),
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
    prune_siblings(&socket);
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
}

/// Accept connections until nothing has been asked for `idle`, or a
/// [`STOP`](super::sockets::STOP) request arrives. Then `leave` runs (the socket goes, so the next
/// call starts another server) and the calls taken finish.
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
    let mut next_id = 0u64;
    loop {
        let accepted = tokio::select! {
            accepted = listener.accept() => accepted,
            () = daemon.stop.notified() => break,
            () = tokio::time::sleep(poll) => {
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
            let (active, _) = state.get();
            state.set((active + 1, std::time::Instant::now()));
            handle(&daemon, stream, id).await;
            let (active, _) = state.get();
            state.set((active.saturating_sub(1), std::time::Instant::now()));
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

/// Await `execution` unless the client leaves first: it never writes after
/// its request, so a read that returns (end of stream on Ctrl-C or exit)
/// means it left. Then `cancel` runs, the cancelled work drains, and the
/// result is `None`.
pub(super) async fn unless_left<T>(
    execution: impl std::future::Future<Output = T>,
    client: &mut (impl tokio::io::AsyncRead + Unpin),
    cancel: impl FnOnce(),
) -> Option<T> {
    use tokio::io::AsyncReadExt;
    tokio::pin!(execution);
    let mut probe = [0u8; 1];
    tokio::select! {
        result = &mut execution => Some(result),
        _ = client.read(&mut probe) => {
            cancel();
            let _ = execution.await;
            None
        }
    }
}

/// One connection: read the request line, run it on the runtime of the
/// client's directory and environment in the client's output mode, and
/// write the printed call back. A request this server cannot answer as the
/// client would run it gets a `fallback` reply, so the client runs it in
/// process. A [`STOP`](super::sockets::STOP) request stops the server.
pub(super) async fn handle(daemon: &Daemon, stream: tokio::net::UnixStream, id: String) {
    use tokio::io::{AsyncWriteExt, BufReader};
    let (read, mut write) = stream.into_split();
    let mut reader = BufReader::new(read);
    let Some(line) = read_line_bounded(&mut reader, MAX_REQUEST_BYTES).await else {
        return;
    };
    touch(&daemon.socket);
    let reply = match serde_json::from_str::<Value>(&line) {
        Ok(request) if request["stop"] == true => {
            daemon.stop.notify_one();
            return;
        }
        Ok(mut request) => match served(daemon, &mut request).await {
            Ok((served, tool, json_out, input)) => {
                let runtime = &served.runtime;
                // What this call raises, and only that, goes in its reply.
                let notices = octocode_io::notice::Scope::default();
                let query = input.clone();
                // The client's directory, which `served` read.
                let cwd = request["cwd"].as_str().map(PathBuf::from);
                let printed = match runtime.admit(id.clone()) {
                    Err(error) => Printed::finish(&tool, Err(error), json_out, &query, None),
                    Ok(admission) => {
                        let execution = notices.run(runtime.execute(
                            admission,
                            tool.clone(),
                            input,
                            octocode_native::runtime::Channel::Cli,
                        ));
                        let cancel = || {
                            runtime.requests.cancel(&id);
                        };
                        let Some(result) = unless_left(execution, &mut reader, cancel).await else {
                            return;
                        };
                        Printed::finish(&tool, result, json_out, &query, cwd.as_deref())
                    }
                };
                let mut reply = printed.to_json();
                reply["notices"] = json!(notices.notices());
                reply
            }
            Err(reason) => json!({ "fallback": reason }),
        },
        Err(_) => json!({ "fallback": "the request was not JSON" }),
    };
    let mut line = reply.to_string();
    line.push('\n');
    let _ = write.write_all(line.as_bytes()).await;
}

/// The runtime, tool, output mode and input of `request`, or why this
/// server does not run it.
pub(super) async fn served(
    daemon: &Daemon,
    request: &mut Value,
) -> Result<(std::rc::Rc<Served>, String, bool, Value), String> {
    let (Some(tool), Some(json_out), Some(cwd), Some(identity), Ok(env)) = (
        request["tool"].as_str().map(str::to_owned),
        request["json"].as_bool(),
        request["cwd"].as_str().map(PathBuf::from),
        request["identity"].as_str().map(str::to_owned),
        serde_json::from_value::<BTreeMap<String, String>>(request["env"].take()),
    ) else {
        return Err("the request was not one this server reads".into());
    };
    if !ToolId::from_name(&tool).is_some_and(runs_on_server) {
        return Err(format!("it does not run {tool}"));
    }
    let served = daemon.runtime_for(&cwd, env, &identity, &tool).await?;
    Ok((served, tool, json_out, request["input"].take()))
}
