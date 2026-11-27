use super::{Workspace, compact_line, search, stderr, stdout};
use serde_json::Value;
use std::io::Read;
use std::os::fd::{FromRawFd, OwnedFd};
use std::process::{Command, Output, Stdio};

/// Which of the child's streams is the terminal.
#[derive(Clone, Copy)]
enum Tty {
    Stdout,
    Stderr,
}

/// Run `command` with one stream on a fresh pseudo-terminal: its output,
/// and what the terminal showed.
fn tty(mut command: Command, stream: Tty) -> (Output, String) {
    let (mut master, mut slave) = (-1, -1);
    // SAFETY: both out-pointers are valid for writes; the name, termios
    // and window-size arguments may be null.
    let opened = unsafe {
        libc::openpty(
            &raw mut master,
            &raw mut slave,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    assert_eq!(opened, 0, "openpty: {}", std::io::Error::last_os_error());
    // Parallel tests spawn unrelated processes. They must not inherit either
    // terminal end and keep this reader alive after its own child exits.
    for descriptor in [master, slave] {
        // SAFETY: openpty returned valid descriptors; F_SETFD takes an integer flag.
        let result = unsafe { libc::fcntl(descriptor, libc::F_SETFD, libc::FD_CLOEXEC) };
        assert_eq!(
            result,
            0,
            "close-on-exec: {}",
            std::io::Error::last_os_error()
        );
    }
    // SAFETY: `openpty` succeeded, so both are open descriptors that
    // nothing else owns.
    let (master, slave) = unsafe { (OwnedFd::from_raw_fd(master), OwnedFd::from_raw_fd(slave)) };
    // Drain while the child writes: a terminal buffers little.
    let reader = std::thread::spawn(move || {
        let mut master = std::fs::File::from(master);
        let mut shown = Vec::new();
        let mut buffer = [0_u8; 4096];
        // Once every slave end is closed, a read ends (0 or EIO).
        while let Ok(read @ 1..) = master.read(&mut buffer) {
            shown.extend_from_slice(&buffer[..read]);
        }
        shown
    });
    match stream {
        Tty::Stdout => command.stdout(Stdio::from(slave)),
        Tty::Stderr => command.stderr(Stdio::from(slave)),
    };
    let output = command.output().expect("run on a terminal");
    // The command holds the last parent copy of the slave end.
    drop(command);
    let shown = reader.join().expect("terminal reader");
    let shown = String::from_utf8(shown).expect("UTF-8 terminal");
    (output, shown.replace("\r\n", "\n"))
}

/// JSON on a terminal is indented, for every JSON command, `--json` or not.
#[test]
fn terminal_json_is_indented() {
    let (workspace, query) = search();
    for args in [
        vec!["localSearch", query, "--json"],
        vec!["schema"],
        vec!["schema", "localSearch", "--view", "query", "--json"],
        vec!["config", "--json"],
        vec!["config", "home", "--json"],
        vec!["install", "--list", "--json"],
    ] {
        let mut command = workspace.cli();
        command.args(&args);
        let (output, shown) = tty(command, Tty::Stdout);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{args:?}: {}",
            stderr(&output)
        );
        let value: Value = serde_json::from_str(&shown).expect("JSON on the terminal");
        let piped = workspace.cli().args(&args).output().expect("piped JSON");
        assert!(piped.status.success(), "{args:?}: {}", stderr(&piped));
        assert_eq!(
            value,
            serde_json::from_slice::<Value>(&piped.stdout).expect("same JSON in a pipe"),
            "terminal formatting must preserve every value: {args:?}"
        );
        assert!(
            shown.lines().any(|line| {
                line.starts_with(char::is_whitespace) && line.trim_start().starts_with('"')
            }),
            "terminal object fields must be indented: {args:?}: {shown}"
        );
    }
}

/// A JSON error on a terminal (`--json`) is indented too, and stays on
/// stdout: the person asked for JSON.
#[test]
fn explicit_json_errors_on_a_terminal_are_indented_on_stdout() {
    let workspace = Workspace::new();
    let mut command = workspace.cli();
    command.args(["localSearch", r#"{"queries":["#, "--json"]);
    let (output, shown) = tty(command, Tty::Stdout);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).is_empty(), "{}", stderr(&output));
    let envelope: Value = serde_json::from_str(&shown).expect("JSON error");
    assert_eq!(envelope["errorCode"], "invalidInput");
    assert!(shown.lines().count() > 1, "{shown}");
}

/// When a pipe (not `--json`) chose JSON and a person watches stderr, the
/// error is also told there as text; stdout keeps the same JSON line.
#[test]
fn pipe_implied_json_errors_are_also_told_to_a_terminal_stderr() {
    let (workspace, _) = search();
    // Only `schema` answers a pipe in JSON; tools print text there.
    {
        let (args, told) = (vec!["schema", "noSuchTool"], "Unknown tool: noSuchTool");
        let piped = workspace.cli().args(&args).output().expect("piped");
        let mut command = workspace.cli();
        command.args(&args);
        let (output, shown) = tty(command, Tty::Stderr);
        assert_eq!(output.status.code(), piped.status.code(), "{args:?}");
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert_eq!(stdout(&output), stdout(&piped), "{args:?}");
        let envelope = compact_line(stdout(&output));
        assert!(envelope["errorCode"].is_string(), "{args:?}: {envelope}");
        assert!(shown.contains(told), "{args:?}: {shown}");
        assert!(!shown.contains("\"errorCode\""), "{args:?}: {shown}");
        // A piped stderr gets nothing, as before.
        assert!(stderr(&piped).is_empty(), "{args:?}: {}", stderr(&piped));

        // `--json` asked for JSON: no copy on the terminal.
        let mut explicit = workspace.cli();
        explicit.args(&args).arg("--json");
        let (output, shown) = tty(explicit, Tty::Stderr);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        compact_line(stdout(&output));
        assert!(shown.is_empty(), "{args:?} --json: {shown}");
    }
}

const BUILD_APP: &str = r#"{"queries":[{"path":"app"}]}"#;

/// `graphBuild` reports each stage on a terminal stderr as it goes, a `# `
/// note per line as every stderr line of a tool call is; stdout stays the one
/// JSON result, so a pipe reads it unchanged. Progress is for a person
/// watching: a piped stderr, which a program reads only when the call ends,
/// gets none.
#[test]
fn graph_build_reports_progress_on_a_terminal_stderr_and_the_result_on_stdout() {
    let workspace = Workspace::new();
    workspace.write(".git/HEAD", "ref: refs/heads/main\n");
    workspace.write(
        "app/lib.ts",
        "export function f() { return g(); }\nfunction g() {}\n",
    );
    workspace.write(
        "app/main.ts",
        "import { f } from './lib';\nexport const main = () => f();\n",
    );
    let build = |query: &str| {
        let mut command = workspace.cli();
        command.args(["__graph", "graphBuild", "--json", query]);
        let (output, shown) = tty(command, Tty::Stderr);
        assert_eq!(output.status.code(), Some(0), "{shown}");
        let lines = shown.lines().map(str::to_owned).collect::<Vec<_>>();
        (compact_line(stdout(&output)), lines)
    };
    let piped = workspace
        .cli()
        .args(["__graph", "graphBuild", "--json", BUILD_APP])
        .output()
        .expect("piped graphBuild");
    assert_eq!(piped.status.code(), Some(0), "{}", stderr(&piped));
    assert!(
        !stderr(&piped).contains("# graphBuild: "),
        "no progress on a pipe: {}",
        stderr(&piped)
    );
    compact_line(stdout(&piped));
    let (receipt, lines) = build(r#"{"queries":[{"path":"app","refresh":true}]}"#);
    let id = receipt
        .pointer("/results/0/data/graph")
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("snapshot id: {receipt}"));
    assert!(
        lines.iter().all(|line| line.starts_with("# graphBuild: ")),
        "{lines:?}"
    );
    // Progress names the scope like every output path: workspace-relative.
    assert!(
        lines
            .iter()
            .any(|line| line == "# graphBuild: scanning app"),
        "{lines:?}"
    );
    let mut after = 0;
    for stage in [
        "scanning ",
        "parsing 2 files",
        "parsed 2/2 files",
        "linked 2 files",
        "encoding snapshot",
        "published ",
    ] {
        let at = lines[after..]
            .iter()
            .position(|line| line.contains(stage))
            .unwrap_or_else(|| panic!("stage {stage:?} after line {after} in {lines:?}"));
        after += at + 1;
    }
    assert!(
        lines.last().is_some_and(|line| line.contains(id)),
        "{lines:?}"
    );
    // An unchanged scope answers with its current snapshot: the same id,
    // its progress naming the reuse, the result still on stdout.
    let (again_result, again_lines) = build(BUILD_APP);
    let again_id = again_result
        .pointer("/results/0/data/graph")
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("snapshot id: {again_result}"));
    assert_eq!(again_id, id, "{again_result}");
    assert!(
        again_lines
            .iter()
            .all(|line| line.starts_with("# graphBuild: ") && !line.contains("parsed ")),
        "{again_lines:?}"
    );
    assert!(
        again_lines
            .last()
            .is_some_and(|line| line.contains("reused ") && line.contains(id)),
        "{again_lines:?}"
    );
    // `refresh` builds again: a new snapshot, the full progress.
    let (refreshed_result, refreshed_lines) =
        build(r#"{"queries":[{"path":"app","refresh":true}]}"#);
    let refreshed_id = refreshed_result
        .pointer("/results/0/data/graph")
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("snapshot id: {refreshed_result}"));
    assert_ne!(refreshed_id, id, "{refreshed_result}");
    assert!(
        refreshed_lines
            .iter()
            .any(|line| line.contains("parsed 2/2 files")),
        "{refreshed_lines:?}"
    );
    assert!(
        refreshed_lines
            .last()
            .is_some_and(|line| line.contains("published ") && line.contains(refreshed_id)),
        "{refreshed_lines:?}"
    );
}
