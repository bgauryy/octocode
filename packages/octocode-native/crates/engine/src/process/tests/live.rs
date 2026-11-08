//! Real process trees: CPU clocks, the tree walk and teardown.

use super::*;
use crate::process::identity::ProcessTable;
use std::time::{Duration, Instant};

fn python_available() -> bool {
    std::process::Command::new("python3")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

fn wait_for_pid_file(path: &std::path::Path) -> u32 {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(pid) = std::fs::read_to_string(path)
            .ok()
            .and_then(|text| text.trim().parse().ok())
        {
            return pid;
        }
        assert!(Instant::now() < deadline, "pid file never written");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn process_gone(pid: u32) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        // SAFETY: signal 0 only checks existence.
        let alive = unsafe { libc::kill(pid as i32, 0) } == 0;
        if !alive || rss_bytes(pid).is_none() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn spawn_shell(script: &str) -> std::process::Child {
    use std::os::unix::process::CommandExt;
    std::process::Command::new("sh")
        .args(["-c", script])
        .process_group(0)
        .spawn()
        .expect("spawn sh")
}

/// The OS read behind the clock: this thread's own work shows in this
/// process's tree. How a tree's processes add up is `cpu_clock`'s, on samples.
#[test]
fn tree_cpu_clock_reads_this_process_work() {
    let clock = TreeCpuClock::new(std::process::id());
    let before = clock.read().expect("this process is readable");
    let started = crate::test_cpu::thread_cpu_time();
    let mut spin = 0u64;
    while crate::test_cpu::thread_cpu_time() - started < Duration::from_millis(30) {
        spin = std::hint::black_box(spin.wrapping_add(1));
    }
    let after = clock.read().expect("still readable");
    assert!(after > before, "{after:?} after {before:?}");
}

/// A child of the root is read with it: an idle one adds its own time, which
/// `cpu_clock`'s samples prove is counted.
#[test]
fn tree_cpu_clock_reads_a_child_of_its_root() {
    let mut child = spawn_shell("sleep 30");
    let clock = TreeCpuClock::new(std::process::id());
    let read = clock.read();
    let walked = descendants(std::process::id())
        .iter()
        .any(|process| process.pid == child.id());
    kill_tree(child.id());
    let _ = child.wait();
    assert!(read.is_some(), "the root and its child are readable");
    assert!(walked, "the clock's walk reaches the child");
}

#[test]
fn tree_cpu_clock_is_none_for_a_gone_process() {
    let mut child = spawn_shell("exit 0");
    let pid = child.id();
    let _ = child.wait();
    assert!(TreeCpuClock::new(pid).read().is_none());
}

#[test]
fn tree_walk_and_kill_reach_a_setsid_grandchild() {
    if !python_available() {
        return;
    }
    let dir = tempfile::tempdir().expect("dir");
    let pid_file = dir.path().join("grandchild.pid");
    let script = "import os,sys,time\nif os.fork()==0:\n    os.setsid()\n    open(sys.argv[1],'w').write(str(os.getpid()))\n    time.sleep(60)\nelse:\n    time.sleep(60)\n";
    let mut child = {
        use std::os::unix::process::CommandExt;
        std::process::Command::new("python3")
            .args(["-c", script])
            .arg(&pid_file)
            .process_group(0)
            .spawn()
            .expect("spawn python")
    };
    let grandchild = wait_for_pid_file(&pid_file);
    let found = descendants(child.id());
    let seen = found
        .iter()
        .find(|process| process.pid == grandchild)
        .expect("the setsid grandchild is a descendant");
    assert_eq!(
        identity::LiveTable.start(grandchild),
        Some(seen.start),
        "the walk reads the same start time a signal is checked against"
    );
    assert!(tree_rss_bytes(child.id()).is_some_and(|rss| rss > 0));
    kill_tree(child.id());
    let _ = child.wait();
    assert!(
        process_gone(grandchild),
        "the setsid grandchild must die with the tree"
    );
}

#[test]
fn parent_pid_of_this_process_is_a_positive_pid() {
    let parent = parent_pid(std::process::id());
    assert!(parent.is_some_and(|pid| pid > 0), "{parent:?}");
}
