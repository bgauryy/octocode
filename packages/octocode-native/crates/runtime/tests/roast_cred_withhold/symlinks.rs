//! A repository symlink serves its target's bytes under the target's rules,
//! against a repository answered as GitHub answers it
//! ([`octocode_test_support::mock_http::GitRepository`]: a contents read of a link
//! to a file answers the target's bytes under the link's name, naming no
//! target). A link to a credential file is withheld before any of its bytes
//! are asked for, and no file under the home holds them after the read; a
//! link to a plain file is followed and states its target; a link check
//! GitHub does not answer serves nothing.
use crate::support::{Workspace, call, row_data, row_status};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use octocode_test_support::mock_http::GitRepository;
use serde_json::Value;
use serde_json::json;
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";
const API: &str = "/api/v3/repos/a/b";

/// The withheld target's whole text: the marker and the start of its base64
/// (how a JSON contents answer would carry it) must reach no file.
const SECRET_TEXT: &str = "SECRET_KEY=r3AtRestMarkerValue0451\nDEBUG=1\n";
const MARKER: &str = "r3AtRestMarkerValue0451";

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

/// Every file under the home holding the secret's text or its base64.
fn secret_at_rest(workspace: &Workspace) -> Vec<String> {
    let mut found = super::files_holding(&workspace.home, MARKER);
    found.extend(super::files_holding(
        &workspace.home,
        &STANDARD.encode(SECRET_TEXT)[..24],
    ));
    found
}

async fn requests_to(server: &MockServer, route: &str) -> usize {
    let wanted = format!("{API}{route}");
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|request| request.url.path() == wanted)
        .count()
}

async fn requests_under(server: &MockServer, prefix: &str) -> usize {
    let wanted = format!("{API}{prefix}");
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|request| request.url.path().starts_with(&wanted))
        .count()
}

/// The chatbot_ner shape: `config.example` links to `.env.example`. The read
/// is refused by the target's name, the target's bytes are never asked for
/// (neither by its own path nor under the link's), and after the read no
/// file under the home holds them.
#[tokio::test]
async fn a_link_to_a_credential_file_is_withheld_and_never_stored() {
    let server = MockServer::builder().start().await;
    GitRepository::new()
        .file(".env.example", SECRET_TEXT)
        .link("config.example", ".env.example")
        .file("README.md", "readme\n")
        .mount(&server, "a", "b", SHA)
        .await;
    let workspace = Workspace::new();
    let served = read_file(&server, &workspace, "config.example").await;
    let text = served.to_string();
    assert!(!text.contains(MARKER), "{text}");
    assert!(text.contains("pathPolicyDenied"), "{text}");
    assert!(
        text.contains(".env.example"),
        "the refusal names the target: {text}"
    );
    assert_eq!(requests_under(&server, "/contents").await, 0);
    assert!(
        secret_at_rest(&workspace).is_empty(),
        "{:?}",
        secret_at_rest(&workspace)
    );
}

/// The cloudbreak shape: `flow/CLAUDE.md` links to `AGENTS.md` beside it. The
/// read serves the target's text and names the target; the link's own path
/// is never read, and a plain file states no target.
#[tokio::test]
async fn a_link_to_a_plain_file_states_its_target() {
    let server = MockServer::builder().start().await;
    GitRepository::new()
        .file("flow/AGENTS.md", "# Agents\nrelease notes body\n")
        .link("flow/CLAUDE.md", "AGENTS.md")
        .mount(&server, "a", "b", SHA)
        .await;
    let workspace = Workspace::new();
    let served = read_file(&server, &workspace, "flow/CLAUDE.md").await;
    let data = &served["results"][0]["data"];
    assert_eq!(data["symlinkTarget"], "flow/AGENTS.md", "{served}");
    assert!(
        data["content"]
            .as_str()
            .unwrap_or("")
            .contains("release notes body"),
        "{served}"
    );
    assert_eq!(requests_to(&server, "/contents/flow%2FCLAUDE.md").await, 0);

    let regular = read_file(&server, &workspace, "flow/AGENTS.md").await;
    assert!(
        regular["results"][0]["data"].get("symlinkTarget").is_none(),
        "{regular}"
    );
}

/// The linux shape: a file under a symlinked directory is read at the
/// directory link's target and states it.
#[tokio::test]
async fn a_file_under_a_symlinked_directory_is_read_at_its_target() {
    let server = MockServer::builder().start().await;
    GitRepository::new()
        .file(
            "include/dt-bindings/gpio/gpio.h",
            "#define GPIO_ACTIVE_HIGH 0\n",
        )
        .link(
            "scripts/dtc/include-prefixes/dt-bindings",
            "../../../include/dt-bindings",
        )
        .mount(&server, "a", "b", SHA)
        .await;
    let workspace = Workspace::new();
    let served = read_file(
        &server,
        &workspace,
        "scripts/dtc/include-prefixes/dt-bindings/gpio/gpio.h",
    )
    .await;
    let data = &served["results"][0]["data"];
    assert_eq!(
        data["symlinkTarget"], "include/dt-bindings/gpio/gpio.h",
        "{served}"
    );
    assert!(
        data["content"]
            .as_str()
            .unwrap_or("")
            .contains("GPIO_ACTIVE_HIGH"),
        "{served}"
    );
}

/// A link check GitHub answers with a 404 or 422 checks nothing: the read is
/// refused, its body is shown nowhere and kept nowhere, and never downloaded:
/// GitHub is asked only whether the path exists.
#[tokio::test]
async fn an_unanswered_link_check_serves_and_keeps_nothing() {
    for status in [404, 422] {
        let server = MockServer::builder().start().await;
        Mock::given(method("GET"))
            .and(path_regex("/git/trees/"))
            .respond_with(ResponseTemplate::new(status))
            .mount(&server)
            .await;
        // GitHub answers any method of the read: a HEAD with the status alone.
        Mock::given(path(format!("{API}/contents/config.example")))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                SECRET_TEXT.as_bytes().to_vec(),
                "application/vnd.github.raw; charset=utf-8",
            ))
            .mount(&server)
            .await;
        let workspace = Workspace::new();
        let served = read_file(&server, &workspace, "config.example").await;
        let text = served.to_string();
        assert!(!text.contains(MARKER), "{status}: {text}");
        assert!(text.contains("\"unavailable\""), "{status}: {text}");
        assert!(text.contains("config.example"), "{status}: {text}");
        assert_ne!(served["results"][0]["status"], "success", "{text}");
        assert!(secret_at_rest(&workspace).is_empty(), "{status}");
        let downloads = server
            .received_requests()
            .await
            .unwrap_or_default()
            .iter()
            .filter(|request| {
                request.method.as_str() == "GET" && request.url.path().contains("/contents/")
            })
            .count();
        assert_eq!(downloads, 0, "{status}: the bytes were never asked for");
    }
}

/// Every string under a `hints` key of a served result.
fn hints_of(served: &Value) -> Vec<String> {
    fn walk(value: &Value, out: &mut Vec<String>) {
        match value {
            Value::Object(fields) => {
                for (key, field) in fields {
                    match (key.as_str(), field) {
                        ("hints", Value::Array(items)) => {
                            out.extend(items.iter().filter_map(Value::as_str).map(str::to_owned));
                        }
                        _ => walk(field, out),
                    }
                }
            }
            Value::Array(items) => items.iter().for_each(|item| walk(item, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(served, &mut out);
    out
}

/// `unavailable` has one recovery for every cause behind it, so it is true
/// of a link check GitHub did not answer as it is of an HTTP 451: the served
/// failure carries a hint, and no hint blames a legal block the failure does
/// not state.
#[tokio::test]
async fn an_unavailable_failure_carries_a_hint_true_of_its_cause() {
    for (status, legal) in [(404, false), (451, true)] {
        let server = MockServer::builder().start().await;
        Mock::given(method("GET"))
            .and(path_regex("/git/trees/"))
            .respond_with(ResponseTemplate::new(status))
            .mount(&server)
            .await;
        Mock::given(path(format!("{API}/contents/config.example")))
            .respond_with(if legal {
                ResponseTemplate::new(status)
            } else {
                ResponseTemplate::new(200).set_body_raw(
                    SECRET_TEXT.as_bytes().to_vec(),
                    "application/vnd.github.raw; charset=utf-8",
                )
            })
            .mount(&server)
            .await;
        let workspace = Workspace::new();
        let served = read_file(&server, &workspace, "config.example").await;
        let text = served.to_string();
        assert!(text.contains("\"unavailable\""), "{status}: {text}");
        let hints = hints_of(&served);
        assert!(!hints.is_empty(), "{status}: no hint served: {text}");
        assert!(
            !hints.iter().any(|hint| hint.contains("legal")),
            "{status}: no hint may name a legal block the failure does not state: {hints:?}"
        );
        assert_eq!(text.contains("legal reasons"), legal, "{status}: {text}");
    }
}

/// The cost of the check: a cold read asks one tree, its folder's; a second
/// file of that folder asks none; and after a ghStructure listing of the
/// file's folder, or a deep listing of a folder above it, a read asks no tree.
#[tokio::test]
async fn a_read_asks_only_its_folders_tree_once() {
    let server = MockServer::builder().start().await;
    GitRepository::new()
        .file("src/cmd/go/testdata/script/a.txt", "a\n")
        .file("src/cmd/go/testdata/script/b.txt", "b\n")
        .file("src/net/http/status.go", "package http\n")
        .file("src/lib/deep/er/x.go", "package er\n")
        .mount(&server, "a", "b", SHA)
        .await;
    let workspace = Workspace::new();
    read_file(&server, &workspace, "src/cmd/go/testdata/script/a.txt").await;
    read_file(&server, &workspace, "src/cmd/go/testdata/script/b.txt").await;
    assert_eq!(requests_under(&server, "/git/trees/").await, 1);
    assert_eq!(
        requests_to(
            &server,
            &format!("/git/trees/{SHA}:src%2Fcmd%2Fgo%2Ftestdata%2Fscript")
        )
        .await,
        1
    );

    // A listing of the file's own folder is the one tree its check asks; a
    // deep (recursive) listing of a folder above it answers the check too.
    for (listed, depth, file) in [
        ("src/net/http", 1, "src/net/http/status.go"),
        ("src/lib", 4, "src/lib/deep/er/x.go"),
    ] {
        let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
        call(
            &runtime,
            "ghStructure",
            json!({"operation":"tree","owner":"a","repo":"b","ref":SHA,"path":listed,"maxDepth":depth}),
        )
        .await
        .expect("listing");
        runtime.close().await;
        let trees = requests_under(&server, "/git/trees/").await;
        let served = read_file(&server, &workspace, file).await;
        assert!(row_text(&served).contains("package"), "{served}");
        let asked = server
            .received_requests()
            .await
            .unwrap_or_default()
            .iter()
            .filter(|request| request.url.path().contains("/git/trees/"))
            .map(|request| request.url.to_string())
            .collect::<Vec<_>>();
        assert_eq!(
            requests_under(&server, "/git/trees/").await,
            trees,
            "{listed}: {asked:#?}"
        );
    }
}

fn row_text(served: &Value) -> &str {
    served["results"][0]["data"]["content"]
        .as_str()
        .unwrap_or_default()
}

/// A missing path below a link to a file, at the root and one folder down,
/// for a file read and a listing. GitHub answers a contents read of the link
/// with the target's bytes, so the failure's recovery types each folder from
/// its git tree: the served failure still leads to the deepest folder that
/// exists, no request reads a link or its target as a file, and no file
/// under the home holds the target's bytes.
#[tokio::test]
async fn a_missing_path_below_a_link_to_a_file_recovers_without_reading_the_link() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path(API))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            octocode_test_support::json_fixture(
                r#"{"default_branch": "main", "full_name": "a/b"}"#,
            ),
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("{API}/commits/{SHA}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha": SHA})))
        .mount(&server)
        .await;
    GitRepository::new()
        .file(".env.example", SECRET_TEXT)
        .file("keys/deploy.txt", SECRET_TEXT)
        .link("config", ".env.example")
        .file("docs/guide.md", "# guide\n")
        .link("docs/config", "../keys/deploy.txt")
        .mount(&server, "a", "b", SHA)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let mut leads = Vec::new();
    for (tool, requested, nearest) in [
        ("ghGetFileContent", "config/settings.json", "."),
        ("ghGetFileContent", "docs/config/a/b.json", "docs"),
        ("ghStructure", "config/sub", "."),
        ("ghStructure", "docs/config/sub", "docs"),
    ] {
        let mut query = json!({"owner":"a","repo":"b","path":requested,"ref":SHA});
        if tool == "ghStructure" {
            query["operation"] = json!("tree");
        }
        let outcome = call(&runtime, tool, query).await.expect("error row");
        let served = outcome.structured_content.to_string();
        assert!(!served.contains(MARKER), "{tool} {requested}: {served}");
        assert_eq!(
            row_status(&outcome),
            "error",
            "{tool} {requested}: {served}"
        );
        let listing =
            crate::support::checks::find_related_call(row_data(&outcome), "ghStructure", |input| {
                input["queries"][0]["operation"] != "refs"
            })
            .map(|call| call["input"]["queries"][0]["path"].clone());
        leads.push((tool, requested, listing, nearest, served));
    }
    runtime.close().await;
    assert!(
        secret_at_rest(&workspace).is_empty(),
        "{:?}",
        secret_at_rest(&workspace)
    );
    let target_blobs = [".env.example", "keys/deploy.txt"].map(GitRepository::blob_sha);
    for never in [
        "/contents/config".to_owned(),
        "/contents/docs%2Fconfig".to_owned(),
        "/contents/.env.example".to_owned(),
        "/contents/keys%2Fdeploy.txt".to_owned(),
        format!("/git/blobs/{}", target_blobs[0]),
        format!("/git/blobs/{}", target_blobs[1]),
    ] {
        assert_eq!(requests_to(&server, &never).await, 0, "{never}");
    }
    // The recovery still leads to the deepest folder that exists.
    let asked = server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .map(|request| request.url.path().to_owned())
        .collect::<Vec<_>>();
    for (tool, requested, listing, nearest, served) in leads {
        assert_eq!(
            listing,
            Some(json!(nearest)),
            "{tool} {requested}: {served}\n{asked:#?}"
        );
    }
}
