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
