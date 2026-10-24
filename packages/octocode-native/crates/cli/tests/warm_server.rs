//! The warm tool server end to end: the one integration test whose tool
//! calls go through `octocode serve` (every other test runs them in process).
use crate::support::Workspace;

/// The server processes started under `sockets` (their socket path names it).
fn servers_under(sockets: &std::path::Path) -> usize {
    let listed = std::process::Command::new("ps")
        .args(["-axo", "args"])
        .output()
        .expect("ps");
    String::from_utf8_lossy(&listed.stdout)
        .lines()
        .filter(|line| {
            line.contains(" serve --socket ") && line.contains(&*sockets.to_string_lossy())
        })
        .count()
}

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
    // A short private temp dir: the socket path must fit `sockaddr_un`.
    let sockets = tempfile::Builder::new()
        .prefix("oc")
        .tempdir_in("/tmp")
        .expect("socket dir");
    let query = r#"{"queries":[{"matchString": ["alpha"],"path":"."}]}"#;
    let run = |served: bool| {
        let mut command = workspace.cli();
        command
            .env("TMPDIR", sockets.path())
            .args(["localSearch", query]);
        if served {
            command.env("OCTOCODE_STORAGE_MODE", "persistent");
        }
        command.output().expect("localSearch")
    };
    let in_process = run(false);
    assert_eq!(in_process.status.code(), Some(0), "{in_process:?}");
    assert_eq!(
        servers_under(sockets.path()),
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
        let now_asked = last_asked(sockets.path());
        assert!(now_asked.is_some(), "{call}: a server socket");
        assert!(now_asked > asked, "{call}: the server was asked");
        asked = now_asked;
    }
    assert_eq!(servers_under(sockets.path()), 1, "one server answered both");
    let _ = std::process::Command::new("pkill")
        .args(["-f", &*sockets.path().to_string_lossy()])
        .status();
    let stopped = (0..100).any(|_| {
        std::thread::sleep(std::time::Duration::from_millis(20));
        servers_under(sockets.path()) == 0
    });
    assert!(stopped, "the test's server is stopped");
}
