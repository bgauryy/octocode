//! Patches of credential-named files stay out of every file under the home, on
//! each route that returns them, while the row itself is unchanged. A secret in
//! no known format is exactly what a recognizer cannot catch, so the marker here
//! is plain text and the path facts of the patch decide.
use crate::support::{Workspace, call, row_data, row_status, walk};
use octocode_test_support::json_fixture;
use serde_json::{Value, json};
use std::path::Path;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const WITHHELD: &str = "plainvaluewithheldfromdisk";
const CONTROL: &str = "plainvaluekeptondisk";
const BASE: &str = "1111111111111111111111111111111111111111";
const HEAD: &str = "2222222222222222222222222222222222222222";

/// Every file under `dir` holding `marker`.
fn files_holding(dir: &Path, marker: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in std::fs::read_dir(&next).expect("directory").flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if std::fs::read(&path).is_ok_and(|bytes| {
                bytes
                    .windows(marker.len())
                    .any(|chunk| chunk == marker.as_bytes())
            }) {
                found.push(path.display().to_string());
            }
        }
    }
    found
}

fn changed(filename: &str, marker: &str) -> Value {
    json!({"filename": filename, "status": "modified", "additions": 1, "deletions": 0,
           "changes": 1, "patch": format!("@@ -0,0 +1 @@\n+{marker}")})
}

fn commit(sha: &str, files: Vec<Value>) -> Value {
    json!({"sha": sha, "commit": {"message": "m", "author": {"name": "a", "date": "2026-01-01T00:00:00Z"}},
           "parents": [{"sha": BASE}], "stats": {"total": 1, "additions": 1, "deletions": 0}, "files": files})
}

fn compare(files: Vec<Value>) -> Value {
    json!({"status": "ahead", "ahead_by": 1, "behind_by": 0, "total_commits": 1,
           "base_commit": {"sha": BASE}, "merge_base_commit": {"sha": BASE},
           "commits": [{"sha": HEAD, "commit": {"message": "m", "author": {"name": "a", "date": "2026-01-01T00:00:00Z"}}}],
           "files": files})
}

fn pull_request(number: u64) -> Value {
    json!({"number": number, "title": "t", "state": "open", "body": "b",
           "user": {"login": "bob"}, "head": {"sha": HEAD, "ref": "h"}, "base": {"sha": BASE, "ref": "main"},
           "changed_files": 1, "additions": 1, "deletions": 0})
}

/// One route's fixture: `(route, body)` pairs and the call reading its patches.
struct Route {
    name: &'static str,
    mounts: Vec<(String, Value)>,
    query: Value,
}

fn routes(files: Vec<Value>) -> Vec<Route> {
    vec![
        Route {
            name: "commit",
            mounts: vec![(
                format!("/api/v3/repos/a/b/commits/{HEAD}"),
                commit(HEAD, files.clone()),
            )],
            query: json!({"operation":"commit","owner":"a","repo":"b","ref":HEAD,"sections":["patches"]}),
        },
        Route {
            name: "compare",
            mounts: vec![(
                format!("/api/v3/repos/a/b/compare/{BASE}...{HEAD}"),
                compare(files.clone()),
            )],
            query: json!({"operation":"compare","owner":"a","repo":"b","baseRef":BASE,"headRef":HEAD,"sections":["patches"]}),
        },
        Route {
            name: "pull request files",
            mounts: vec![
                ("/api/v3/repos/a/b/pulls/7".to_owned(), pull_request(7)),
                (
                    "/api/v3/repos/a/b/pulls/7/files".to_owned(),
                    Value::Array(files),
                ),
            ],
            query: json_fixture(
                r#"{
                    "operation": "pullRequest",
                    "owner": "a",
                    "repo": "b",
                    "number": 7,
                    "sections": ["patches"]
                }"#,
            ),
        },
    ]
}

async fn mounted(route: &Route) -> MockServer {
    let server = MockServer::builder().start().await;
    for (at, body) in &route.mounts {
        Mock::given(method("GET"))
            .and(path(at))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("etag", "\"v1\"")
                    .set_body_json(body),
            )
            .mount(&server)
            .await;
    }
    server
}

/// Commit, compare and pull-request patches of a credential-named file, by its
/// name or a rename's old name: the patch is withheld whole and the path
/// stays, a repeat answers the same, and the marker is in no file under the
/// home once the runtime closes. The same patch of an ordinary file is on disk
/// (the scan can see it).
#[tokio::test]
async fn patches_of_credential_files_reach_no_file_under_the_home() {
    let renamed = {
        let mut file = changed("config.sample", WITHHELD);
        file["previous_filename"] = json!("deploy/.env");
        file["status"] = json!("renamed");
        file
    };
    // Each fixture, and the path its row keeps.
    for (files, kept) in [
        (vec![changed("tests/.env", WITHHELD)], "tests/.env"),
        (
            vec![changed("deploy/client.ovpn", WITHHELD)],
            "deploy/client.ovpn",
        ),
        (
            vec![
                changed("src/app.rs", "ordinary"),
                changed("KEY.PEM", WITHHELD),
            ],
            "KEY.PEM",
        ),
        (vec![renamed], "config.sample"),
    ] {
        for route in routes(files.clone()) {
            let server = mounted(&route).await;
            let workspace = Workspace::new();
            let settings = [("GITHUB_API_URL", format!("{}/api/v3", server.uri()))];
            let runtime = workspace.runtime(&settings);
            let first = call(&runtime, "ghGetHistoryItem", route.query.clone())
                .await
                .expect("read");
            assert_eq!(row_status(&first), "success", "{}", route.name);
            let row = row_data(&first).to_string();
            assert!(
                !row.contains(WITHHELD) && row.contains(kept),
                "{}: a credential file's patch is withheld, its path kept: {row}",
                route.name
            );
            let repeat = call(&runtime, "ghGetHistoryItem", route.query.clone())
                .await
                .expect("repeat");
            assert_eq!(row_data(&repeat), row_data(&first), "{}", route.name);
            runtime.close().await;
            assert_eq!(
                files_holding(&workspace.home, WITHHELD),
                Vec::<String>::new(),
                "{}",
                route.name
            );
        }
    }
    for route in routes(vec![changed("src/app.rs", CONTROL)]) {
        let server = mounted(&route).await;
        let workspace = Workspace::new();
        let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
        call(&runtime, "ghGetHistoryItem", route.query.clone())
            .await
            .expect("read");
        runtime.close().await;
        assert!(
            !files_holding(&workspace.home, CONTROL).is_empty(),
            "{}: the scan sees nothing, so its other assertions prove nothing",
            route.name
        );
    }
}

/// A patch too long for one response page is served through the saved pages and
/// cursors: a credential file's is withheld on every page, and none reaches a
/// file.
#[tokio::test]
async fn a_paged_patch_of_a_credential_file_reaches_no_file_under_the_home() {
    let body = (0..400)
        .map(|n| format!("+{WITHHELD}{n}\n"))
        .collect::<String>();
    let mut file = changed("tests/.env", WITHHELD);
    file["patch"] = json!(format!("@@ -0,0 +1,400 @@\n{body}"));
    file["additions"] = json!(400);
    for route in routes(vec![file.clone()]) {
        let server = mounted(&route).await;
        let workspace = Workspace::new();
        let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
        let start = json!({"queries": [route.query.clone()], "responseChars": 1500});
        let walked = walk(&runtime, "ghGetHistoryItem", start, |output| {
            vec![output.to_string()]
        })
        .await;
        let seen = walked.rows.concat();
        assert!(
            !seen.contains(WITHHELD),
            "{}: a patch line shows",
            route.name
        );
        assert!(
            seen.contains("tests/.env"),
            "{}: the path stays",
            route.name
        );
        runtime.close().await;
        assert_eq!(
            files_holding(&workspace.home, WITHHELD),
            Vec::<String>::new(),
            "{}",
            route.name
        );
    }
}
