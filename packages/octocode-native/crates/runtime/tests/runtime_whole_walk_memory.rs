//! A whole read's walk keeps its first page's view, the unscanned source,
//! in process memory only: walking a file that holds raw secrets, with
//! persistent storage on, writes their bytes to no file but the source.

use crate::support::{self, Run as _, Workspace};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// A GitHub token the scanner redacts; not the workspace's own credential.
const TOKEN: &str = "ghp_Z9y8X7w6V5u4T3s2R1q0P9o8N7m6L5k4J3i2";
const KEY_TAG: &str = "memoryOnlyKeyBody";

fn source() -> String {
    let mut source: String = (1..=3_000).map(|n| format!("filler {n}\n")).collect();
    source.push_str("-----BEGIN RSA PRIVATE KEY-----\n");
    for n in 0..40 {
        source.push_str(&format!(
            "MIIEpQIBAAKCAQEA{KEY_TAG}{n:04}AAAAAAAAAAAAAAAAAAAA\n"
        ));
    }
    source.push_str("-----END RSA PRIVATE KEY-----\n");
    source.push_str(&format!("{}token={TOKEN};\n", "x".repeat(20_000)));
    source.extend((1..=3_000).map(|n| format!("closing {n}\n")));
    source
}

/// The next input of a page: the response's split first, else the row's.
fn next_input(payload: &Value) -> Option<Value> {
    let split = &payload["next"]["input"];
    let row = &payload["next"]["input"];
    [split, row]
        .into_iter()
        .find(|next| next.is_object())
        .cloned()
}

/// Every file under `dir`, recursively.
fn files(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            files(&path, found);
        } else {
            found.push(path);
        }
    }
}

#[tokio::test]
async fn a_whole_walk_writes_no_unscanned_source_to_disk() {
    let workspace = Workspace::new();
    let file = workspace.write("memory_only.txt", source());
    let env = [("OCTOCODE_STORAGE_MODE", "persistent".into())];
    for view in ["content", "compact"] {
        let runtime = workspace.runtime(&env);
        let mut request = json!({"queries":[{"path": file, "view": view}], "responseChars": 4_000});
        let mut pages = 0;
        loop {
            let result = runtime
                .run_mcp(format!("{view}-{pages}"), "localFetch".into(), request)
                .await
                .expect("page");
            assert_ne!(result["isError"], true, "{result}");
            let payload = support::mcp_payload(&result);
            let text = payload.to_string();
            assert!(
                !text.contains(TOKEN) && !text.contains(KEY_TAG),
                "{view}: secret shown"
            );
            pages += 1;
            assert!(pages < 2_000, "{view}: the walk terminates");
            let Some(next) = next_input(&payload) else {
                break;
            };
            request = next;
        }
        assert!(pages > 3, "{view}: {pages} pages");
        runtime.close().await;
    }
    let mut written = Vec::new();
    files(&workspace.home, &mut written);
    files(&workspace.workspace, &mut written);
    written.retain(|path| *path != file);
    assert!(
        !written.is_empty(),
        "persistent storage wrote nothing to check"
    );
    for path in written {
        let bytes = std::fs::read(&path).unwrap_or_default();
        let text = String::from_utf8_lossy(&bytes);
        assert!(
            !text.contains(TOKEN) && !text.contains(KEY_TAG),
            "unscanned source on disk: {}",
            path.display()
        );
    }
}
