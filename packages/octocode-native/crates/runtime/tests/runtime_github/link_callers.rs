//! A repository symlink through the callers that read file bytes: the bytes
//! under a link's name are its target's, judged by the target's name, and a
//! materialize never reads a link at all.
use crate::support::{self, Run as _, Workspace, call, row_data, row_status};
use octocode_test_support::json_fixture;
use octocode_test_support::mock_http::GitRepository;
use serde_json::{Value, json};
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";
const API: &str = "/api/v3/repos/o/r";

async fn mount(server: &MockServer, route: &str, status: u16, body: Value) {
    Mock::given(method("GET"))
        .and(path(format!("{API}{route}")))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .mount(server)
        .await;
}

/// A code-search line read follows a link to its target, against links
/// answered as GitHub answers them (a contents read of a link to a file is
/// the target's bytes under the link's name): an ordinary target's lines are
/// listed whole, so no read of them is offered; a credential-named target is
/// never asked for, and its row lists the path only.
#[tokio::test]
async fn code_search_lines_follow_a_link_and_judge_its_target() {
    let server = MockServer::builder().start().await;
    let hit = |name: &str, n: usize| {
        json!({"name": name, "path": name, "sha": format!("{n:040x}"),
            "html_url": format!("https://github.com/o/r/blob/{SHA}/{name}"),
            "repository": {"full_name": "o/r", "html_url": "https://github.com/o/r",
                           "url": "https://api.github.com/repos/o/r"}})
    };
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count": 2, "incomplete_results": false,
            "items": [hit("shown.md", 1), hit("hidden.md", 2)]
        })))
        .mount(&server)
        .await;
    GitRepository::new()
        .file("notes.txt", "a needle in notes\n")
        .file(".env", "PRIVATE=needle-held-back\n")
        .link("shown.md", "notes.txt")
        .link("hidden.md", ".env")
        .mount(&server, "o", "r", SHA)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&support::github_api(&server));
    let output = runtime
        .run(
            "link".into(),
            "ghSearchCode".into(),
            json_fixture(
                r#"{
                    "queries": [{"owner": "o", "repo": "r", "keywords": ["needle"]}]
                }"#,
            ),
        )
        .await
        .expect("search")
        .structured_content
        .into_value();
    let text = output.to_string();
    assert!(text.contains("a needle in notes"), "{text}");
    assert!(!text.contains("needle-held-back"), "{text}");
    for never in [
        "/contents/.env",
        "/contents/hidden.md",
        "/contents/shown.md",
    ] {
        assert_eq!(requests_to(&server, never).await, 0, "{never}");
    }
    // The ordinary link's row lists its target's lines, so no read of it
    // is offered; the withheld target is listed by path only and offers none.
    assert!(output.pointer("/results/0/data/next").is_none(), "{output}");
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

/// A materialize reads the files its git tree typed as files: a link is
/// typed by its mode and never read, so no link target is written and no
/// request asks whether a listed file is a link.
#[tokio::test]
async fn a_materialize_writes_listed_files_and_reads_no_link() {
    let server = MockServer::builder().start().await;
    mount(
        &server,
        "",
        200,
        json_fixture(r#"{"default_branch": "main"}"#),
    )
    .await;
    Mock::given(method("GET"))
        .and(path_regex("/api/v3/repos/o/r/commits/(main|HEAD)$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha": SHA})))
        .mount(&server)
        .await;
    GitRepository::new()
        .file("a.txt", "one\n")
        .file(".env", "PRIVATE=never-written\n")
        .link("link", ".env")
        .mount(&server, "o", "r", SHA)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&support::github_api(&server));
    let outcome = call(
        &runtime,
        "ghStructure",
        json_fixture(
            r#"{
                "operation": "tree",
                "owner": "o",
                "repo": "r",
                "materialize": true
            }"#,
        ),
    )
    .await
    .expect("materialize");
    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    let data = row_data(&outcome);
    let local = std::path::PathBuf::from(data["location"]["path"].as_str().expect("checkout"));
    assert_eq!(
        std::fs::read_to_string(local.join("a.txt")).expect("file"),
        "one\n"
    );
    assert!(
        !local.join("link").exists(),
        "a link is not written: {data}"
    );
    let requests = server.received_requests().await.expect("recorded");
    assert!(
        requests
            .iter()
            .all(|request| request.url.path() != format!("{API}/contents/link")),
        "{requests:?}"
    );
    // The listed file is read once.
    let a_reads = requests
        .iter()
        .filter(|request| request.url.path() == format!("{API}/contents/a.txt"))
        .count();
    assert_eq!(a_reads, 1);
}
