//! The missing-path recovery walk against a mocked GitHub that answers git
//! trees and Contents reads as GitHub does.
use super::*;
use crate::gh_shared::test_support::{default_context, mock_provider};
use octocode_github::RetryPolicy;
use octocode_test_support::json_fixture;
use octocode_test_support::mock_http::{GitRepository, mount_json};
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

async fn server() -> MockServer {
    let server = MockServer::builder().start().await;
    for reference in ["main", "HEAD"] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/commits/{reference}")))
            .respond_with(ResponseTemplate::new(200).set_body_string(SHA))
            .mount(&server)
            .await;
    }
    mount_json(
        &server,
        "/api/v3/repos/a/b",
        200,
        json_fixture(r#"{"default_branch": "main", "full_name": "a/b"}"#),
    )
    .await;
    server
}

async fn located(server: &MockServer, requested: &str) -> Option<PathRecovery> {
    let provider = mock_provider(server, RetryPolicy::default());
    let at = RepoPath {
        path: requested,
        ..RepoPath::root("a", "b")
    };
    locate_path(&provider, &at, &default_context()).await
}

/// The request paths `server` saw that read Contents.
async fn contents_reads(server: &MockServer) -> Vec<String> {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .map(|request| request.url.path().to_owned())
        .filter(|path| path.contains("/contents"))
        .collect()
}

/// GitHub answers no tree of a file (a 422): a missing path below a file
/// climbs to the folder that holds the file, never to the file itself.
#[tokio::test]
async fn a_missing_path_below_a_file_climbs_past_the_file() {
    let server = server().await;
    GitRepository::new()
        .file("a.json", "{}")
        .mount(&server, "a", "b", SHA)
        .await;
    let found = located(&server, "a.json/x.json").await.expect("located");
    assert_eq!(found.directory, "", "the parent is a file");
    assert!(found.file.is_none());
}

/// A folder tree GitHub truncated may omit the wanted entry: the recovery
/// says so, and a whole tree does not.
#[tokio::test]
async fn a_truncated_parent_tree_is_disclosed_as_possibly_cut() {
    for truncated in [true, false] {
        let server = server().await;
        Mock::given(method("GET"))
            .and(path_regex(format!(
                "^/api/v3/repos/a/b/git/trees/{SHA}(:|%3A)docs$"
            )))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "sha": SHA, "truncated": truncated, "tree": [
                    {"path": "guide.md", "mode": "100644", "type": "blob", "sha": "1".repeat(40), "size": 1}
                ]
            })))
            .mount(&server)
            .await;
        let found = located(&server, "docs/readme.md").await.expect("located");
        assert_eq!(found.directory, "docs");
        assert_eq!(found.truncated, truncated);
    }
}

/// GitHub answers a Contents read of a link to a file with the target's
/// bytes under the link's name. A missing path below such a link, at the
/// root or one folder down, is located from git trees alone: no Contents
/// read is sent, the recovery names the deepest folder that exists, and a
/// link is never offered as the case-corrected file, while a file is.
#[tokio::test]
async fn a_missing_path_below_a_link_to_a_file_is_located_from_trees_alone() {
    let server = server().await;
    GitRepository::new()
        .file("keys/deploy.txt", "FAKE_TOKEN=fake-value-not-a-real-secret")
        .link("config", "keys/deploy.txt")
        .file("docs/guide.md", "# guide\n")
        .link("docs/config", "../keys/deploy.txt")
        .mount(&server, "a", "b", SHA)
        .await;
    let mut found = Vec::new();
    for requested in [
        "config/settings.json",
        "docs/config/a/b.json",
        "Docs/Config",
        "DOCS/Guide.md",
    ] {
        let at = located(&server, requested).await.expect("located");
        found.push((requested, at.directory, at.file));
    }
    let contents = contents_reads(&server).await;
    assert!(contents.is_empty(), "{contents:?}");
    assert_eq!(
        found,
        [
            ("config/settings.json", String::new(), None),
            ("docs/config/a/b.json", "docs".to_owned(), None),
            ("Docs/Config", "docs".to_owned(), None),
            (
                "DOCS/Guide.md",
                "docs".to_owned(),
                Some("docs/guide.md".to_owned())
            ),
        ]
    );
}

/// With no folder tree answering (no folder up to the root, or a failure),
/// the recovery names nothing and falls back on no Contents read.
#[tokio::test]
async fn no_answering_tree_locates_nothing() {
    for status in [404, 422, 500] {
        let server = server().await;
        Mock::given(method("GET"))
            .and(path_regex("^/api/v3/repos/a/b/git/trees/"))
            .respond_with(
                ResponseTemplate::new(status)
                    .set_body_json(json_fixture(r#"{"message": "No tree"}"#)),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path_regex("^/api/v3/repos/a/b/contents"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json_fixture(
                r#"[{"name": "docs", "path": "docs", "type": "dir"}]"#,
            )))
            .mount(&server)
            .await;
        let at = RepoPath {
            path: "docs/readme.md",
            ..RepoPath::root("a", "b")
        };
        let provider = crate::gh_shared::test_support::one_shot_provider(&server);
        let found = locate_path(&provider, &at, &default_context()).await;
        assert!(found.is_none(), "{status}");
        let contents = contents_reads(&server).await;
        assert!(contents.is_empty(), "{status}: {contents:?}");
    }
}

/// GitHub's folder tree types a submodule as the commit it pins (mode
/// `160000`), and answers no tree of it: a missing path at or below a
/// submodule is located at its folder, and the submodule is never offered as
/// the case-corrected file.
#[tokio::test]
async fn a_submodule_is_located_as_no_file() {
    let server = server().await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/vendor%2Flib",
        200,
        json_fixture(
            r#"{
                "type": "submodule",
                "name": "lib",
                "path": "vendor/lib",
                "sha": "89abcdef0123456789abcdef0123456789abcdef",
                "size": 0
            }"#,
        ),
    )
    .await;
    for requested in ["vendor/LIB", "vendor/lib/src/x.rs"] {
        let found = located(&server, requested).await.expect("located");
        assert_eq!(found.directory, "vendor", "{requested}");
        assert!(found.file.is_none(), "{requested}: {:?}", found.file);
    }
}

/// A whole listing entry given as the path is located as the entry's own
/// path when that is a file; an entry of an absent file is an ordinary
/// missing path.
#[tokio::test]
async fn a_whole_listing_entry_is_located_as_its_own_path() {
    use octocode_runtime_base::response::groups::entry_text;
    let server = server().await;
    GitRepository::new()
        .file("docs/guide.md", "# guide\n")
        .mount(&server, "a", "b", SHA)
        .await;
    let fields = ["8".to_owned()];
    let entry = format!("docs/{}", entry_text("guide.md", false, &fields));
    let found = located(&server, &entry).await.expect("located");
    assert!(found.listing_entry);
    assert_eq!(found.file.as_deref(), Some("docs/guide.md"));

    let entry = format!("docs/{}", entry_text("gone.md", false, &fields));
    let found = located(&server, &entry).await.expect("located");
    assert!(!found.listing_entry);
    assert_eq!((found.directory.as_str(), found.file), ("docs", None));
}
