//! Large pull-request review: changed-file inventory, patch availability,
//! provider file-list cap, continuation payload size, and scan latency.

use crate::support::{self};

use serde_json::{Value, json};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};
use support::Workspace;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

mod concurrency;
mod continuations;
mod head_reads;
mod inventory;
mod lead_walks;
mod match_string;
mod patch_walks;

/// When each request reached the mock: wiremock asks a responder for its
/// template on arrival, before the template's delay runs.
#[derive(Clone, Default)]
struct Arrivals(Arc<Mutex<Vec<Instant>>>);

impl Arrivals {
    fn sorted(&self) -> Vec<Instant> {
        let mut times = self
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        times.sort();
        times
    }
}

struct Recording(ResponseTemplate, Arrivals);

impl Respond for Recording {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        (self.1)
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(Instant::now());
        self.0.clone()
    }
}

/// No page and no lead: `next` is absent and `hints` holds at most prose.
fn no_continuation(value: &serde_json::Value) -> bool {
    value.get("next").is_none()
        && value
            .get("hints")
            .and_then(serde_json::Value::as_object)
            .is_none_or(|hints| hints.keys().all(|key| key == "text"))
}

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

fn pr(changed_files: usize) -> Value {
    json!({
        "number": 9, "title": "Large refactor", "state": "closed",
        "merged_at": "2024-01-03T00:00:00Z",
        "merge_commit_sha": "fedcba9876543210fedcba9876543210fedcba98",
        "draft": false, "body": "x".repeat(900),
        "user": {"login": "alice"}, "labels": [{"name": "refactor"}],
        "head": {"sha": SHA, "ref": "feat"}, "base": {"ref": "main"},
        "created_at": "2024-01-01T00:00:00Z", "updated_at": "2024-01-02T00:00:00Z",
        "closed_at": "2024-01-03T00:00:00Z",
        "comments": 4, "review_comments": 2,
        "changed_files": changed_files, "additions": 5000, "deletions": 4000
    })
}

/// A numbered patch view without its gutter (`N\t`: new-side numbers on
/// kept and added lines, old-side on removed ones): the raw patch.
fn raw_patch(view: &str) -> String {
    view.split_inclusive('\n')
        .map(|line| match line.split_once('\t') {
            Some((gutter, text))
                if !line.starts_with("@@") && gutter.bytes().all(|b| b.is_ascii_digit()) =>
            {
                text
            }
            _ => line,
        })
        .collect()
}

fn rest_file(name: &str, patch: Option<&str>, additions: u64, deletions: u64) -> Value {
    let mut file = json!({
        "sha": "1111111111111111111111111111111111111111", "filename": name,
        "status": "modified", "additions": additions, "deletions": deletions,
        "changes": additions + deletions
    });
    if let Some(patch) = patch {
        file["patch"] = json!(patch);
    }
    file
}

async fn mount_pr(server: &MockServer, changed_files: usize) {
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/pulls/9"))
        .respond_with(ResponseTemplate::new(200).set_body_json(pr(changed_files)))
        .mount(server)
        .await;
}

/// Mount `batches` provider file pages; every page but the last links `next`.
async fn mount_file_batches(
    server: &MockServer,
    batches: Vec<Vec<Value>>,
    delay: Duration,
) -> Arrivals {
    let arrivals = Arrivals::default();
    let count = batches.len();
    for (index, files) in batches.into_iter().enumerate() {
        let page = index + 1;
        let mut response = ResponseTemplate::new(200)
            .set_delay(delay)
            .set_body_json(Value::Array(files));
        if page < count {
            response = response.insert_header(
                "link",
                format!(
                    "<{}/api/v3/repos/a/b/pulls/9/files?per_page=100&page={}>; rel=\"next\"",
                    server.uri(),
                    page + 1
                )
                .as_str(),
            );
        }
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/a/b/pulls/9/files"))
            .and(query_param("page", page.to_string()))
            .respond_with(Recording(response, arrivals.clone()))
            .mount(server)
            .await;
    }
    arrivals
}

fn numbered(batch: usize, size: usize) -> Vec<Value> {
    (0..size)
        .map(|i| {
            rest_file(
                &format!("src/b{batch}/f{i}.rs"),
                Some("@@ -1 +1 @@\n-a\n+b"),
                1,
                1,
            )
        })
        .collect()
}

async fn run(server: &MockServer, query: Value) -> Value {
    let (status, data) = run_status(server, query).await;
    assert_eq!(status, "success", "{data}");
    data
}

async fn run_status(server: &MockServer, query: Value) -> (String, Value) {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let mut query = query;
    query["operation"] = json!("pullRequest");
    query["owner"] = json!("a");
    query["repo"] = json!("b");
    query["number"] = json!(9);
    // A query without `extras` reads the full view.
    let extras = match query.as_object_mut().and_then(|row| row.remove("extras")) {
        Some(kinds) => serde_json::from_value::<Vec<String>>(kinds).expect("extras"),
        None => support::ALL_EXTRAS.map(str::to_owned).to_vec(),
    };
    let extras = extras.iter().map(String::as_str).collect::<Vec<_>>();
    let row = support::run(&runtime, "ghGetHistoryItem", query, &extras).await;
    let (status, data) = (row.status().to_owned(), row.data().clone());
    runtime.close().await;
    (status, data)
}
