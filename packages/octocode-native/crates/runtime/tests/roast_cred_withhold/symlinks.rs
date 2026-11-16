//! A repository symlink serves its target's bytes under the target's rules:
//! a link to a credential file is withheld, a link to a plain file states its
//! target, and no file under the home holds a withheld target's bytes.
use crate::support::{Workspace, call, row_data, row_status};
use base64::Engine as _;
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";
const BLOB: &str = "f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0";

fn entry(name: &str, kind: &str) -> Value {
    json!({"name": name, "path": name, "sha": BLOB, "type": kind})
}

async fn raw(server: &MockServer, at: &str, body: &str) {
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/a/b/contents/{at}")))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            body.as_bytes().to_vec(),
            "application/vnd.github.raw; charset=utf-8",
        ))
        .mount(server)
        .await;
}

async fn listing(server: &MockServer, entries: Vec<Value>) {
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents"))
        .respond_with(ResponseTemplate::new(200).set_body_json(entries))
        .mount(server)
        .await;
}

async fn link(server: &MockServer, name: &str, target: &str) {
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/a/b/contents/{name}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"type":"symlink","target":target,"path":name,"sha":BLOB,
                   "content": base64::engine::general_purpose::STANDARD.encode(target)}),
        ))
        .mount(server)
        .await;
}

async fn read_file(server: &MockServer, workspace: &Workspace, name: &str) -> Value {
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetFileContent",
        json!({"owner":"a","repo":"b","path":name,"ref":SHA}),
    )
    .await
    .expect("read");
    let served: Value =
        serde_json::from_str(&outcome.structured_content.to_string()).expect("JSON");
    let _ = (row_status(&outcome), row_data(&outcome));
    runtime.close().await;
    served
}

#[tokio::test]
async fn a_link_to_a_credential_file_is_withheld_and_never_stored() {
    let server = MockServer::builder().start().await;
    listing(
        &server,
        vec![entry("env", "symlink"), entry(".env", "file")],
    )
    .await;
    link(&server, "env", ".env").await;
    raw(&server, "%2Eenv", "A=hunter2x\n").await;
    raw(&server, ".env", "A=hunter2x\n").await;
    let workspace = Workspace::new();
    let served = read_file(&server, &workspace, "env").await;
    let text = served.to_string();
    assert!(!text.contains("hunter2x"), "{text}");
    assert!(text.contains("pathPolicyDenied"), "{text}");
    assert!(super::files_holding(&workspace.home, "hunter2x").is_empty());
}

#[tokio::test]
async fn a_link_to_a_plain_file_states_its_target() {
    let server = MockServer::builder().start().await;
    listing(
        &server,
        vec![entry("RelNotes", "symlink"), entry("notes.txt", "file")],
    )
    .await;
    link(&server, "RelNotes", "notes.txt").await;
    raw(&server, "notes.txt", "release notes body\n").await;
    let workspace = Workspace::new();
    let served = read_file(&server, &workspace, "RelNotes").await;
    let data = &served["results"][0]["data"];
    assert_eq!(data["symlinkTarget"], "notes.txt", "{served}");
    assert!(
        data["content"]
            .as_str()
            .unwrap_or("")
            .contains("release notes body"),
        "{served}"
    );

    let regular = read_file(&server, &workspace, "notes.txt").await;
    assert!(
        regular["results"][0]["data"].get("symlinkTarget").is_none(),
        "{regular}"
    );
}
