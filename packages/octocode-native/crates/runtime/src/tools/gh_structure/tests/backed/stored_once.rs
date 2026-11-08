//! A materialized file's body is stored once: the snapshot holds it, no saved
//! provider answer keeps it again, and a repeat materialize reads it from the
//! snapshot, not from GitHub.
use super::*;
use crate::tools::gh_shared::test_support::{default_context, mock_endpoint};
use octocode_github::mock_http::mount_json;
use octocode_github::{
    GitHubBudget, GitHubProvider, GitHubTransport, RetryPolicy, SavedAnswer, SavedAnswers,
};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use wiremock::MockServer;

/// The persistent runtime's answer store, counting the body bytes each kept
/// answer holds by the request it answers.
struct Counted {
    inner: crate::providers::saved_answers::KeptAnswers,
    kept: Mutex<Vec<(String, usize)>>,
}

impl SavedAnswers for Counted {
    fn load(&self, scope: &str, request: &str) -> Option<SavedAnswer> {
        self.inner.load(scope, request)
    }
    fn save(&self, scope: &str, request: &str, saved: SavedAnswer) {
        self.kept
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push((request.to_owned(), saved.answer.body.len()));
        self.inner.save(scope, request, saved);
    }
    fn forget(&self, scope: &str, request: &str) {
        self.inner.forget(scope, request);
    }
}

fn keeping_provider(server: &MockServer, answers: Arc<Counted>) -> GitHubProvider {
    let mut transport = GitHubTransport::new(
        mock_endpoint(server),
        RetryPolicy {
            max_attempts: 1,
            ..Default::default()
        },
        GitHubBudget::relaxed(),
    )
    .expect("transport");
    transport.set_saved_answers(Some(answers));
    GitHubProvider::new(transport)
}

const FILES: [(&str, &str); 3] = [
    ("src/a.rs", "fn a() { /* the first body */ }"),
    ("src/b.rs", "fn b() { /* the second body */ }"),
    ("README.md", "# the third body"),
];

async fn mount_repository(server: &MockServer) {
    mount_ref(server, "main").await;
    let tree = FILES
        .iter()
        .map(|(path, body)| json!({"path": path, "type": "blob", "size": body.len()}))
        .chain([json!({"path": "src", "type": "tree"})])
        .collect::<Vec<_>>();
    mount_json(
        server,
        format!("/api/v3/repos/a/b/git/trees/{TREE_SHA}"),
        200,
        json!({"sha": "c".repeat(40), "truncated": false, "tree": tree}),
    )
    .await;
    for (path, body) in FILES {
        mount_content(server, path, body).await;
    }
}

async fn materialize(provider: &GitHubProvider, home: &std::path::Path) -> Value {
    let query = serde_json::from_value(json!({"operation":"tree","owner":"a","repo":"b",
        "ref":"main","maxDepth":5,"materialize":true}))
    .expect("tree query");
    crate::tools::gh_structure::execute(provider, &query, &default_context(), home)
        .await
        .expect("materialize")
        .data
}

async fn content_requests(server: &MockServer) -> usize {
    server
        .received_requests()
        .await
        .expect("recorded")
        .iter()
        .filter(|request| request.url.path().contains("/contents/"))
        .count()
}

#[tokio::test]
async fn a_materialized_body_is_stored_once_and_a_repeat_reads_the_snapshot() {
    let server = MockServer::builder().start().await;
    mount_repository(&server).await;
    let answers = Arc::new(Counted {
        inner: crate::providers::saved_answers::KeptAnswers::new(None),
        kept: Mutex::default(),
    });
    let provider = keeping_provider(&server, Arc::clone(&answers));
    let home = crate::tools::test_support::Fixture::new();

    let first = materialize(&provider, &home.0).await;
    let local = std::path::PathBuf::from(
        first["location"]["localPath"]
            .as_str()
            .expect("materialized location"),
    );
    for (path, body) in FILES {
        assert_eq!(std::fs::read_to_string(local.join(path)).expect(path), body);
    }
    let body_bytes: usize = answers
        .kept
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .filter(|(request, _)| request.contains("/contents/") || request.contains("/git/blobs/"))
        .map(|(_, bytes)| bytes)
        .sum();
    assert_eq!(
        body_bytes,
        0,
        "saved answers hold the materialized bodies again: {:?}",
        answers
            .kept
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    );
    assert_eq!(content_requests(&server).await, FILES.len());

    // A repeat writes from the snapshot: no file is read again, the
    // checkout is still complete.
    let again = materialize(&provider, &home.0).await;
    assert_eq!(content_requests(&server).await, FILES.len(), "{again}");
    assert_eq!(
        again["location"]["coverage"]["state"], "complete",
        "{again}"
    );

    // A copy edited on disk is no longer the snapshot's: that file alone is
    // read again and restored.
    std::fs::write(local.join("src/a.rs"), "edited").expect("edit");
    materialize(&provider, &home.0).await;
    assert_eq!(content_requests(&server).await, FILES.len() + 1);
    assert_eq!(
        std::fs::read_to_string(local.join("src/a.rs")).expect("restored"),
        FILES[0].1
    );
}
