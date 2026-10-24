//! A process tree built by this test binary re-executing itself, so a test
//! needs no interpreter and no `setsid` binary (macOS has none, Windows has
//! neither). The tree is:
//!
//! ```text
//! leader ── middle (exits at once) ── holder
//! ```
//!
//! The holder inherits the leader's stdio, so it keeps the output pipes open.
//! A silent holder is `sleep` on Unix: it never writes, so it cannot die of
//! a pipe the call has already closed. A re-executed test binary prints its
//! harness banner first, and under load it printed it after the call had
//! returned, died of the broken pipe and no longer held anything.
//! It is reparented when `middle` exits, and with `detach` it also leaves the
//! process group (`setsid`, Unix only): nothing can find it by walking
//! parents or by group, which leaves only a Windows job to contain it.
//! Without `detach` it stays in the group, so Unix reaches it by group kill
//! and Windows by the job: one assertion, "no survivor", holds on both.
use std::{
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, Instant},
};

const ROLE: &str = "OCTOCODE_FIXTURE_ROLE";
const HOLDER: &str = "OCTOCODE_FIXTURE_HOLDER";
const DETACH: &str = "OCTOCODE_FIXTURE_DETACH";
const PIDFILE: &str = "OCTOCODE_FIXTURE_PIDFILE";
/// Long enough that only a bounded wait can return before it ends.
pub(crate) const HOLD: Duration = Duration::from_secs(30);
/// How long past its timeout (or its command's end) a call may take to
/// kill, reap and drain on a loaded machine: well under [`HOLD`] past the
/// longest [`until_set_up`] timeout, so a call a holder delays still fails.
pub(crate) const RETURN_SLACK: Duration = Duration::from_secs(8);

/// What the leader does once `middle` has started the holder.
#[derive(Clone, Copy)]
pub(crate) enum Leader {
    /// Prints `hello` and exits.
    Exit,
    /// Prints `hello` and sleeps for [`HOLD`].
    Sleep,
}

/// What the holder does with the pipes it inherited.
#[derive(Clone, Copy)]
pub(crate) enum Holder {
    /// Holds them open and writes nothing.
    Silent,
    /// Writes as fast as it can, until a write fails (the read end closed).
    Streaming,
}

/// The file that receives the holder's pid, and the directory it lives in.
pub(crate) struct Tree {
    pidfile: PathBuf,
    _dir: tempfile::TempDir,
}

impl Tree {
    /// The command that starts the tree, and the tree's handle.
    pub(crate) fn new(leader: Leader, holder: Holder, detach: bool) -> (Command, Self) {
        let dir = tempfile::tempdir().expect("fixture dir");
        let pidfile = dir.path().join("holder.pid");
        let mut command = reexec(
            match leader {
                Leader::Exit => "leader-exit",
                Leader::Sleep => "leader-sleep",
            },
            &pidfile,
        );
        command
            .env(
                HOLDER,
                match holder {
                    Holder::Silent => "holder",
                    Holder::Streaming => "streamer",
                },
            )
            .env(DETACH, if detach { "1" } else { "0" });
        (command, Self { pidfile, _dir: dir })
    }

    /// The holder's pid when `middle` has recorded it by now.
    pub(crate) fn recorded_pid(&self) -> Option<u32> {
        recorded_pid(&self.pidfile)
    }

    /// The holder's pid, once `middle` has recorded it.
    pub(crate) fn holder_pid(&self) -> u32 {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(pid) = self.recorded_pid() {
                return pid;
            }
            assert!(
                Instant::now() < deadline,
                "the holder never recorded its pid"
            );
            thread::sleep(Duration::from_millis(20));
        }
    }
}

/// The pid a fixture wrote to `pidfile`; `None` when it wrote none (or not
/// all of it).
pub(crate) fn recorded_pid(pidfile: &Path) -> Option<u32> {
    std::fs::read_to_string(pidfile).ok()?.trim().parse().ok()
}

/// The first of `attempt`'s runs, one per timeout in `timeouts`, that set
/// its fixture up before the timeout fired. A run's deadline starts at
/// spawn, and under load it can fire before the command started the
/// descendant the test checks: such a run tests nothing (`None`), so the
/// next runs with a longer timeout.
pub(crate) fn until_set_up<T>(
    timeouts: &[Duration],
    mut attempt: impl FnMut(Duration) -> Option<T>,
) -> T {
    timeouts
        .iter()
        .find_map(|timeout| attempt(*timeout))
        .unwrap_or_else(|| panic!("the fixture never set up within any of {timeouts:?}"))
}

/// Polls until `pid` is gone; `false` when it is still alive after `within`.
pub(crate) fn gone_within(pid: u32, within: Duration) -> bool {
    let deadline = Instant::now() + within;
    while crate::process_status::is_alive(pid) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    !crate::process_status::is_alive(pid)
}

/// Ends a survivor so a failed assertion does not leave it running.
pub(crate) fn kill(pid: u32) {
    #[cfg(unix)]
    // SAFETY: the fixture recorded the pid of its own detached child.
    unsafe {
        libc::kill(pid as i32, libc::SIGKILL);
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Threading::{
            OpenProcess, PROCESS_TERMINATE, TerminateProcess,
        };
        // SAFETY: the handle is owned here and closed exactly once.
        unsafe {
            let handle = OpenProcess(PROCESS_TERMINATE, 0, pid);
            if !handle.is_null() {
                TerminateProcess(handle, 1);
                CloseHandle(handle);
            }
        }
    }
}

fn reexec(role: &str, pidfile: &Path) -> Command {
    // `module_path!` names this module under the crate; the test filter does not.
    let test = module_path!()
        .split_once("::")
        .map_or(String::new(), |(_, path)| format!("{path}::fixture"));
    let mut command = Command::new(std::env::current_exe().expect("test binary"));
    command
        .args(["--exact", &test, "--nocapture", "--test-threads=1"])
        .env(ROLE, role)
        .env(PIDFILE, pidfile);
    command
}

/// The re-executed entry point. Without a role it is an ordinary passing test.
#[test]
fn fixture() {
    let Ok(role) = std::env::var(ROLE) else {
        return;
    };
    let pidfile = PathBuf::from(std::env::var(PIDFILE).expect("pidfile"));
    let holder = || std::env::var(HOLDER).expect("holder role");
    match role.as_str() {
        "leader-exit" | "leader-sleep" => {
            let mut middle = reexec("middle", &pidfile);
            middle
                .env(HOLDER, holder())
                .env(DETACH, std::env::var(DETACH).expect("detach"));
            middle.spawn().expect("middle").wait().expect("wait");
            print!("hello");
            std::io::stdout().flush().expect("flush");
            if role == "leader-sleep" {
                thread::sleep(HOLD);
            }
        }
        "middle" => {
            let mut command = match holder().as_str() {
                #[cfg(unix)]
                "holder" => {
                    let mut sleep = Command::new("sleep");
                    sleep.arg(HOLD.as_secs().to_string());
                    sleep
                }
                role => reexec(role, &pidfile),
            };
            #[cfg(unix)]
            if std::env::var(DETACH).as_deref() == Ok("1") {
                use std::os::unix::process::CommandExt;
                // SAFETY: setsid is async-signal-safe and runs before exec in the forked child.
                unsafe {
                    command.pre_exec(|| {
                        libc::setsid();
                        Ok(())
                    });
                }
            }
            let holder = command.spawn().expect("holder");
            std::fs::write(&pidfile, holder.id().to_string()).expect("pidfile");
            // Left running on purpose: it is the process the tests look for.
            drop(holder);
        }
        // The silent holder where there is no `sleep` (Windows).
        "holder" => thread::sleep(HOLD),
        "streamer" => {
            let deadline = Instant::now() + HOLD;
            let mut out = std::io::stdout();
            let chunk = vec![b'x'; 64 * 1024];
            while Instant::now() < deadline {
                // The read end closed: the pipe is broken.
                if out.write_all(&chunk).is_err() {
                    return;
                }
            }
        }
        other => panic!("unknown fixture role {other}"),
    }
}
