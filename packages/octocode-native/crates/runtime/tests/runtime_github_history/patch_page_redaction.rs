//! A changed file's patch is redacted whole where it enters, before any page
//! cuts it: walking every issued cursor of a long diff line full of tokens
//! shows no raw token fragment on a page or across two adjacent pages, and
//! delivers the redacted patch exactly once.
use crate::support::{Run, Workspace, call, next_input, row_data};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const MARKER: &str = "[REDACTED-GITHUBTOKENS]";

fn secret() -> String {
    format!("ghp_{}", "Zq7".repeat(12))
}

/// An 8-character run of the token's secret characters, which no placeholder
/// keeps and which is shorter than the pattern the output gate matches.
fn leaks(text: &str) -> bool {
    let secret = secret();
    let tail = &secret[4..];
    (0..=tail.len() - 8).any(|at| text.contains(&tail[at..at + 8]))
}

fn patch_of(outcome: &Value) -> Option<&str> {
    outcome.pointer("/files/0/patch").and_then(Value::as_str)
}

#[tokio::test]
async fn a_long_token_line_pages_without_a_raw_fragment_and_exactly_once() {
    let sha = "c".repeat(40);
    let unit = format!("{} {} ", "a".repeat(55), secret());
    let line = format!("+{}", unit.repeat(1_200));
    let patch = format!("@@ -1,0 +1,1 @@\n{line}");
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/o/r/commits/{sha}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "sha": sha,
            "commit": {"message": "Add", "author": {"name": "A", "date": "2024-01-01T00:00:00Z"}},
            "files": [{"filename":"src/a.js","status":"modified","additions":1,
                "deletions":0,"patch":patch}]
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let mut outcome = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"commit","owner":"o","repo":"r","ref":sha,"sections":["patches"]}),
    )
    .await
    .expect("commit read");
    let (mut pages, mut delivered, mut previous) = (0, String::new(), String::new());
    loop {
        pages += 1;
        let whole = outcome.structured_content.to_string();
        assert!(!leaks(&whole), "page {pages} shows a raw token fragment");
        if let Some(text) = patch_of(row_data(&outcome)) {
            // Two adjacent pages joined: a prefix and a suffix of one token.
            let joined = format!("{previous}{text}");
            assert!(
                !leaks(&joined),
                "pages {} and {pages} join into a token",
                pages - 1
            );
            previous = text.to_owned();
            delivered.push_str(text);
        }
        let Some(next) = next_input(&outcome.structured_content).or_else(|| {
            outcome
                .structured_content
                .pointer("/results/0/data/next/patches/input")
                .cloned()
        }) else {
            break;
        };
        assert!(pages < 400, "the walk ends");
        outcome = runtime
            .run("walk".into(), "ghGetHistoryItem".into(), next)
            .await
            .expect("next page");
    }
    runtime.close().await;
    assert!(pages > 2, "the line spans several pages ({pages})");
    assert_eq!(delivered.matches(MARKER).count(), 1_200, "{pages} pages");
    // The view numbers each diff line (`1\t+…`); the rest is the patch.
    let numbered = format!("@@ -1,0 +1,1 @@\n1\t{line}");
    assert_eq!(
        delivered.replace(MARKER, &secret()),
        numbered,
        "every page delivered once"
    );
}
