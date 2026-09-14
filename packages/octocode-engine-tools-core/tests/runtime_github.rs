mod support;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde_json::json;
use support::{Workspace, call, row_data, row_status};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn github_file_read_goes_through_execute_and_redacts() {
    let server = MockServer::start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/main"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha": sha})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/src%2Flib.rs"))
        .and(query_param("ref", sha))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type": "file",
            "encoding": "base64",
            "content": STANDARD.encode(format!("one\nneedle ghp_{}\nthree\n", "a".repeat(37))),
            "size": 22,
            "sha": "f".repeat(40),
            "path": "src/lib.rs"
        })))
        .mount(&server)
        .await;

    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetFileContent",
        json!({
            "owner": "a",
            "repo": "b",
            "path": "src/lib.rs",
            "branch": "main",
            "forceRefresh": true,
            "chunkType": "lines",
            "limit": 2
        }),
    )
    .await
    .expect("github read");
    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    let file = &row_data(&outcome)["files"][0];
    let content = file["content"].as_str().unwrap_or("");
    assert!(content.contains("one\n"), "{content}");
    assert!(
        content.contains("[REDACTED"),
        "expected secret redaction, got {content}"
    );
    assert!(file["next"]["continue"]["query"].is_object());
    runtime.close().await;
}

#[tokio::test]
async fn github_missing_identity_is_a_contract_error() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    let error = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"commit","owner":"a","repo":"b"}),
    )
    .await
    .expect_err("missing sha");
    assert_eq!(error.code, "invalidInput");
    runtime.close().await;
}

fn blob_sha(index: usize) -> String {
    format!("{:040x}", index + 1)
}

fn listing_entry(index: usize) -> serde_json::Value {
    json!({
        "name": format!("f{index:03}.rs"),
        "path": format!("f{index:03}.rs"),
        "type": "file",
        "size": 16,
        "sha": blob_sha(index)
    })
}

#[tokio::test]
async fn tree_materialize_120_blobs_copy_forwards_previous_snapshot() {
    use std::fs;
    use std::path::Path;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use wiremock::Respond;
    use wiremock::matchers::path_regex;

    let server = MockServer::start().await;
    let commit = "0123456789abcdef0123456789abcdef01234567";
    let entries = (0..120).map(listing_entry).collect::<Vec<_>>();
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/o/r/commits/main"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha": commit})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/o/r/contents"))
        .and(query_param("ref", "main"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!(entries)))
        .mount(&server)
        .await;

    #[derive(Clone)]
    struct BlobResponder {
        inflight: Arc<AtomicUsize>,
        max: Arc<AtomicUsize>,
    }
    impl Respond for BlobResponder {
        fn respond(&self, request: &wiremock::Request) -> ResponseTemplate {
            let current = self.inflight.fetch_add(1, Ordering::SeqCst) + 1;
            let mut max = self.max.load(Ordering::SeqCst);
            while current > max {
                match self
                    .max
                    .compare_exchange(max, current, Ordering::SeqCst, Ordering::SeqCst)
                {
                    Ok(_) => break,
                    Err(actual) => max = actual,
                }
            }
            let sha = request.url.path().rsplit('/').next().unwrap_or_default();
            let index = u64::from_str_radix(sha, 16).unwrap_or(1).saturating_sub(1);
            let body = format!("blob-{index:03}\n");
            self.inflight.fetch_sub(1, Ordering::SeqCst);
            ResponseTemplate::new(200).set_body_json(json!({
                "encoding": "base64",
                "content": STANDARD.encode(body),
                "sha": sha
            }))
        }
    }
    let inflight = Arc::new(AtomicUsize::new(0));
    let max = Arc::new(AtomicUsize::new(0));
    Mock::given(method("GET"))
        .and(path_regex(r"^/api/v3/repos/o/r/git/blobs/[0-9a-f]{40}$"))
        .respond_with(BlobResponder {
            inflight: inflight.clone(),
            max: max.clone(),
        })
        .mount(&server)
        .await;

    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let first = call(
        &runtime,
        "ghSearch",
        json!({
            "operation": "tree",
            "owner": "o",
            "repo": "r",
            "branch": "main",
            "materialize": true
        }),
    )
    .await
    .expect("first materialize");
    assert_eq!(
        row_status(&first),
        "success",
        "{}",
        first.structured_content
    );
    let first_data = row_data(&first);
    assert_eq!(first_data["pagination"]["page"], 1);
    assert_eq!(first_data["pagination"]["written"], 50);
    assert_eq!(first_data["pagination"]["materializeOffset"], 50);
    assert_eq!(first_data["pagination"]["reason"], "writeCap");
    assert_eq!(first_data["pagination"]["hasMore"], true);
    assert!(first_data["next"]["nextPage"].is_null());
    assert!(first_data["next"]["searchLocal"].is_null());
    assert_eq!(
        first_data["next"]["continueMaterialize"]["query"]["page"],
        1
    );
    assert_eq!(
        first_data["next"]["continueMaterialize"]["query"]["materializeOffset"],
        50
    );
    assert_eq!(
        first_data["next"]["continueMaterialize"]["query"]["materialize"],
        true
    );
    let first_path = first_data["location"]["localPath"]
        .as_str()
        .expect("first localPath")
        .to_owned();
    assert!(
        Path::new(&first_path).is_absolute(),
        "location.localPath must stay absolute: {first_path}"
    );
    assert!(
        first_path.contains(&workspace.home.to_string_lossy().into_owned())
            || first_path.contains("/tmp/tree/")
            || first_path.contains("tmp/tree"),
        "expected tmp/tree path, got {first_path}"
    );
    assert!(Path::new(&first_path).join("f000.rs").is_file());
    assert!(Path::new(&first_path).join("f049.rs").is_file());
    assert!(!Path::new(&first_path).join("f050.rs").exists());

    let second_query = first_data["next"]["continueMaterialize"]["query"].clone();
    let second = call(&runtime, "ghSearch", second_query)
        .await
        .expect("second materialize");
    let second_data = row_data(&second);
    assert_eq!(second_data["pagination"]["page"], 1);
    assert_eq!(second_data["pagination"]["written"], 50);
    assert_eq!(second_data["pagination"]["reason"], "listing");
    assert_eq!(
        second_data["next"]["continueMaterialize"]["query"]["page"],
        2
    );
    assert_eq!(
        second_data["next"]["continueMaterialize"]["query"]["materializeOffset"],
        0
    );
    let second_path = second_data["location"]["localPath"]
        .as_str()
        .expect("second localPath")
        .to_owned();
    assert_ne!(first_path, second_path);
    for index in 0..100 {
        let path = Path::new(&second_path).join(format!("f{index:03}.rs"));
        let body =
            fs::read_to_string(&path).unwrap_or_else(|_| format!("missing {}", path.display()));
        assert_eq!(body, format!("blob-{index:03}\n"), "{}", path.display());
    }
    assert!(!Path::new(&second_path).join("f100.rs").exists());
    assert!(Path::new(&first_path).join("f000.rs").is_file());
    assert!(!Path::new(&first_path).join("f050.rs").exists());
    assert!(
        max.load(Ordering::SeqCst) <= 5,
        "blob fetch concurrency was {}",
        max.load(Ordering::SeqCst)
    );
    runtime.close().await;
}
