//! The per-server memory cap: the RSS watchdog and the Windows Job Object.

use super::*;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use crate::lsp::client::server_process::memory_watchdog_for;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use crate::lsp::transport::ClientRequestContext;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use serde_json::json;

/// On macOS and Linux a server whose resident memory passes its memory cap is
/// killed by the RSS watchdog and its requests fail with a clear "exceeded
/// memory cap" error.
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn server_over_its_memory_cap_is_killed_with_a_clear_error() {
    if !node_available() {
        return;
    }
    let (_tmp, root, config) = configurable_server(
        "octocode-engine-memory-cap",
        &[("FAKE_ALLOC_MB", "400".into())],
        Some(128),
    );
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let client = NativeLspClient::new(config);
        client.start().await.expect("fake server starts");
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        while client.is_alive().await {
            assert!(
                std::time::Instant::now() < deadline,
                "the watchdog never fired"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let error = client
            .get_hover(root.join("a.ts").to_string_lossy().into_owned(), 0, 0)
            .await
            .expect_err("the connection was failed");
        assert!(error.to_string().contains("exceeded memory cap"), "{error}");
        // A typed cause (callers branch on it, never on the text), and the
        // text names no setting a caller cannot pass.
        assert_eq!(*error.kind(), crate::error::ErrorKind::ResourceExhausted);
        assert!(!error.to_string().contains("maxMemoryMb"), "{error}");
        let _ = client.stop().await;
    });
    let _ = std::fs::remove_dir_all(root);
}

/// The RSS watchdog is armed on every platform that has a tree RSS reader
/// (macOS and Linux): a server tree over its cap is killed and the connection
/// fails with the "exceeded memory cap" error.
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn memory_watchdog_is_armed_and_fires_on_macos_and_linux() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let mut child = tokio::process::Command::new("sleep")
            .arg("30")
            .kill_on_drop(true)
            .spawn()
            .expect("spawn sleep");
        let (client_w, _server_r) = tokio::io::duplex(1024);
        let (_server_w, client_r) = tokio::io::duplex(1024);
        let connection = Arc::new(JsonRpcConnection::new(
            client_r,
            client_w,
            ClientRequestContext {
                configuration: json!({}),
                section_root: None,
                workspace_folders: json!([]),
            },
            ProgressTracker::new(),
            None,
        ));
        assert!(
            memory_watchdog_for(&child, None, &connection, Default::default()).is_none(),
            "no cap, no watchdog"
        );
        // A 1-byte cap: the first sample is over it.
        let _watchdog = memory_watchdog_for(&child, Some(1), &connection, Default::default())
            .expect("watchdog armed");
        let status = tokio::time::timeout(Duration::from_secs(30), child.wait())
            .await
            .expect("the watchdog kills the tree")
            .expect("wait");
        assert!(!status.success());
        assert!(!connection.is_alive(), "the connection was failed");
    });
}

/// On Windows the memory cap is a job's committed-memory limit and the job also
/// contains the server's children. A server over the cap fails its allocation
/// and dies; stopping the client then closes the job, so neither it nor the
/// child it started is left running.
#[cfg(windows)]
#[test]
fn a_server_over_its_memory_cap_and_its_child_do_not_outlive_stop() {
    use crate::test_support::{process_alive, process_gone_within};
    assert!(node_available(), "the fake language server needs node");
    let tmp = TmpDir::new();
    let server_pid = tmp.0.join("server.pid");
    let child_pid = tmp.0.join("child.pid");
    let (_server_tmp, root, config) = configurable_server(
        "octocode-engine-job-cap",
        &[
            ("FAKE_ALLOC_MB", "2048".into()),
            ("FAKE_PID_FILE", server_pid.to_string_lossy().into_owned()),
            (
                "FAKE_CHILD_PID_FILE",
                child_pid.to_string_lossy().into_owned(),
            ),
        ],
        Some(512),
    );
    let read_pid = |path: &PathBuf| -> u32 {
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        loop {
            if let Some(pid) = std::fs::read_to_string(path)
                .ok()
                .and_then(|text| text.trim().parse().ok())
            {
                return pid;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "{path:?} never written"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    };
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let client = NativeLspClient::new(config);
        client
            .start()
            .await
            .expect("fake server starts under the cap");
        let server = read_pid(&server_pid);
        let child = read_pid(&child_pid);
        // The job refuses the allocation past the cap, which ends the server.
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        while process_alive(server) {
            assert!(
                std::time::Instant::now() < deadline,
                "the memory cap never ended server {server}"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        client.stop().await.expect("stop");
        assert!(
            process_gone_within(child, Duration::from_secs(10)),
            "the server's child {child} survived stop"
        );
    });
    let _ = std::fs::remove_dir_all(root);
}
