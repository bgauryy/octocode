//! Real process trees: CPU clocks, the tree walk and teardown.

use super::*;
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

/// CPU growth of `pid`'s tree over `window` of wall time.
fn cpu_growth(pid: u32, window: Duration) -> Duration {
    let clock = TreeCpuClock::new(pid);
    let before = clock.read().expect("readable");
    std::thread::sleep(window);
    clock.read().expect("readable").saturating_sub(before)
}

#[test]
fn tree_cpu_clock_grows_for_a_busy_server_and_not_an_idle_one() {
    let mut busy = spawn_shell("while :; do :; done");
    let mut idle = spawn_shell("sleep 60");
    let window = Duration::from_millis(400);
    let busy_growth = cpu_growth(busy.id(), window);
    let idle_growth = cpu_growth(idle.id(), window);
    kill_tree(busy.id());
    kill_tree(idle.id());
    let _ = busy.wait();
    let _ = idle.wait();
    assert!(
        busy_growth >= window / 10,
        "a spinning process burns CPU: {busy_growth:?}"
    );
    assert!(
        idle_growth < window / 20,
        "a sleeping process does not: {idle_growth:?}"
    );
}

#[test]
fn tree_cpu_clock_counts_descendants() {
    // The shell only waits; the CPU is burned by its child.
    let mut parent = spawn_shell("(while :; do :; done) & wait");
    std::thread::sleep(Duration::from_millis(200));
    let window = Duration::from_millis(400);
    let growth = cpu_growth(parent.id(), window);
    kill_tree(parent.id());
    let _ = parent.wait();
    assert!(growth >= window / 10, "child CPU is the tree's: {growth:?}");
}

/// A helper that burns CPU and exits must not make the tree's total
/// fall: a falling total would read as "no progress" to the request wait.
#[test]
fn tree_cpu_clock_never_falls_when_a_busy_descendant_exits() {
    let mut parent =
        spawn_shell("(while :; do :; done) & busy=$!; sleep 1; kill $busy; wait; sleep 60");
    let clock = TreeCpuClock::new(parent.id());
    std::thread::sleep(Duration::from_millis(700));
    let while_busy = clock.read().expect("readable");
    std::thread::sleep(Duration::from_millis(1_000));
    let after_exit = clock.read().expect("readable");
    kill_tree(parent.id());
    let _ = parent.wait();
    assert!(
        after_exit >= while_busy,
        "{after_exit:?} fell below {while_busy:?}"
    );
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
    assert!(descendants(child.id()).contains(&grandchild));
    assert!(tree_rss_bytes(child.id()).is_some_and(|rss| rss > 0));
    kill_tree(child.id());
    let _ = child.wait();
    assert!(
        process_gone(grandchild),
        "the setsid grandchild must die with the tree"
    );
}
