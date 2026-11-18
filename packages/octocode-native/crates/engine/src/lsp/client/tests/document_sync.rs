//! Document sync ordering and project-config change detection.

use super::*;
use crate::lsp::transport::ClientRequestContext;
use serde_json::json;
use tokio::io::{AsyncBufReadExt, BufReader};

/// A client wired to an in-memory server instead of a spawned process.
async fn client_with_fake_server(
    client_to_server: usize,
) -> (
    NativeLspClient,
    BufReader<tokio::io::DuplexStream>,
    tokio::io::DuplexStream,
) {
    client_with_fake_server_for(ts_config(), client_to_server).await
}

async fn client_with_fake_server_for(
    config: LspLanguageServerConfig,
    client_to_server: usize,
) -> (
    NativeLspClient,
    BufReader<tokio::io::DuplexStream>,
    tokio::io::DuplexStream,
) {
    client_with_fake_server_using(NativeLspClient::new(config), client_to_server).await
}

pub(super) async fn client_with_fake_server_using(
    client: NativeLspClient,
    client_to_server: usize,
) -> (
    NativeLspClient,
    BufReader<tokio::io::DuplexStream>,
    tokio::io::DuplexStream,
) {
    let (client_w, server_r) = tokio::io::duplex(client_to_server);
    let (server_w, client_r) = tokio::io::duplex(64 * 1024);
    let connection = JsonRpcConnection::new(
        client_r,
        client_w,
        ClientRequestContext {
            configuration: json!({}),
            section_root: None,
            workspace_folders: json!([]),
        },
        Arc::clone(&client.inner.progress),
        None,
        client.inner.clock.clone(),
    );
    *client.inner.connection.lock().await = Some(Arc::new(connection));
    (client, BufReader::new(server_r), server_w)
}

pub(super) async fn send_server_frame(writer: &mut tokio::io::DuplexStream, message: Value) {
    use tokio::io::AsyncWriteExt;
    let body = serde_json::to_vec(&message).expect("message");
    writer
        .write_all(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes())
        .await
        .expect("mock server header");
    writer.write_all(&body).await.expect("mock server body");
}

pub(super) async fn read_client_frame(reader: &mut BufReader<tokio::io::DuplexStream>) -> Value {
    use tokio::io::AsyncReadExt;
    let mut length = None;
    loop {
        let mut header = String::new();
        reader.read_line(&mut header).await.expect("header");
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some(value) = header.strip_prefix("Content-Length:") {
            length = Some(value.trim().parse::<usize>().expect("length"));
        }
    }
    let mut body = vec![0; length.expect("Content-Length")];
    reader.read_exact(&mut body).await.expect("body");
    serde_json::from_slice(&body).expect("json")
}

#[tokio::test]
async fn did_open_preserves_custom_extension_language_ids_and_builtin_siblings() {
    let directory = tempfile::tempdir().expect("directory");
    std::fs::write(
        directory.path().join("lsp-servers.json"),
        r#"{"languageServers":{
            ".h":{"command":"custom-server","languageId":"cpp"},
            ".js":{"command":"custom-server","languageId":"custom-javascript"},
            ".ts":{"command":"custom-server","languageId":"typescript"}
        }}"#,
    )
    .expect("custom routes");
    let options = crate::lsp::config::LspDiscoveryOptions {
        octocode_home: Some(directory.path().to_path_buf()),
        ..Default::default()
    };
    let root = directory.path().to_string_lossy();
    for (anchor, opened, expected) in [
        ("anchor.h", "source.h", "cpp"),
        ("anchor.js", "source.js", "custom-javascript"),
        ("anchor.ts", "source.tsx", "typescriptreact"),
        ("anchor.tsx", "source.ts", "typescript"),
    ] {
        let anchor = directory.path().join(anchor);
        let config =
            crate::lsp::config::default_server_for_file(&anchor.to_string_lossy(), &root, &options)
                .expect("custom route");
        let (client, mut server, _server_w) = client_with_fake_server_for(config, 64 * 1024).await;
        client
            .open_document(
                directory.path().join(opened).to_string_lossy().into_owned(),
                "",
            )
            .await
            .expect("didOpen");
        let frame = read_client_frame(&mut server).await;
        assert_eq!(frame["method"], "textDocument/didOpen");
        assert_eq!(
            frame["params"]["textDocument"]["languageId"],
            expected,
            "{opened} opened from {}",
            anchor.display()
        );
    }
}

fn clangd_config(root: &Path, language_id: &str) -> LspLanguageServerConfig {
    LspLanguageServerConfig {
        command: "clangd".into(),
        args: None,
        workspace_root: root.to_string_lossy().into_owned(),
        language_id: Some(language_id.into()),
        document_language_ids: Default::default(),
        initialization_options: None,
        env: None,
        max_memory_mb: None,
        source: Default::default(),
    }
}

/// clangd reads a file's compile command at its first `didOpen` and never
/// again, ignoring `workspace/didChangeWatchedFiles` for these files: the
/// client reports the change so the pool can replace it.
#[test]
fn a_clangd_client_reports_a_compile_database_changed_after_it_opened_a_document() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let tmp = TmpDir::new();
        let project = tmp.0.join("project");
        let examples = project.join("examples");
        std::fs::create_dir_all(&examples).unwrap();
        let source = examples.join("kernel.cuh");
        std::fs::write(&source, "__global__ void k() {}\n").unwrap();
        let (client, _server, _server_w) =
            client_with_fake_server_for(clangd_config(&project, "cuda"), 64 * 1024).await;
        client
            .open_document(
                source.to_string_lossy().into_owned(),
                "__global__ void k() {}\n",
            )
            .await
            .unwrap();
        assert!(
            !client.project_config_changed().await,
            "nothing changed yet"
        );

        let flags = project.join("compile_flags.txt");
        std::fs::write(&flags, "-xcuda\n").unwrap();
        assert!(client.project_config_changed().await, "flags file created");

        // A server started afterwards begins from what exists then.
        let (fresh, _server, _server_w) =
            client_with_fake_server_for(clangd_config(&project, "cuda"), 64 * 1024).await;
        fresh
            .open_document(
                source.to_string_lossy().into_owned(),
                "__global__ void k() {}\n",
            )
            .await
            .unwrap();
        assert!(!fresh.project_config_changed().await);
        std::fs::remove_file(&flags).unwrap();
        assert!(fresh.project_config_changed().await, "flags file deleted");
    });
}

#[test]
fn a_server_that_rereads_its_configuration_is_never_reported_changed() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let tmp = TmpDir::new();
        let source = tmp.0.join("a.ts");
        std::fs::write(&source, "export {};\n").unwrap();
        let (client, _server, _server_w) = client_with_fake_server(64 * 1024).await;
        client
            .open_document(source.to_string_lossy().into_owned(), "export {};\n")
            .await
            .unwrap();
        std::fs::write(tmp.0.join("compile_flags.txt"), "-xcuda\n").unwrap();
        assert!(!client.project_config_changed().await);
    });
}

#[test]
fn concurrent_syncs_of_one_document_reach_the_server_in_version_order() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        // Large bodies widen the window between version reservation and the
        // write (JSON encoding happens in between), so without the per-client
        // sync lock the workers reliably interleave out of order.
        const SYNCS: usize = 24;
        const BODY: usize = 512 * 1024;
        // Several rounds (one fresh document each) so an unordered
        // implementation is caught with high probability.
        const ROUNDS: usize = 4;
        let (client, mut server, _server_w) = client_with_fake_server(64 * 1024).await;
        let dir = TmpDir::new();
        for round in 0..ROUNDS {
            let file = dir.join(format!("sync-order-{round}.ts"));
            let file = file.to_string_lossy().into_owned();
            let reader = tokio::spawn(async move {
                let mut frames = Vec::new();
                for _ in 0..SYNCS {
                    frames.push(read_client_frame(&mut server).await);
                }
                (frames, server)
            });
            let tasks: Vec<_> = (0..SYNCS)
                .map(|index| {
                    let client = client.clone();
                    let file = file.clone();
                    tokio::spawn(async move {
                        let mut body = format!("// {index}\n");
                        body.push_str(&"x".repeat(BODY));
                        client.open_document(file, &body).await
                    })
                })
                .collect();
            for task in tasks {
                task.await.unwrap().expect("sync");
            }
            let (frames, returned) = tokio::time::timeout(Duration::from_secs(30), reader)
                .await
                .expect("frames")
                .unwrap();
            server = returned;
            assert_eq!(frames[0]["method"], "textDocument/didOpen", "round {round}");
            assert_eq!(frames[0]["params"]["textDocument"]["version"], 1);
            for (index, frame) in frames.iter().enumerate().skip(1) {
                assert_eq!(
                    frame["method"], "textDocument/didChange",
                    "round {round} frame {index}"
                );
                assert_eq!(
                    frame["params"]["textDocument"]["version"],
                    json!(index + 1),
                    "round {round}: versions must arrive in order"
                );
            }
        }
        assert!(!client.is_busy());
    });
}

#[test]
fn a_blocked_sync_keeps_the_client_busy_and_cancelling_it_releases() {
    paused(|clock| async move {
        let (client, _server, _server_w) =
            client_with_fake_server_using(client_on(&clock, ts_config()), 1).await;
        let tmp = TmpDir::new();
        let content = "x".repeat(4096);
        let mut syncing = Box::pin(client.open_document(
            tmp.0.join("busy.ts").to_string_lossy().into_owned(),
            &content,
        ));
        std::future::poll_fn(|context| {
            assert!(
                syncing.as_mut().poll(context).is_pending(),
                "the unread pipe blocks the sync"
            );
            std::task::Poll::Ready(())
        })
        .await;
        assert!(client.is_busy(), "sync takes its lease before writing");
        clock.advance(Duration::from_millis(50)).await;
        assert!(client.is_busy(), "a stalled sync is activity, not idleness");
        drop(syncing);
        assert!(!client.is_busy(), "cancelling the sync releases its lease");
    });
}
