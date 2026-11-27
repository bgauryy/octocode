//! The Windows job, run for real: this test binary re-executes itself as the
//! contained child, which starts a grandchild once the job holds it (so the
//! grandchild is born inside the job).
use super::*;
use crate::test_support::{process_alive, process_gone_within};
use std::{
    io::{BufRead, Write},
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

const ROLE: &str = "OCTOCODE_JOB_FIXTURE_ROLE";
const PIDFILE: &str = "OCTOCODE_JOB_FIXTURE_PIDFILE";
const HOLD: Duration = Duration::from_secs(60);
const CAP_MIB: u64 = 256;
/// Exit code of the fixture whose allocation the job refused.
const REFUSED: i32 = 3;

fn reexec(role: &str, pidfile: &Path) -> Command {
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

/// Waits for the test's go-ahead, sent once the job holds this process.
fn await_go_ahead() {
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line).expect("go");
}

/// The re-executed entry point. Without a role it is an ordinary passing test.
#[test]
fn fixture() {
    let Ok(role) = std::env::var(ROLE) else {
        return;
    };
    let pidfile = std::path::PathBuf::from(std::env::var(PIDFILE).expect("pidfile"));
    match role.as_str() {
        "parent" => {
            await_go_ahead();
            let grandchild = reexec("idle", &pidfile).spawn().expect("grandchild");
            std::fs::write(&pidfile, grandchild.id().to_string()).expect("pidfile");
            // Left running on purpose: the job must be what ends it.
            drop(grandchild);
            std::thread::sleep(HOLD);
        }
        "idle" => std::thread::sleep(HOLD),
        "hog" => {
            await_go_ahead();
            let mut memory: Vec<u8> = Vec::new();
            if memory
                .try_reserve_exact((CAP_MIB as usize * 4) << 20)
                .is_err()
            {
                std::process::exit(REFUSED);
            }
            memory.resize(memory.capacity(), 1);
            std::hint::black_box(&memory);
        }
        other => panic!("unknown fixture role {other}"),
    }
}

#[test]
fn closing_the_job_ends_the_child_and_its_grandchild() {
    let dir = tempfile::tempdir().expect("dir");
    let pidfile = dir.path().join("grandchild.pid");
    let mut child = reexec("parent", &pidfile)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("child");
    let mut job = Job::attach(&child, None).expect("job");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(b"go\n")
        .expect("go");
    let deadline = Instant::now() + Duration::from_secs(20);
    let grandchild: u32 = loop {
        if let Some(pid) = std::fs::read_to_string(&pidfile)
            .ok()
            .and_then(|text| text.trim().parse().ok())
        {
            break pid;
        }
        assert!(Instant::now() < deadline, "the grandchild never started");
        std::thread::sleep(Duration::from_millis(20));
    };
    assert!(
        process_alive(grandchild),
        "the grandchild must be running first"
    );
    job.terminate();
    let reaped = child.wait();
    let gone = process_gone_within(grandchild, Duration::from_secs(10));
    assert!(
        reaped.is_ok_and(|status| !status.success()),
        "the child survived"
    );
    assert!(gone, "grandchild {grandchild} survived the job");
}

#[test]
fn dropping_the_job_ends_the_child() {
    let dir = tempfile::tempdir().expect("dir");
    let mut child = reexec("idle", &dir.path().join("none"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("child");
    drop(Job::attach(&child, None).expect("job"));
    let status = child.wait().expect("reaped");
    assert!(!status.success(), "the child exited by itself: {status:?}");
}

/// The cap refuses an allocation past it; it does not kill the process.
#[test]
fn the_job_memory_cap_refuses_an_allocation_past_it() {
    let dir = tempfile::tempdir().expect("dir");
    let mut child = reexec("hog", &dir.path().join("none"))
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("child");
    let _job = Job::attach(&child, Some(CAP_MIB << 20)).expect("job");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(b"go\n")
        .expect("go");
    let status = child.wait().expect("reaped");
    assert_eq!(
        status.code(),
        Some(REFUSED),
        "the allocation past the cap was not refused: {status:?}"
    );
}
