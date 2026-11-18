//! One stored copy of a provider answer's body: the saved answer and the
//! continuation that owns it share it, and each keeps it alive for itself.

use crate::support::{self, Run as _, Workspace};
use octocode_test_support::json_fixture;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::Path;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer};

const MARKER: &str = "body-marker-seen-once";

async fn search_server() -> MockServer {
    let server = MockServer::builder().start().await;
    let items: Vec<Value> = (0..40)
        .map(|index| {
            json!({
                "full_name": format!("o/r{index}"), "name": format!("r{index}"),
                "html_url": "https://x", "default_branch": "main",
                "description": format!("{MARKER} {index} {}", "words ".repeat(200)),
            })
        })
        .collect();
    Mock::given(method("GET"))
        .and(path("/api/v3/search/repositories"))
        .respond_with(octocode_test_support::mock_http::SearchIndex(items))
        .mount(&server)
        .await;
    server
}

/// The distinct files under `root` holding `text`: a hard link counts once.
#[cfg(unix)]
fn files_holding(root: &Path, text: &str) -> BTreeSet<u64> {
    use std::os::unix::fs::MetadataExt as _;
    let mut found = BTreeSet::new();
    let mut dirs = vec![root.to_owned()];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(dir).expect("dir").flatten() {
            let file = entry.path();
            let meta = std::fs::symlink_metadata(&file).expect("meta");
            if meta.is_dir() {
                dirs.push(file);
            } else if String::from_utf8_lossy(&std::fs::read(&file).expect("file")).contains(text) {
                found.insert(meta.ino());
            }
        }
    }
    found
}

fn query() -> Value {
    json_fixture(
        r#"{
            "queries": [{"operation": "search", "keywords": ["once"], "limit": 3}],
            "responseChars": 50000
        }"#,
    )
}

fn next_call(page: &Value) -> Option<Value> {
    support::checks::leads(page)
        .into_iter()
        .find(|call| call.kind == "continue" && call.tool == "ghSearchRepo")
        .map(|call| call.query)
}

fn rows(page: &Value) -> Value {
    page["results"][0]["data"]["repositories"].clone()
}

/// Different workspaces issue different handles; compare every evidence
/// value and continuation field except that explicit opaque identity.
fn page_evidence(page: &Value) -> (Value, Option<Value>) {
    let next = next_call(page).map(|mut input| {
        let cursor = input.get_mut("cursor").expect("issued cursor");
        assert!(cursor.is_string());
        *cursor = Value::Null;
        input
    });
    (rows(page), next)
}

/// A call and the continuation it issues store its body once, and the later
/// pages come from that copy whatever happened to the saved answer: a restart
/// with every saved answer gone (eviction, the idle drop) preserves every row
/// and adds no provider reread. Reissuing the query preserves its cursor.
#[cfg(unix)]
#[tokio::test]
async fn a_body_is_stored_once_and_a_continuation_outlives_its_saved_answer() {
    let server = search_server().await;
    let settings = [("GITHUB_API_URL", format!("{}/api/v3", server.uri()))];
    let baseline = Workspace::new();
    let runtime = baseline.runtime(&settings);
    let mut baseline_pages = Vec::new();
    let mut page = runtime
        .run("first".into(), "ghSearchRepo".into(), query())
        .await
        .expect("first page")
        .structured_content;
    let baseline_first_next = next_call(&page).expect("first continuation");
    loop {
        baseline_pages.push(page_evidence(&page));
        let Some(next) = next_call(&page) else { break };
        page = runtime
            .run("next".into(), "ghSearchRepo".into(), next)
            .await
            .expect("later page")
            .structured_content;
    }
    runtime.close().await;
    assert!(baseline_pages.len() > 1, "the search pages");
    let requests = server.received_requests().await.expect("recorded").len();

    let evicted = Workspace::new();
    let runtime = evicted.runtime(&settings);
    let first = runtime
        .run("first".into(), "ghSearchRepo".into(), query())
        .await
        .expect("first page")
        .structured_content;
    let first_next = next_call(&first).expect("first continuation");
    assert_ne!(first_next, baseline_first_next, "workspace-bound handles");
    let sent_before = server.received_requests().await.expect("recorded").len();
    let before_eviction = runtime
        .run("before-eviction".into(), "ghSearchRepo".into(), query())
        .await
        .expect("same query before eviction")
        .structured_content;
    assert_eq!(rows(&before_eviction), rows(&first));
    assert_eq!(next_call(&before_eviction), Some(first_next));
    assert_eq!(
        server.received_requests().await.expect("recorded").len(),
        sent_before,
        "reissuing the query reuses the saved provider answer"
    );
    runtime.close().await;
    assert_eq!(
        files_holding(&evicted.home, MARKER).len(),
        1,
        "the saved answer and the continuation's evidence share one file"
    );
    std::fs::remove_dir_all(evicted.home.join("cache")).expect("saved answers");
    let sent = server.received_requests().await.expect("recorded").len();
    let runtime = evicted.runtime(&settings);
    let mut pages = vec![page_evidence(&first)];
    let mut next = next_call(&first);
    while let Some(input) = next {
        let page = runtime
            .run("next".into(), "ghSearchRepo".into(), input)
            .await
            .expect("later page")
            .structured_content;
        next = next_call(&page);
        pages.push(page_evidence(&page));
    }
    runtime.close().await;
    assert_eq!(pages, baseline_pages);
    assert_eq!(
        server.received_requests().await.expect("recorded").len() - sent,
        requests - 1,
        "later pages ask what they asked before, nothing more"
    );
}
