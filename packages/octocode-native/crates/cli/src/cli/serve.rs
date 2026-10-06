//! Warm `lspSearch` across CLI calls.
//!
//! A language server answers in milliseconds once it has loaded a project,
//! but every CLI process used to start one cold (3–6 s for TypeScript). The
//! first `lspSearch` call of a workspace therefore starts `octocode serve`: a
//! detached process that owns one [`ToolRuntime`] (and its servers) behind a
//! private Unix socket. Later calls send their query there.
//!
//! - One server per binary build, working directory, Octocode/GitHub
//!   environment, and config file version: the socket name hashes all of
//!   them, so a rebuild or config edit gets a fresh server.
//! - The socket lives in `<octocode home>/run` (mode 0700); only the owner
//!   can connect. With `storage.mode = memory`, or on Windows, calls run in
//!   process as before.
//! - Any failure to reach the server falls back to an in-process call.
//! - The server exits after [`IDLE_EXIT`] without a request; a client that
//!   disconnects mid-call cancels its request.

use serde_json::{Value, json};

/// The server exits after this long without a request.
#[cfg(unix)]
const IDLE_EXIT: std::time::Duration = std::time::Duration::from_secs(600);

/// The reply line: exactly what an in-process call would print.
#[cfg(unix)]
fn reply(structured: Value, text: String, exit: u8) -> Value {
    json!({"structured": structured, "text": text, "exit": exit})
}

/// Run `tool` through a workspace server when one is reachable or can be
/// started. `None` means the caller runs it in process.
#[cfg(unix)]
pub(super) async fn call(tool: &str, input: &Value, json_out: bool) -> Option<u8> {
    let socket = socket_path()?;
    let mut stream = match connect(&socket).await {
        Some(stream) => stream,
        None => {
            spawn_server(&socket)?;
            wait_for(&socket).await?
        }
    };
    let response = exchange(&mut stream, tool, input).await?;
    let exit = u8::try_from(response["exit"].as_u64()?).ok()?;
    let printed = if json_out {
        super::write_json(&response["structured"], true)
    } else {
        let text = response["text"].as_str().unwrap_or_default();
        if response["error"].as_bool() == Some(true) {
            eprintln!("{text}");
            0
        } else {
            super::write_text(text)
        }
    };
    Some(if printed == 0 { exit } else { printed })
}

#[cfg(not(unix))]
pub(super) async fn call(_tool: &str, _input: &Value, _json_out: bool) -> Option<u8> {
    None
}

#[cfg(unix)]
async fn exchange(
    stream: &mut tokio::net::UnixStream,
    tool: &str,
    input: &Value,
) -> Option<Value> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let mut line = json!({"tool": tool, "input": input}).to_string();
    line.push('\n');
    stream.write_all(line.as_bytes()).await.ok()?;
    let mut reader = BufReader::new(stream);
    let mut response = String::new();
    let read = tokio::select! {
        read = reader.read_line(&mut response) => read.ok()?,
        // Dropping the connection cancels the request on the server.
        _ = tokio::signal::ctrl_c() => std::process::exit(130),
    };
    if read == 0 {
        return None;
    }
    serde_json::from_str(&response).ok()
}

#[cfg(unix)]
async fn connect(socket: &std::path::Path) -> Option<tokio::net::UnixStream> {
    tokio::net::UnixStream::connect(socket).await.ok()
}

/// Wait up to two seconds for a just-started server to bind its socket.
#[cfg(unix)]
async fn wait_for(socket: &std::path::Path) -> Option<tokio::net::UnixStream> {
    for _ in 0..80 {
        if let Some(stream) = connect(socket).await {
            return Some(stream);
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    None
}

/// Start `octocode serve --socket <path>` detached from this process: its own
/// session, no inherited stdio (a caller capturing our output must not wait
/// for the server).
#[cfg(unix)]
fn spawn_server(socket: &std::path::Path) -> Option<()> {
    use std::os::unix::process::CommandExt;
    let exe = std::env::current_exe().ok()?;
    let mut command = std::process::Command::new(exe);
    command
        .arg("serve")
        .arg("--socket")
        .arg(socket)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    // SAFETY: setsid is async-signal-safe and touches only the child.
    unsafe {
        command.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    command.spawn().ok().map(|_| ())
}

/// `<home>/run/lsp-<key>.sock`, or `None` when the server must not be used:
/// memory storage, an unsafe run directory, or a path too long for a socket.
#[cfg(unix)]
fn socket_path() -> Option<std::path::PathBuf> {
    use octocode_native::config::{
        RuntimeSurface, acquire_config_input, is_persistent_storage_enabled, resolve_config,
    };
    use std::collections::BTreeMap;
    let env: BTreeMap<String, String> = std::env::vars().collect();
    let cwd = std::env::current_dir().ok()?;
    let os_home = std::env::home_dir()?;
    let input = acquire_config_input(env.clone(), cwd.clone(), os_home, false, RuntimeSurface::Cli);
    let config = resolve_config(&input);
    if !is_persistent_storage_enabled(&config.resolved) {
        return None;
    }
    let run = config.home.join("run");
    let key = server_key(&env, &cwd, &config.home)?;
    let socket = run.join(format!("lsp-{key}.sock"));
    // macOS limits a socket path to 104 bytes.
    if socket.as_os_str().len() >= 100 {
        return None;
    }
    private_dir(&run)?;
    Some(socket)
}

/// A directory only this user can enter, created when missing.
#[cfg(unix)]
fn private_dir(dir: &std::path::Path) -> Option<()> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
    if !dir.exists() {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
            .ok()?;
    }
    let meta = std::fs::symlink_metadata(dir).ok()?;
    // SAFETY: getuid has no preconditions.
    let uid = unsafe { libc::getuid() };
    (meta.is_dir() && meta.uid() == uid && meta.permissions().mode() & 0o077 == 0).then_some(())
}

/// 16 hex digits over everything that changes what the runtime would do:
/// this binary, the working directory, Octocode/GitHub/PATH environment, and
/// the mtimes of the config and .env files.
#[cfg(unix)]
fn server_key(
    env: &std::collections::BTreeMap<String, String>,
    cwd: &std::path::Path,
    home: &std::path::Path,
) -> Option<String> {
    use std::fmt::Write;
    let stamp = |path: &std::path::Path| -> String {
        std::fs::metadata(path)
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or_else(|| "-".into(), |age| format!("{}", age.as_nanos()))
    };
    let exe = std::env::current_exe().ok()?;
    let exe_meta = std::fs::metadata(&exe).ok()?;
    let mut identity = String::new();
    let _ = writeln!(identity, "{}|{}|{}", exe.display(), exe_meta.len(), stamp(&exe));
    let _ = writeln!(identity, "{}", cwd.display());
    for (name, value) in env {
        if name.starts_with("OCTOCODE_")
            || name.starts_with("GITHUB_")
            || name.starts_with("GH_")
            || name == "PATH"
        {
            let _ = writeln!(identity, "{name}={value}");
        }
    }
    let project = cwd.join(".octocode");
    for path in [
        home.join(".octocoderc"),
        home.join(".env"),
        project.join(".octocoderc"),
        project.join(".env"),
    ] {
        let _ = writeln!(identity, "{}={}", path.display(), stamp(&path));
    }
    Some(octocode_engine::digest::sha256(identity.as_bytes())[..16].to_owned())
}

/// `octocode serve --socket <path>`: answer requests until idle.
#[cfg(unix)]
pub(super) async fn run(socket: &std::path::Path) -> u8 {
    tokio::task::LocalSet::new()
        .run_until(serve(socket.to_owned()))
        .await
}

#[cfg(not(unix))]
pub(super) async fn run(_socket: &std::path::Path) -> u8 {
    super::emit_error("octocode serve needs Unix sockets.", false);
    2
}

#[cfg(unix)]
async fn serve(socket: std::path::PathBuf) -> u8 {
    use octocode_native::config::RuntimeSurface;
    use octocode_native::runtime::{HostOptions, ToolRuntime};
    use std::os::unix::fs::PermissionsExt;
    use std::rc::Rc;
    let Some(listener) = bind(&socket) else {
        return 0;
    };
    prune_stale(&socket);
    let _ = std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600));
    let Ok(runtime) = ToolRuntime::from_host(HostOptions {
        surface: RuntimeSurface::Cli,
        timeout_secs: Some(super::INTERACTIVE_EXECUTION_TIMEOUT_SECS),
        ..HostOptions::default()
    }) else {
        let _ = std::fs::remove_file(&socket);
        return 5;
    };
    let runtime = Rc::new(runtime);
    let state = Rc::new(std::cell::Cell::new((0usize, std::time::Instant::now())));
    let mut next_id = 0u64;
    loop {
        let accepted = tokio::select! {
            accepted = listener.accept() => accepted,
            () = tokio::time::sleep(std::time::Duration::from_secs(15)) => {
                let (active, last) = state.get();
                if active == 0 && last.elapsed() >= IDLE_EXIT {
                    break;
                }
                continue;
            }
        };
        let Ok((stream, _)) = accepted else { continue };
        next_id += 1;
        let id = format!("serve-{next_id}");
        let (runtime, state) = (Rc::clone(&runtime), Rc::clone(&state));
        tokio::task::spawn_local(async move {
            let (active, _) = state.get();
            state.set((active + 1, std::time::Instant::now()));
            handle(&runtime, stream, id).await;
            let (active, _) = state.get();
            state.set((active.saturating_sub(1), std::time::Instant::now()));
        });
    }
    // Remove the socket only while it is still ours.
    let _ = std::fs::remove_file(&socket);
    runtime.close().await;
    0
}

/// Remove sibling sockets no server answers on: servers of an older build or
/// configuration that were killed before their idle exit.
#[cfg(unix)]
fn prune_stale(own: &std::path::Path) {
    let Some(dir) = own.parent() else { return };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let is_socket = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("lsp-") && name.ends_with(".sock"));
        if is_socket && path != own && std::os::unix::net::UnixStream::connect(&path).is_err() {
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// Bind the socket, replacing a stale file no server answers on. `None`
/// when another server already owns it.
#[cfg(unix)]
fn bind(socket: &std::path::Path) -> Option<tokio::net::UnixListener> {
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

/// One connection: read the request line, run it, and write the reply. A
/// client that disconnects first cancels the request.
#[cfg(unix)]
async fn handle(
    runtime: &octocode_native::runtime::ToolRuntime,
    stream: tokio::net::UnixStream,
    id: String,
) {
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
    let (read, mut write) = stream.into_split();
    let mut reader = BufReader::new(read);
    let mut line = String::new();
    if reader.read_line(&mut line).await.unwrap_or(0) == 0 {
        return;
    }
    let Ok(mut request) = serde_json::from_str::<Value>(&line) else {
        return;
    };
    let (Some(tool), input) = (
        request["tool"].as_str().map(str::to_owned),
        request["input"].take(),
    ) else {
        return;
    };
    let execution = runtime.execute_rendered(id.clone(), tool, input);
    tokio::pin!(execution);
    let mut probe = [0u8; 1];
    let result = tokio::select! {
        result = &mut execution => result,
        // The client never writes again; a read returning means it left.
        _ = reader.read(&mut probe) => {
            runtime.requests.cancel(&id);
            let _ = execution.await;
            return;
        }
    };
    let response = match result {
        Ok(outcome) => {
            let exit = super::exit_code(outcome.exit_class());
            let text = super::outcome_text(&outcome.content);
            reply(outcome.structured_content, text, exit)
        }
        Err(error) => {
            let exit = super::exit_code(error.exit_class());
            let value = super::runtime_error_output(error);
            let text = super::runtime_error_text(&value);
            let mut response = reply(value, text, exit);
            response["error"] = json!(true);
            response
        }
    };
    let mut line = response.to_string();
    line.push('\n');
    let _ = write.write_all(line.as_bytes()).await;
}

#[cfg(all(test, unix))]
mod tests {
    use std::collections::BTreeMap;

    #[test]
    fn server_key_tracks_environment_and_working_directory() {
        let home = tempfile::tempdir().expect("home");
        let mut env = BTreeMap::from([("OCTOCODE_HOME".to_owned(), "a".to_owned())]);
        let cwd = home.path().join("w");
        let key = super::server_key(&env, &cwd, home.path()).expect("key");
        assert_eq!(key.len(), 16);
        assert_eq!(super::server_key(&env, &cwd, home.path()), Some(key.clone()));
        env.insert("TERM_SESSION_ID".into(), "ignored".into());
        assert_eq!(super::server_key(&env, &cwd, home.path()), Some(key.clone()));
        env.insert("GITHUB_TOKEN".into(), "t".into());
        assert_ne!(super::server_key(&env, &cwd, home.path()), Some(key.clone()));
        assert_ne!(
            super::server_key(&BTreeMap::new(), &home.path().join("other"), home.path()),
            Some(key)
        );
    }

    #[test]
    fn stale_sockets_are_pruned_and_live_ones_kept() {
        let dir = tempfile::tempdir().expect("dir");
        let own = dir.path().join("lsp-own.sock");
        let live = dir.path().join("lsp-live.sock");
        let _listener = std::os::unix::net::UnixListener::bind(&live).expect("bind");
        let stale = dir.path().join("lsp-stale.sock");
        drop(std::os::unix::net::UnixListener::bind(&stale).expect("bind"));
        let other = dir.path().join("notes.txt");
        std::fs::write(&other, "x").expect("write");
        super::prune_stale(&own);
        assert!(live.exists() && other.exists());
        assert!(!stale.exists());
    }

    #[test]
    fn run_directory_must_be_private() {
        use std::os::unix::fs::PermissionsExt;
        let home = tempfile::tempdir().expect("home");
        let run = home.path().join("run");
        assert!(super::private_dir(&run).is_some());
        std::fs::set_permissions(&run, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        assert!(super::private_dir(&run).is_none());
    }
}
