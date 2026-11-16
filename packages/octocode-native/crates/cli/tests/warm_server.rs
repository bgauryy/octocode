//! The warm tool server end to end: the one integration test whose tool
//! calls compare `octocode serve` with in-process execution.
use crate::support::Workspace;

/// When the server under `sockets` was last asked a call: a server marks
/// its socket's modification time on each request it reads.
fn last_asked(sockets: &std::path::Path) -> Option<std::time::SystemTime> {
    std::fs::read_dir(sockets)
        .ok()?
        .flatten()
        .filter_map(|user| std::fs::read_dir(user.path()).ok())
        .flatten()
        .flatten()
        .find(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.starts_with("serve-") && name.ends_with(".sock")
        })?
        .metadata()
        .ok()?
        .modified()
        .ok()
}

/// A call through the warm server prints what the same call prints in
/// process: stdout, stderr and exit code. The first call starts the server,
/// the second reaches it (the server marks it as asked, which a call run
/// in process never does); the test stops it at the end.
#[test]
fn a_tool_call_answers_alike_through_the_warm_server_and_in_process() {
    let workspace = Workspace::new();
    std::fs::write(
        workspace.workspace.join("lib.rs"),
        "fn alpha() {}\nfn beta() { alpha(); }\n",
    )
    .expect("source");
    let persistent = workspace.persistent_cli();
    let query = r#"{"queries":[{"matchString": ["alpha"],"path":"."}]}"#;
    let run = |served: bool| {
        let mut command = if served {
            persistent.cli()
        } else {
            workspace.cli()
        };
        command
            .env("TMPDIR", persistent.sockets())
            .args(["localSearch", query, "--json"]);
        command.output().expect("localSearch")
    };
    let in_process = run(false);
    assert_eq!(in_process.status.code(), Some(0), "{in_process:?}");
    assert_eq!(
        persistent.server_pids().len(),
        0,
        "memory storage runs in process"
    );
    let mut asked = None;
    for call in ["starts the server", "reaches it"] {
        let served = run(true);
        assert_eq!(served.status.code(), in_process.status.code(), "{call}");
        assert_eq!(
            String::from_utf8_lossy(&served.stdout),
            String::from_utf8_lossy(&in_process.stdout),
            "{call}"
        );
        assert_eq!(
            String::from_utf8_lossy(&served.stderr),
            String::from_utf8_lossy(&in_process.stderr),
            "{call}"
        );
        let now_asked = last_asked(persistent.sockets());
        assert!(now_asked.is_some(), "{call}: a server socket");
        assert!(now_asked > asked, "{call}: the server was asked");
        asked = now_asked;
    }
    assert_eq!(
        persistent.server_pids().len(),
        1,
        "one server answered both"
    );
    assert!(persistent.stop(), "the test's server is stopped");
}

#[test]
fn a_persistent_fixture_stops_its_server_when_an_assertion_unwinds() {
    let workspace = Workspace::new();
    workspace.write("lib.rs", "fn alpha() {}\n");
    let mut owned = None;
    let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let persistent = workspace.persistent_cli();
        let output = persistent
            .cli()
            .args([
                "localSearch",
                r#"{"queries":[{"matchString":["alpha"],"path":"."}]}"#,
                "--json",
            ])
            .output()
            .expect("persistent search");
        assert!(output.status.success(), "{output:?}");
        let pids = persistent.server_pids();
        assert_eq!(pids.len(), 1, "one owned server");
        owned = Some(pids[0]);
        panic!("exercise fixture cleanup during assertion unwinding");
    }));
    assert!(unwound.is_err());
    let pid = owned.expect("the fixture started a server before unwinding");
    let listed = std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "args="])
        .output()
        .expect("inspect stopped PID");
    assert!(
        !String::from_utf8_lossy(&listed.stdout).contains(" serve --socket "),
        "the owned server survived unwinding: {listed:?}"
    );
}

/// CLI calls that find no server and start one at once share one server:
/// every starter but the owner finds it answering, and each call answers
/// once, as in process.
#[test]
fn calls_starting_a_server_at_once_share_one() {
    let workspace = Workspace::new();
    workspace.write("lib.rs", "fn alpha() {}\nfn beta() { alpha(); }\n");
    let persistent = workspace.persistent_cli();
    let query = r#"{"queries":[{"matchString": ["alpha"],"path":"."}]}"#;
    let in_process = workspace
        .cli()
        .env("TMPDIR", persistent.sockets())
        .args(["localSearch", query, "--json"])
        .output()
        .expect("in process");
    assert_eq!(in_process.status.code(), Some(0), "{in_process:?}");
    let calls: Vec<_> = (0..6)
        .map(|_| {
            persistent
                .cli()
                .args(["localSearch", query, "--json"])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .expect("starter")
        })
        .collect();
    for call in calls {
        let served = call.wait_with_output().expect("starter output");
        assert_eq!(served.status.code(), Some(0), "{served:?}");
        assert_eq!(
            String::from_utf8_lossy(&served.stdout),
            String::from_utf8_lossy(&in_process.stdout)
        );
        assert_eq!(
            String::from_utf8_lossy(&served.stderr),
            String::from_utf8_lossy(&in_process.stderr)
        );
    }
    assert_eq!(persistent.server_pids().len(), 1, "one server for all");
    assert!(persistent.stop(), "the test's server is stopped");
}

/// A server that cannot bind its socket says why at once; the CLI names
/// that cause on stderr and runs the call in process.
#[test]
fn a_server_that_cannot_start_is_named_and_the_call_runs_in_process() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let workspace = Workspace::new();
    workspace.write("lib.rs", "fn alpha() {}\n");
    let persistent = workspace.persistent_cli();
    let uid = std::fs::metadata(persistent.sockets()).expect("meta").uid();
    // Private, as the CLI requires, but not writable: no socket binds.
    let dir = persistent.sockets().join(format!("octocode-{uid}"));
    std::fs::create_dir(&dir).expect("socket dir");
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o500)).expect("chmod");
    let query = r#"{"queries":[{"matchString": ["alpha"],"path":"."}]}"#;
    let in_process = workspace
        .cli()
        .args(["localSearch", query, "--json"])
        .output()
        .expect("in process");
    let fallback = persistent
        .cli()
        .args(["localSearch", query, "--json"])
        .output()
        .expect("fallback");
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).expect("chmod");
    assert_eq!(
        fallback.status.code(),
        in_process.status.code(),
        "{fallback:?}"
    );
    assert_eq!(
        String::from_utf8_lossy(&fallback.stdout),
        String::from_utf8_lossy(&in_process.stdout)
    );
    let stderr = String::from_utf8_lossy(&fallback.stderr);
    let note = stderr.lines().next().unwrap_or_default();
    assert!(
        note.starts_with("# octocode: warm server did not start (cannot bind ")
            && note.ends_with("; running localSearch in process."),
        "{stderr}"
    );
    assert_eq!(
        stderr.matches("did not start").count(),
        1,
        "named once: {stderr}"
    );
    assert!(persistent.server_pids().is_empty(), "no server");
}

/// A server that cannot announce it is ready (its client is gone) leaves
/// before it serves, and takes its socket with it.
#[test]
fn a_server_whose_client_is_gone_does_not_serve() {
    let workspace = Workspace::new();
    let persistent = workspace.persistent_cli();
    let socket = persistent.sockets().join("serve-gone.sock");
    let (reader, writer) = std::io::pipe().expect("channel");
    drop(reader);
    let status = persistent
        .cli()
        .args(["serve", "--socket"])
        .arg(&socket)
        .stdin(std::process::Stdio::null())
        .stdout(writer)
        .stderr(std::process::Stdio::null())
        .status()
        .expect("serve");
    assert_eq!(status.code(), Some(1), "{status:?}");
    assert!(!socket.exists(), "its socket is gone");
    assert!(persistent.server_pids().is_empty(), "no server");
}

/// The sockets of the servers under `sockets`.
fn server_sockets(sockets: &std::path::Path) -> Vec<std::path::PathBuf> {
    std::fs::read_dir(sockets)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|user| std::fs::read_dir(user.path()).ok())
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("serve-") && name.ends_with(".sock"))
        })
        .collect()
}

/// A variable nothing reads keys nothing: two calls that differ only in an
/// `OCTOCODE_` variable no setting declares reach one server.
#[test]
fn calls_differing_only_in_an_unread_variable_share_a_server() {
    let workspace = Workspace::new();
    workspace.write("lib.rs", "fn alpha() {}\n");
    let persistent = workspace.persistent_cli();
    let query = r#"{"queries":[{"matchString": ["alpha"],"path":"."}]}"#;
    for value in ["one", "two"] {
        let output = persistent
            .cli()
            .env("OCTOCODE_UNREAD_BY_ANYTHING", value)
            .args(["localSearch", query, "--json"])
            .output()
            .expect("localSearch");
        assert_eq!(output.status.code(), Some(0), "{output:?}");
    }
    assert_eq!(persistent.server_pids().len(), 1, "one server for both");
    assert_eq!(server_sockets(persistent.sockets()).len(), 1);
    assert!(persistent.stop(), "the test's server is stopped");
}

/// A hard link to the binary is the same build: its calls reach the server
/// the binary started. (The link lives beside the binary, on its
/// filesystem, and goes when the test ends.)
#[test]
fn a_hard_linked_binary_shares_the_server_of_the_binary() {
    struct Link(std::path::PathBuf);
    impl Drop for Link {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let workspace = Workspace::new();
    workspace.write("lib.rs", "fn alpha() {}\n");
    let persistent = workspace.persistent_cli();
    let query = r#"{"queries":[{"matchString": ["alpha"],"path":"."}]}"#;
    let started = persistent
        .cli()
        .args(["localSearch", query, "--json"])
        .output()
        .expect("starts the server");
    assert_eq!(started.status.code(), Some(0), "{started:?}");
    let exe = std::path::PathBuf::from(persistent.cli().get_program());
    let link = Link(exe.with_file_name(format!("octocode-hardlink-{}", std::process::id())));
    std::fs::hard_link(&exe, &link.0).expect("hard link beside the binary");
    // The same environment and directory as the binary's own calls.
    let command = persistent.cli();
    let linked = std::process::Command::new(&link.0)
        .args(["localSearch", query, "--json"])
        .env_clear()
        .envs(
            command
                .get_envs()
                .filter_map(|(name, value)| Some((name, value?))),
        )
        .current_dir(command.get_current_dir().expect("workspace"))
        .output()
        .expect("linked call");
    assert_eq!(linked.status.code(), Some(0), "{linked:?}");
    assert_eq!(
        server_sockets(persistent.sockets()).len(),
        1,
        "the linked binary found the binary's server"
    );
    assert_eq!(persistent.server_pids().len(), 1);
    assert!(persistent.stop(), "the test's server is stopped");
}

/// A server of another build (its socket carries another key) is left
/// alone by a new client, which starts its own, and every server stops on a
/// cache clear (`stop_all`), the old one too.
#[test]
fn a_server_under_another_key_is_ignored_by_clients_and_stopped_by_stop_all() {
    use std::io::BufRead;
    let workspace = Workspace::new();
    workspace.write("lib.rs", "fn alpha() {}\n");
    let persistent = workspace.persistent_cli();
    let query = r#"{"queries":[{"matchString": ["alpha"],"path":"."}]}"#;
    let started = persistent
        .cli()
        .args(["localSearch", query, "--json"])
        .output()
        .expect("starts the server");
    assert_eq!(started.status.code(), Some(0), "{started:?}");
    let current = server_sockets(persistent.sockets());
    assert_eq!(current.len(), 1);
    let old = current[0].with_file_name("serve-0000000000000000.sock");
    let mut child = persistent
        .cli()
        .args(["serve", "--socket"])
        .arg(&old)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("the old server");
    let mut announced = String::new();
    std::io::BufReader::new(child.stdout.take().expect("channel"))
        .read_line(&mut announced)
        .expect("startup line");
    assert!(announced.contains("ready"), "{announced}");
    assert_eq!(persistent.server_pids().len(), 2);
    let asked = |socket: &std::path::Path| std::fs::metadata(socket).and_then(|m| m.modified());
    let before = asked(&old).expect("old socket");
    for _ in 0..2 {
        let served = persistent
            .cli()
            .args(["localSearch", query, "--json"])
            .output()
            .expect("localSearch");
        assert_eq!(served.status.code(), Some(0), "{served:?}");
    }
    assert_eq!(asked(&old).expect("old socket"), before, "never asked");
    let cleared = persistent
        .cli()
        .args(["cache", "clear"])
        .output()
        .expect("cache clear");
    assert!(cleared.status.success(), "{cleared:?}");
    let _ = child.wait();
    assert!(persistent.server_pids().is_empty(), "every server stopped");
}

/// Clients that differ only in what a call carries to language servers (`PATH`,
/// `LANG`, a toolchain variable) share one server: its runtimes are told
/// apart by the call's whole environment.
#[test]
fn clients_differing_only_in_call_environment_share_a_server() {
    let workspace = Workspace::new();
    workspace.write("lib.rs", "fn alpha() {}\n");
    let persistent = workspace.persistent_cli();
    let query = r#"{"queries":[{"matchString": ["alpha"],"path":"."}]}"#;
    for (path, lang, gopath) in [
        ("/usr/bin:/bin", "C", "/go/a"),
        ("/bin:/usr/bin:/usr/local/bin", "en_US.UTF-8", "/go/b"),
    ] {
        let output = persistent
            .cli()
            .env("PATH", path)
            .env("LANG", lang)
            .env("GOPATH", gopath)
            .args(["localSearch", query, "--json"])
            .output()
            .expect("localSearch");
        assert_eq!(output.status.code(), Some(0), "{output:?}");
    }
    assert_eq!(persistent.server_pids().len(), 1, "one server for both");
    assert_eq!(server_sockets(persistent.sockets()).len(), 1);
    assert!(persistent.stop(), "the test's server is stopped");
}

/// A server started under one `PATH` finds a language server on the `PATH`
/// of the client it answers, and starts it under that `PATH`: discovery and
/// start read the call's environment, never the server process's.
#[test]
fn a_client_with_another_path_discovers_and_starts_servers_under_its_own() {
    use std::os::unix::fs::PermissionsExt;
    let workspace = Workspace::new();
    workspace.write("a.py", "x = 1\n");
    let bin = workspace.home.join("client-b-bin");
    std::fs::create_dir_all(&bin).unwrap();
    let marker = workspace.home.join("server-path");
    let script = bin.join("pyright-langserver");
    // The marker is the `PATH` the language server was started with.
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\necho \"$PATH\" > '{}'\nexec cat > /dev/null\n",
            marker.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let persistent = workspace.persistent_cli();
    let query = r#"{"queries":[{"matchString": ["x"],"path":"."}]}"#;
    let started = persistent
        .cli()
        .args(["localSearch", query, "--json"])
        .output()
        .expect("starts the server under PATH A");
    assert_eq!(started.status.code(), Some(0), "{started:?}");
    let path_b = format!("/usr/bin:/bin:{}", bin.display());
    let lsp = format!(
        r#"{{"queries":[{{"operation":"documentSymbols","path":"{}"}}]}}"#,
        workspace.workspace.join("a.py").display()
    );
    let served = persistent
        .cli()
        .env("PATH", &path_b)
        .args(["lspSearch", &lsp, "--json"])
        .output()
        .expect("lspSearch under PATH B");
    assert_eq!(served.status.code(), Some(0), "{served:?}");
    assert_eq!(
        persistent.server_pids().len(),
        1,
        "one warm server for both"
    );
    let seen = std::fs::read_to_string(&marker).expect("B's server started");
    assert_eq!(seen.trim(), path_b, "started under the caller's PATH");
    assert!(persistent.stop(), "the test's server is stopped");
}

/// A language server that answers the handshake and counts its launches in
/// `marker`, one line each.
#[cfg(unix)]
fn counting_language_server(bin: &std::path::Path, marker: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(bin).expect("bin");
    let script = bin.join("pyright-langserver");
    std::fs::write(
        &script,
        format!(
            r##"#!/bin/sh
echo started >> '{}'
reply() {{
  printf 'Content-Length: %d\r\n\r\n%s' "${{#1}}" "$1"
}}
while IFS= read -r header; do
  header=$(printf %s "$header" | tr -d '\r')
  case "$header" in
    Content-Length:*) length=${{header#Content-Length: }} ;;
    "")
      body=$(dd bs=1 count="$length" 2>/dev/null)
      method=$(printf %s "$body" | sed -n 's/.*"method":"\([^"]*\)".*/\1/p')
      id=$(printf %s "$body" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
      [ -n "$method" ] || continue
      case "$method" in
        initialize) reply '{{"jsonrpc":"2.0","id":'"$id"',"result":{{"capabilities":{{"textDocumentSync":1,"documentSymbolProvider":true}}}}}}' ;;
        exit) exit 0 ;;
        *) [ -z "$id" ] || reply '{{"jsonrpc":"2.0","id":'"$id"',"result":null}}' ;;
      esac ;;
  esac
done
"##,
            marker.display()
        ),
    )
    .expect("server");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");
}

/// A GUI host and a terminal reach one language server through different
/// strings (`PATH` order and extras, `LANG`), and share the warm server and
/// the one language server it holds: the pool keys a server by what its
/// launch resolves to, not by the spelling of the caller's environment. A
/// client whose environment selects another toolchain (`VIRTUAL_ENV`) gets
/// its own.
#[cfg(unix)]
#[test]
fn clients_with_other_spellings_of_one_toolchain_share_one_language_server() {
    let workspace = Workspace::new();
    workspace.write("a.py", "x = 1\n");
    let bin = workspace.home.join("shared-bin");
    let marker = workspace.home.join("launches");
    counting_language_server(&bin, &marker);
    let persistent = workspace.persistent_cli();
    let lsp = format!(
        r#"{{"queries":[{{"operation":"documentSymbols","path":"{}"}}]}}"#,
        workspace.workspace.join("a.py").display()
    );
    let launches = || std::fs::read_to_string(&marker).map_or(0, |log| log.lines().count());
    let call = |path: String, lang: &str, extra: &[(&str, &str)]| {
        let mut command = persistent.cli();
        command
            .env("PATH", path)
            .env("LANG", lang)
            .args(["lspSearch", &lsp, "--json"]);
        for (name, value) in extra {
            command.env(name, value);
        }
        let output = command.output().expect("lspSearch");
        assert_eq!(output.status.code(), Some(0), "{output:?}");
    };
    let gui = || format!("/usr/bin:/bin:{}", bin.display());
    let terminal = || format!("{}:/bin:/usr/bin:/usr/local/bin", bin.display());
    for _ in 0..2 {
        call(gui(), "C", &[]);
        call(terminal(), "en_US.UTF-8", &[]);
    }
    assert_eq!(persistent.server_pids().len(), 1, "one warm server");
    assert_eq!(launches(), 1, "one language server for both clients");
    call(gui(), "C", &[("VIRTUAL_ENV", "/another/environment")]);
    assert_eq!(persistent.server_pids().len(), 1, "still one warm server");
    assert_eq!(launches(), 2, "another toolchain, its own language server");
    assert!(persistent.stop(), "the test's server is stopped");
}
