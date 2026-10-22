//! What the CLI prints, by stream: JSON is indented for a terminal and one
//! compact line on a pipe; errors keep their whole message and one envelope;
//! a pipe-implied JSON error is also told to a person watching stderr.
//!
//! A terminal is a pseudo-terminal from `openpty`, given to the child as its
//! stdout or stderr. The terminal turns `\n` into `\r\n`; [`tty`] undoes it.

use crate::support::Workspace;
use serde_json::Value;
use std::process::Output;

fn stdout(output: &Output) -> &str {
    std::str::from_utf8(&output.stdout).expect("UTF-8 stdout")
}

fn stderr(output: &Output) -> &str {
    std::str::from_utf8(&output.stderr).expect("UTF-8 stderr")
}

/// The one compact JSON line a pipe gets, parsed back.
fn compact_line(text: &str) -> Value {
    let line = text.strip_suffix('\n').expect("newline-terminated");
    assert!(!line.contains('\n'), "one line: {text}");
    let value: Value = serde_json::from_str(line).expect("JSON line");
    assert_eq!(serde_json::to_string(&value).expect("compact"), line);
    value
}

/// A search hit in a fresh workspace: the query and its workspace.
fn search() -> (Workspace, &'static str) {
    let workspace = Workspace::new();
    workspace.write("notes.txt", "alpha needle omega\n");
    (
        workspace,
        r#"{"queries":[{"path":"notes.txt","matchString":"needle"}]}"#,
    )
}

/// Every JSON command answers a pipe with one compact line, as before.
#[test]
fn piped_json_is_one_compact_line() {
    let (workspace, query) = search();
    for args in [
        vec!["localSearch", query, "--json"],
        vec!["schema"],
        vec!["schema", "localSearch", "--view", "query", "--json"],
        vec!["config", "--json"],
        vec!["config", "home", "--json"],
        vec!["install", "--list", "--json"],
    ] {
        let output = workspace.cli().args(&args).output().expect("run");
        assert_eq!(
            output.status.code(),
            Some(0),
            "{args:?}: {}",
            stderr(&output)
        );
        compact_line(stdout(&output));
    }
}

/// A tool prints text on a pipe, as rg and gh do: hits as `path:line:text`
/// on stdout, counts and leads as `# ` notes on stderr. Flags and the JSON
/// query print the same lines.
#[test]
fn tools_print_rg_style_lines_on_a_pipe() {
    let workspace = Workspace::new();
    workspace.write("notes.txt", "alpha\nneedle here\nomega\n");
    let flags = workspace
        .cli()
        .args(["localSearch", "needle", ".", "-C", "1"])
        .output()
        .expect("flags");
    assert_eq!(flags.status.code(), Some(0), "{}", stderr(&flags));
    assert_eq!(
        stdout(&flags),
        "notes.txt-1-alpha\nnotes.txt:2:needle here\nnotes.txt-3-omega\n"
    );
    assert!(
        stderr(&flags).contains("# matchCount: 1"),
        "{}",
        stderr(&flags)
    );
    let json = workspace
        .cli()
        .args([
            "localSearch",
            r#"{"path":".","matchString":"needle","contextLines":1}"#,
        ])
        .output()
        .expect("json query");
    assert_eq!(stdout(&json), stdout(&flags));
    let files = workspace
        .cli()
        .args(["localSearch", "needle", "-l"])
        .output()
        .expect("files");
    assert_eq!(stdout(&files), "notes.txt\n");
}

/// `--bool false` with a space is refused in the real parser, not read as
/// a path.
#[test]
fn a_spaced_boolean_value_is_refused() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .args(["localSearch", "foo", "--wholeWord", "false"])
        .output()
        .expect("run");
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert!(
        stderr(&output).contains("--wholeWord=false"),
        "{}",
        stderr(&output)
    );
}

/// `skill --json` answers a pipe like every other JSON command.
#[test]
fn piped_skill_json_is_one_compact_line() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .env("PATH", workspace.home.join("no-executables"))
        .args(["skill", "list", "--json"])
        .output()
        .expect("skill list");
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let catalog = compact_line(stdout(&output));
    assert!(catalog["skills"].is_array(), "{catalog}");
}

/// A skill error is the `{error, errorCode}` envelope every command uses.
#[test]
fn skill_errors_use_the_common_envelope() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .env("PATH", workspace.home.join("no-executables"))
        .args(["skill", "info", "no-such-skill", "--json"])
        .output()
        .expect("skill info");
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).is_empty(), "{}", stderr(&output));
    let envelope = compact_line(stdout(&output));
    let keys: Vec<&str> = envelope
        .as_object()
        .expect("object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys, ["error", "errorCode"], "{envelope}");
    assert_eq!(envelope["errorCode"], "invalidInput");
    assert!(
        envelope["error"]
            .as_str()
            .is_some_and(|error| error.contains("no-such-skill")),
        "{envelope}"
    );
}

/// A JSON argument error keeps every line clap wrote: the error line and
/// its did-you-mean tip are `error`, the rest (possible values, usage) are
/// `details`.
#[test]
fn json_argument_errors_keep_the_whole_clap_message() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .args(["graph", "query", "notAnOp", "--json"])
        .output()
        .expect("graph query");
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).is_empty(), "{}", stderr(&output));
    let envelope = compact_line(stdout(&output));
    assert_eq!(envelope["errorCode"], "invalidInput");
    assert_eq!(envelope["error"], "invalid value 'notAnOp' for '<OP>'");
    let details: Vec<&str> = envelope["details"]
        .as_array()
        .expect("details")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(
        details
            .iter()
            .any(|line| line.starts_with("[possible values: stats, ")),
        "{envelope}"
    );
    assert!(
        details.iter().any(|line| line.contains("--help")),
        "{envelope}"
    );

    let mistyped = workspace
        .cli()
        .args(["localSerch", "--json"])
        .output()
        .expect("mistyped tool");
    let envelope = compact_line(stdout(&mistyped));
    let error = envelope["error"].as_str().expect("error");
    assert!(
        error.starts_with("unrecognized subcommand 'localSerch'; tip: ")
            && error.contains("'localSearch'"),
        "{envelope}"
    );
    let details = envelope["details"].to_string();
    assert!(details.contains("Usage: octocode"), "{envelope}");
    assert!(details.contains("--help"), "{envelope}");
}

#[cfg(unix)]
mod terminal {
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
        // SAFETY: `openpty` succeeded, so both are open descriptors that
        // nothing else owns.
        let (master, slave) =
            unsafe { (OwnedFd::from_raw_fd(master), OwnedFd::from_raw_fd(slave)) };
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
            assert_eq!(
                shown,
                format!(
                    "{}\n",
                    serde_json::to_string_pretty(&value).expect("pretty")
                ),
                "{args:?}"
            );
            assert!(shown.lines().count() > 1, "{args:?}: {shown}");
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
}
