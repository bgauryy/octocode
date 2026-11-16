//! ghGetHistoryItem lists a credential file's path, status and counts and
//! serves none of its patch, whatever the file's format (a `.pgpass` or a
//! `.netrc` line matches no value rule).
use crate::support::{Workspace, call, row_status};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SHA: &str = "2222222222222222222222222222222222222222";
const PARENT: &str = "1111111111111111111111111111111111111111";

fn file(name: &str, patch: &str) -> Value {
    json!({"filename": name, "status": "added", "additions": 1, "deletions": 0,
           "changes": 1, "patch": format!("@@ -0,0 +1 @@\n+{patch}")})
}

async fn read(files: Vec<Value>, env: &[(&str, &str)]) -> (Value, Workspace) {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/a/b/commits/{SHA}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "sha": SHA,
            "commit": {"message": "m", "author": {"name": "a", "date": "2026-01-01T00:00:00Z"}},
            "parents": [{"sha": PARENT}],
            "stats": {"total": 1, "additions": 1, "deletions": 0},
            "files": files
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let mut settings = vec![("GITHUB_API_URL", format!("{}/api/v3", server.uri()))];
    settings.extend(env.iter().map(|(k, v)| (*k, (*v).to_owned())));
    let runtime = workspace.runtime(&settings);
    let outcome = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"commit","owner":"a","repo":"b","ref":SHA,"sections":["patches"]}),
    )
    .await
    .expect("commit read");
    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    let served: Value =
        serde_json::from_str(&outcome.structured_content.to_string()).expect("JSON");
    runtime.close().await;
    (served, workspace)
}

#[tokio::test]
async fn a_credential_files_patch_is_not_served() {
    let (served, workspace) = read(
        vec![
            file(".pgpass", "h:5432:d:u:hunter2"),
            file("deploy/.netrc", "password hunter2"),
            file("src/app.rs", "let ordinary = 1;"),
        ],
        &[],
    )
    .await;
    let text = served.to_string();
    assert!(!text.contains("hunter2"), "{text}");
    assert!(
        text.contains(".pgpass") && text.contains("deploy/.netrc"),
        "{text}"
    );
    assert!(
        text.contains("ordinary"),
        "an ordinary patch is shown: {text}"
    );
    assert!(super::files_holding(&workspace.home, "hunter2").is_empty());
}

#[tokio::test]
async fn a_rename_from_a_credential_name_is_withheld_too() {
    let mut renamed = file("config.sample", "password hunter2");
    renamed["status"] = json!("renamed");
    renamed["previous_filename"] = json!(".netrc");
    let (served, _) = read(vec![renamed], &[]).await;
    let text = served.to_string();
    assert!(!text.contains("hunter2"), "{text}");
    assert!(text.contains("config.sample"), "{text}");
}

#[tokio::test]
async fn the_opt_in_shows_a_credential_files_patch_redacted() {
    let token = format!("ghp_{}", "a1B2c3D4e5F6".repeat(3));
    let (served, _) = read(
        vec![file(".pgpass", &format!("h:5432:d:u:hunter2 {token}"))],
        &[("OCTOCODE_CREDENTIAL_FILES", "allow")],
    )
    .await;
    let patch = row_data_files(&served);
    assert!(patch.contains("5432"), "the patch is shown: {served}");
    assert!(
        !patch.contains(&token),
        "known secrets stay redacted: {served}"
    );
}

fn row_data_files(served: &Value) -> String {
    let data = &served["results"][0]["data"];
    data["files"].to_string()
}
