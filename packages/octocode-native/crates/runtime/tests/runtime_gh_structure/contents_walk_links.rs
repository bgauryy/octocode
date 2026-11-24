//! A listing no whole recursive tree covers (truncated, or answered with a
//! 404 or 422) walks folder by folder. GitHub's Contents listing types a link
//! to a file `file`, and a contents read of the link answers the target's
//! bytes under the link's name: the walk lists and materializes such a link
//! as the tree path does, and asks for no byte of its target.
use super::*;
use octocode_test_support::mock_http::GitRepository;

/// The fake credential every link below points at.
const SECRET: &str = "FAKE_TOKEN=fake-value-not-a-real-secret";

/// A repository with plain files, a credential file, and links to it at the
/// root and one folder down.
fn repository_with_links() -> GitRepository {
    GitRepository::new()
        .file("README.md", "# readme\n")
        .file(".env", SECRET)
        .link("env.txt", ".env")
        .file("docs/guide.md", "# guide\n")
        .link("docs/env.txt", "../.env")
}

/// How the mock answers the git trees of the listed commit.
#[derive(Clone, Copy, Debug)]
enum Trees {
    /// Whole: the listing takes the tree path.
    Whole,
    /// The recursive tree is past GitHub's limit; each folder's own tree
    /// answers.
    Truncated,
    /// The recursive tree answers this status; each folder's own tree
    /// answers.
    RecursiveFails(u16),
    /// Every tree answers this status: the walk has only Contents listings.
    AllFail(u16),
}

async fn serve(repository: GitRepository, trees: Trees) -> MockServer {
    let server = MockServer::builder().start().await;
    repository.mount(&server, "a", "b", SHA).await;
    let recursive = |answer: ResponseTemplate| {
        Mock::given(method("GET"))
            .and(path_regex("/api/v3/repos/a/b/git/trees/"))
            .and(query_param("recursive", "1"))
            .respond_with(answer)
            .with_priority(1)
    };
    match trees {
        Trees::Whole => {}
        Trees::Truncated => {
            recursive(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"sha": SHA, "truncated": true, "tree": []})),
            )
            .mount(&server)
            .await;
        }
        Trees::RecursiveFails(status) => {
            recursive(
                ResponseTemplate::new(status)
                    .set_body_json(json_fixture(r#"{"message": "No tree"}"#)),
            )
            .mount(&server)
            .await;
        }
        Trees::AllFail(status) => {
            Mock::given(method("GET"))
                .and(path_regex("/api/v3/repos/a/b/git/trees/"))
                .respond_with(
                    ResponseTemplate::new(status)
                        .set_body_json(json_fixture(r#"{"message": "No tree"}"#)),
                )
                .with_priority(1)
                .mount(&server)
                .await;
        }
    }
    server
}

/// One ghStructure tree call: the served result, and the workspace whose
/// home it wrote under.
async fn tree_call(
    server: &MockServer,
    query: serde_json::Value,
) -> (serde_json::Value, Workspace) {
    let workspace = Workspace::new();
    let runtime = github(&workspace, server);
    let outcome = call(&runtime, "ghStructure", query).await.expect("served");
    runtime.close().await;
    let served: serde_json::Value = (*outcome.structured_content).clone();
    assert!(
        !served.to_string().contains(SECRET),
        "the target's bytes were served: {served}"
    );
    assert_nothing_at_rest(&workspace.home);
    (served, workspace)
}

/// A materialize of the whole repository: the served row and the checkout.
async fn materialize(server: &MockServer) -> (serde_json::Value, std::path::PathBuf, Workspace) {
    let (served, workspace) = tree_call(
        server,
        json!({"operation":"tree","owner":"a","repo":"b","ref":SHA,"materialize":true}),
    )
    .await;
    assert_eq!(
        served
            .pointer("/results/0/status")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("success"),
        "success",
        "{served}"
    );
    let data = served["results"][0]["data"].clone();
    let local = std::path::PathBuf::from(data["location"]["path"].as_str().expect("location"));
    (data, local, workspace)
}

/// Every file under `root`, repository-relative, with its text.
fn written(root: &std::path::Path) -> std::collections::BTreeMap<String, String> {
    let mut out = std::collections::BTreeMap::new();
    for path in files_under(root) {
        let relative = path.strip_prefix(root).expect("below root");
        out.insert(
            relative.to_string_lossy().replace('\\', "/"),
            String::from_utf8_lossy(&std::fs::read(&path).expect("file")).into_owned(),
        );
    }
    out
}

fn files_under(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else {
                found.push(path);
            }
        }
    }
    found
}

/// No file under the Octocode home (checkouts and saved answers alike)
/// holds the target's bytes, as text or base64.
fn assert_nothing_at_rest(home: &std::path::Path) {
    let encoded = STANDARD.encode(SECRET);
    for path in files_under(home) {
        let text = String::from_utf8_lossy(&std::fs::read(&path).unwrap_or_default()).into_owned();
        assert!(
            !text.contains(SECRET) && !text.contains(&encoded),
            "the target's bytes are at rest in {}",
            path.display()
        );
    }
}

/// Request paths the mock saw that read a file's or a blob's bytes.
async fn byte_reads(server: &MockServer) -> Vec<String> {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .map(|request| request.url.path().replace("%2F", "/"))
        .filter(|path| path.contains("/contents/") || path.contains("/git/blobs/"))
        .collect()
}

/// No read asked for a link's path as a file, or for the target's bytes.
async fn assert_no_link_or_target_read(server: &MockServer) {
    let target_blob = GitRepository::blob_sha(".env");
    let reads = byte_reads(server).await;
    for path in &reads {
        assert!(
            !path.ends_with("/contents/env.txt")
                && !path.ends_with("/contents/docs/env.txt")
                && !path.ends_with("/contents/.env")
                && !path.ends_with(&format!("/git/blobs/{target_blob}")),
            "a link was read as a file, or its target's bytes were asked for: {reads:?}"
        );
    }
}

/// Whatever sends the listing past its recursive tree (a truncated tree, a
/// 404, a 422), the walk types each folder from its own tree: the links are
/// listed and skipped exactly as the tree path does, the plain files are
/// written, and nothing reads a link or its target.
#[tokio::test]
async fn a_walked_link_the_contents_api_types_file_is_materialized_as_on_the_tree_path() {
    let tree_path = serve(repository_with_links(), Trees::Whole).await;
    let (expected, expected_root, _keep) = materialize(&tree_path).await;
    for trees in [
        Trees::Truncated,
        Trees::RecursiveFails(404),
        Trees::RecursiveFails(422),
    ] {
        let walked = serve(repository_with_links(), trees).await;
        let (data, root, _workspace) = materialize(&walked).await;

        assert_eq!(data["files"], expected["files"], "{trees:?}: {data}");
        assert_eq!(data["summary"], expected["summary"], "{trees:?}: {data}");
        assert_eq!(
            data["location"]["coverage"], expected["location"]["coverage"],
            "{trees:?}: {data}"
        );
        assert_eq!(written(&root), written(&expected_root), "{trees:?}: {data}");
        assert_eq!(
            written(&root).get("README.md").map(String::as_str),
            Some("# readme\n"),
            "{trees:?}"
        );
        assert_no_link_or_target_read(&walked).await;
    }
}

/// With no tree at all, nothing tells a link the Contents API types `file`
/// from a file: such entries are left out, the row says its coverage is
/// partial, and no link or target byte is read or written.
#[tokio::test]
async fn a_walk_with_no_tree_leaves_out_what_it_cannot_type_and_says_so() {
    for status in [404, 422] {
        let server = serve(repository_with_links(), Trees::AllFail(status)).await;
        let (data, root, _workspace) = materialize(&server).await;

        let files = written(&root);
        assert!(!files.contains_key("env.txt"), "{status}: {files:?}");
        assert!(!files.contains_key("docs/env.txt"), "{status}: {files:?}");
        assert!(
            data["partialReasons"]
                .as_array()
                .is_some_and(|reasons| !reasons.is_empty()),
            "{status}: {data}"
        );
        // `partialReasons` states the partial checkout once: no
        // `location.coverage` repeats it.
        assert!(
            data["location"].get("coverage").is_none(),
            "{status}: {data}"
        );
        assert!(
            data["warnings"]
                .as_array()
                .is_some_and(|warnings| !warnings.is_empty()),
            "the left-out entries are stated: {data}"
        );
        assert!(
            data["files"].to_string().contains("docs/"),
            "folders are still listed: {data}"
        );
        assert_no_link_or_target_read(&server).await;
    }
}

/// A listing of the link itself: its own tree answers a 422, and the tree
/// of its folder types it a link, so it is refused as no directory without a
/// contents read of it.
#[tokio::test]
async fn a_listing_of_a_link_is_typed_from_its_folder_without_reading_it() {
    let server = serve(repository_with_links(), Trees::Whole).await;
    for query in [
        json!({"operation":"tree","owner":"a","repo":"b","ref":SHA,"path":"env.txt"}),
        json!({"operation":"tree","owner":"a","repo":"b","ref":SHA,"path":"docs/env.txt","materialize":true}),
    ] {
        let (served, _workspace) = tree_call(&server, query.clone()).await;
        assert_eq!(
            served
                .pointer("/results/0/status")
                .and_then(serde_json::Value::as_str),
            Some("error"),
            "{query}: {served}"
        );
    }
    assert_no_link_or_target_read(&server).await;
}

/// Plain files reached by the walk still materialize, byte for byte, as
/// the tree path writes them.
#[tokio::test]
async fn a_plain_file_on_the_walk_still_materializes() {
    let plain = || {
        GitRepository::new()
            .file("README.md", "# readme\n")
            .file("src/lib.rs", "pub fn f() {}\n")
            .file("src/deep/mod.rs", "mod x;\n")
    };
    let tree_path = serve(plain(), Trees::Whole).await;
    let (_, expected_root, _keep) = materialize(&tree_path).await;
    for trees in [Trees::Truncated, Trees::RecursiveFails(404)] {
        let walked = serve(plain(), trees).await;
        let (data, root, _workspace) = materialize(&walked).await;

        assert_eq!(written(&root), written(&expected_root), "{trees:?}: {data}");
        assert_eq!(
            written(&root).get("src/deep/mod.rs").map(String::as_str),
            Some("mod x;\n"),
            "{trees:?}"
        );
        assert_eq!(
            data["location"]["coverage"]["state"], "complete",
            "{trees:?}: {data}"
        );
        assert!(data.get("partialReasons").is_none(), "{trees:?}: {data}");
    }
}
