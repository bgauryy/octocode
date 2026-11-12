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
