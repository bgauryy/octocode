//! Spawning, graceful exit, and teardown of the server process tree.

use super::*;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use crate::lsp::client::server_process::cleanup_failed_start;
use crate::lsp::client::server_process::lsp_spawn_program;
#[cfg(unix)]
use crate::lsp::client::server_process::{end_child, wait_for_graceful_exit};
#[cfg(unix)]
use tokio::time::timeout;

#[test]
fn node_shebang_launch_uses_node_instead_of_native_host() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let tmp = TmpDir::new();
        let script = tmp.0.join("octocode-engine-node-shebang");
        std::fs::write(
            &script,
            "#!/usr/bin/env node\nprocess.stdout.write(JSON.stringify(process.argv.slice(2)))\n",
        )
        .unwrap();
        let mut args = vec!["--stdio".to_owned(), "argument with spaces".to_owned()];
        let program = lsp_spawn_program(&script.to_string_lossy(), &mut args)
            .await
            .unwrap();
        let output = tokio::time::timeout(
            Duration::from_secs(5),
            tokio::process::Command::new(program)
                .args(args)
                .kill_on_drop(true)
                .output(),
        )
        .await
        .unwrap()
        .unwrap();
        std::fs::remove_file(script).unwrap();
        assert!(output.status.success(), "{output:?}");
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            r#"["--stdio","argument with spaces"]"#
        );
    });
}

// `sh` is used to spawn a real, minimal child process for the two tests
// below rather than a synthetic stand-in — the property under test
// (`wait_for_graceful_exit` actually observing/killing a real OS process) is
// not meaningfully testable without one. Gated to unix: this crate's CI runs
// `cargo test` only on its native host (macOS/Linux); the win32 platforms are
// cross-compiled release targets that are never executed in CI, and `sh` is
// not guaranteed on a developer's native Windows machine.
#[cfg(unix)]
#[test]
fn wait_for_graceful_exit_does_not_kill_a_promptly_exiting_process() {
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    runtime.block_on(async {
        let mut child = tokio::process::Command::new("sh")
            .args(["-c", "exit 0"])
            .kill_on_drop(true)
            .spawn()
            .expect("spawn sh");

        let exited_on_own =
            wait_for_graceful_exit(&mut child, tokio::time::Duration::from_millis(2_000)).await;

        assert!(
            exited_on_own,
            "expected the process to exit on its own within the bound, not be killed"
        );
    });
}

#[cfg(unix)]
#[test]
fn wait_for_graceful_exit_kills_a_process_that_ignores_the_window() {
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    runtime.block_on(async {
        let mut child = tokio::process::Command::new("sh")
            .args(["-c", "sleep 30"])
            .kill_on_drop(true)
            .spawn()
            .expect("spawn sh");

        let exited_on_own =
            wait_for_graceful_exit(&mut child, tokio::time::Duration::from_millis(50)).await;

        assert!(
            !exited_on_own,
            "expected escalation to kill for a process that outlives the graceful window"
        );
        // Confirm the process was actually terminated, not just abandoned —
        // it must be reapable promptly after the kill.
        let status = tokio::time::timeout(std::time::Duration::from_secs(2), child.wait())
            .await
            .expect("process should be reaped promptly after kill")
            .expect("wait should succeed");
        assert!(!status.success());
    });
}

/// The sweep signals `-pid`. Once the leader is reaped its pid can be
/// recycled, and a new process that leads its own group would take the
/// signal. The sweep must therefore fire while the leader is still
/// unreaped (alive or a zombie), on both the clean-exit and timeout paths.
#[cfg(unix)]
#[test]
fn group_sweep_fires_before_the_leader_is_reaped() {
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    runtime.block_on(async {
        for (script, window, expect_exit) in [("exit 0", 2_000, true), ("sleep 30", 50, false)] {
            let mut child = tokio::process::Command::new("sh")
                .args(["-c", script])
                .process_group(0)
                .kill_on_drop(true)
                .spawn()
                .expect("spawn sh");
            let reserved = std::cell::Cell::new(None);
            let exited = end_child(
                &mut child,
                tokio::time::Duration::from_millis(window),
                |pid| {
                    // SAFETY: signal 0 only checks that `pid` still names a
                    // process (a zombie counts); nothing is delivered.
                    let probe = unsafe { libc::kill(pid as i32, 0) };
                    reserved.set(Some(probe == 0));
                    std::future::ready(())
                },
            )
            .await;
            assert_eq!(exited, expect_exit, "{script}");
            assert_eq!(
                reserved.get(),
                Some(true),
                "{script}: the sweep ran after the leader was reaped"
            );
            assert!(
                child.id().is_none(),
                "{script}: the leader is reaped afterwards"
            );
        }
    });
}

#[cfg(unix)]
fn wait_until_gone(pid: i32) -> bool {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    // SAFETY: signal 0 only checks whether the pid exists.
    while unsafe { libc::kill(pid, 0) } == 0 {
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    true
}

/// `stop` overlapping a slow `start` neither deadlocks nor leaves the
/// server `start` publishes running with a live connection: it waits for the
/// start (same lock order) and then tears it down.
#[cfg(unix)]
#[test]
fn stop_overlapping_a_slow_start_waits_then_tears_it_down() {
    if !node_available() {
        return;
    }
    let tmp = TmpDir::new();
    let pid_file = tmp.0.join("octocode-engine-start-stop-pid");
    let (_tmp, root, config) = configurable_server(
        "octocode-engine-start-stop",
        &[
            ("FAKE_INIT_DELAY_MS", "600".into()),
            ("FAKE_PID_FILE", pid_file.to_string_lossy().into_owned()),
        ],
        None,
    );
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let client = NativeLspClient::new(config);
        let starter = client.clone();
        let start = tokio::spawn(async move { starter.start().await });
        // The server writes its pid on spawn and answers `initialize` only
        // after FAKE_INIT_DELAY_MS: once the pid exists, start is in flight.
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while !pid_file.exists() {
            assert!(
                std::time::Instant::now() < deadline,
                "the slow server never spawned"
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert!(!start.is_finished(), "start must still be in flight");
        timeout(Duration::from_secs(20), client.stop())
            .await
            .expect("stop must not deadlock against an in-flight start")
            .expect("stop");
        timeout(Duration::from_secs(20), start)
            .await
            .expect("start finishes")
            .expect("start task")
            .expect("start succeeds");
        assert!(
            client.connection_handle().await.is_err(),
            "stop must tear down the connection the overlapping start published"
        );
        assert!(client.inner.child.lock().await.is_none(), "child reaped");
        assert!(!client.is_alive().await);
    });
    let pid: i32 = std::fs::read_to_string(&pid_file)
        .expect("server pid")
        .trim()
        .parse()
        .expect("numeric pid");
    assert!(wait_until_gone(pid), "the started server was left running");
    let _ = std::fs::remove_file(pid_file);
    let _ = std::fs::remove_dir_all(root);
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[tokio::test]
async fn hard_kill_reaps_a_setsid_grandchild() {
    let dir = tempfile::tempdir().expect("dir");
    let pidfile = dir.path().join("escaped.pid");
    let mut child = tokio::process::Command::new("python3")
        .args(["-c", "import os,sys,time\nif os.fork()==0:\n os.setsid()\n open(sys.argv[1],'w').write(str(os.getpid()))\n time.sleep(60)\nelse:\n time.sleep(60)\n"])
        .arg(&pidfile).process_group(0).kill_on_drop(true).spawn().expect("python3");
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while !pidfile.exists() && std::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let pid: i32 = std::fs::read_to_string(pidfile)
        .expect("escaped pid")
        .trim()
        .parse()
        .unwrap();
    cleanup_failed_start(&mut child, None).await;
    assert!(child.id().is_none(), "the direct child must also be reaped");
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    // SAFETY: signal 0 only probes the fixture child's existence.
    while unsafe { libc::kill(pid, 0) } == 0 && std::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    // SAFETY: signal 0 only probes the fixture child's existence.
    let survived = unsafe { libc::kill(pid, 0) } == 0;
    if survived {
        // SAFETY: the fixture recorded this live child's pid.
        unsafe {
            libc::kill(pid, libc::SIGKILL);
        }
    }
    assert!(!survived, "setsid grandchild {pid} survived hard kill");
}
