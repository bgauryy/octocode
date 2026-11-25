//! `path` names a folder or a file: a folder searches under it, a file searches that file.
use super::*;
use crate::test_support::backed::{TREE_SHA, listing, mount_read, mount_ref};
use octocode_test_support::json_fixture;
use octocode_test_support::mock_http::mount_json;

fn item(path: &str) -> serde_json::Value {
    json!({"name":path.rsplit('/').next(),"path":path,"sha":"1","html_url":"https://x",
        "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"}})
}

/// Mount GitHub's answer to the search `q`: `items` of `total` matches,
/// sent `times` times.
async fn answer(server: &MockServer, q: &str, total: usize, items: &[&str], times: u64) {
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .and(query_param("q", q))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count":total,"incomplete_results":false,
            "items":items.iter().map(|path| item(path)).collect::<Vec<_>>()
        })))
        .expect(times)
        .mount(server)
        .await;
}

async fn search(
    server: &MockServer,
    row: serde_json::Value,
) -> octocode_runtime_base::response::result::ToolData {
    let provider = one_shot_provider(server);
    let query: GhSearchCodeQuery = serde_json::from_value(row).expect("query");
    execute(
        &provider,
        &query,
        &default_context(),
        &Passthrough,
        &octocode_runtime_base::policy::discovery::is_sensitive_repository_file,
    )
    .await
    .expect("search")
}

/// A path the read cannot type (here GitHub resolves neither the ref nor
/// the path) is searched: GitHub's `path:` matches no file's own path, so
/// the empty folder search runs once more as the file, which lists that
/// file alone (a deeper path that ends the same way is another file), with
/// no repair hint.
#[tokio::test]
async fn a_file_path_searches_that_file() {
    let server = MockServer::builder().start().await;
    answer(
        &server,
        r#"main path:"src/bin/main.rs" repo:a/b in:path"#,
        0,
        &[],
        1,
    )
    .await;
    answer(
        &server,
        r#"main filename:"src/bin/main.rs" path:"src/bin" repo:a/b in:path"#,
        2,
        &["src/bin/main.rs", "src/bin/x/src/bin/main.rs"],
        1,
    )
    .await;
    let out = search(
        &server,
        json_fixture(
            r#"{
                "owner": "a",
                "repo": "b",
                "keywords": ["main"],
                "matchIn": ["path"],
                "path": "src/bin/main.rs"
            }"#,
        ),
    )
    .await;
    assert_eq!(paths_of(&out), ["src/bin/main.rs"], "{}", out.data);
    assert!(
        !out.data["hints"].to_string().contains("nameContains"),
        "{}",
        out.data
    );
    // A root file is anchored at the root.
    let server = MockServer::builder().start().await;
    answer(&server, "main path:Cargo.toml repo:a/b in:path", 0, &[], 1).await;
    answer(
        &server,
        "main filename:Cargo.toml path:/ repo:a/b in:path",
        1,
        &["Cargo.toml"],
        1,
    )
    .await;
    let out = search(
        &server,
        json_fixture(
            r#"{
                "owner": "a",
                "repo": "b",
                "keywords": ["main"],
                "matchIn": ["path"],
                "path": "Cargo.toml"
            }"#,
        ),
    )
    .await;
    assert_eq!(paths_of(&out), ["Cargo.toml"], "{}", out.data);
}

/// A folder `path` searches under it once; a dotted folder name is a folder.
#[tokio::test]
async fn a_folder_path_searches_under_it_once() {
    let server = MockServer::builder().start().await;
    answer(
        &server,
        r#"main path:"docs/v1.2" repo:a/b in:path"#,
        2,
        &["docs/v1.2/main.md", "docs/v1.2/a/main.rs"],
        1,
    )
    .await;
    answer(
        &server,
        r#"main filename:"docs/v1.2" path:docs repo:a/b in:path"#,
        0,
        &[],
        0,
    )
    .await;
    let out = search(
        &server,
        json_fixture(
            r#"{
                "owner": "a",
                "repo": "b",
                "keywords": ["main"],
                "matchIn": ["path"],
                "path": "docs/v1.2"
            }"#,
        ),
    )
    .await;
    let mut paths = paths_of(&out);
    paths.sort();
    assert_eq!(
        paths,
        ["docs/v1.2/a/main.rs", "docs/v1.2/main.md"],
        "{}",
        out.data
    );
}

/// No code search may run: a searched file costs two of the quota's ten a
/// minute (a folder search, then a file search).
async fn no_code_search(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(server)
        .await;
}

/// A repo-scoped file path is read, not searched: the row lists the file's
/// keyword lines at the commit it was read at, from the core quota alone,
/// with no index disclosure (nothing came from the index).
#[tokio::test]
async fn a_file_path_in_one_repo_is_read_with_no_code_search() {
    let server = MockServer::builder().start().await;
    no_code_search(&server).await;
    mount_ref(&server, "HEAD").await;
    mount_read(
        &server,
        "src/bin/main.rs",
        "use std::io;\nfn main() {\n    run();\n}\n",
    )
    .await;
    let out = search(
        &server,
        json_fixture(
            r#"{"owner": "a", "repo": "b", "keywords": ["fn main"], "path": "src/bin/main.rs"}"#,
        ),
    )
    .await;
    assert_eq!(paths_of(&out), ["src/bin/main.rs"], "{}", out.data);
    assert_eq!(
        listing(&out.data["files"][0]),
        json!(["2\tfn main() {"]),
        "{}",
        out.data
    );
    assert!(out.data.to_string().contains(TREE_SHA), "{}", out.data);
    assert!(out.data.get("coverage").is_none(), "{}", out.data);
    assert!(!out.data.to_string().contains("index"), "{}", out.data);
}

/// A file that lacks a keyword is an empty answer, complete over the file
/// read whole, not over an index; still no code search.
#[tokio::test]
async fn a_read_file_without_every_keyword_is_empty_and_complete() {
    let server = MockServer::builder().start().await;
    no_code_search(&server).await;
    mount_ref(&server, "HEAD").await;
    mount_read(&server, "src/lib.rs", "pub fn run() {}\n").await;
    let out = search(
        &server,
        json_fixture(
            r#"{"owner": "a", "repo": "b", "keywords": ["run", "absent_term"], "path": "src/lib.rs"}"#,
        ),
    )
    .await;
    assert_eq!(out.status, Some("empty"), "{}", out.data);
    assert!(paths_of(&out).is_empty(), "{}", out.data);
    assert_eq!(out.data["coverage"]["state"], "complete", "{}", out.data);
    let reason = out.data["coverage"]["reason"].as_str().unwrap_or_default();
    assert!(
        reason.contains("src/lib.rs") && reason.contains(TREE_SHA),
        "{reason}"
    );
}

/// A path the read types as a folder runs the folder search once, and an
/// empty one is the answer: no second search as a file.
#[tokio::test]
async fn a_read_folder_path_runs_one_folder_search() {
    let server = MockServer::builder().start().await;
    mount_ref(&server, "HEAD").await;
    mount_json(
        &server,
        "/api/v3/repos/a/b",
        200,
        json!({"full_name": "a/b", "default_branch": "main", "private": false, "archived": false}),
    )
    .await;
    // The folder is a tree at every level of the path, holding one file.
    octocode_test_support::mock_http::list_in_trees(&server, "a", "b", "docs/v1.2/main.md").await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/docs%2Fv1.2",
        200,
        json!([{"type": "file", "name": "main.md", "path": "docs/v1.2/main.md"}]),
    )
    .await;
    answer(
        &server,
        r#"main path:"docs/v1.2" repo:a/b in:file"#,
        0,
        &[],
        1,
    )
    .await;
    answer(
        &server,
        r#"main filename:"docs/v1.2" path:docs repo:a/b in:file"#,
        0,
        &[],
        0,
    )
    .await;
    let out = search(
        &server,
        json_fixture(r#"{"owner": "a", "repo": "b", "keywords": ["main"], "path": "docs/v1.2"}"#),
    )
    .await;
    assert!(paths_of(&out).is_empty(), "{}", out.data);
}

/// A path the policy withholds is never read to type it: no contents
/// request asks for it, and the searches answer the row as before.
#[tokio::test]
async fn a_withheld_path_is_never_read_to_type_it() {
    let server = MockServer::builder().start().await;
    mount_ref(&server, "HEAD").await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/.env"))
        .respond_with(ResponseTemplate::new(200).set_body_string("SECRET=1"))
        .expect(0)
        .mount(&server)
        .await;
    answer(&server, "SECRET path:.env repo:a/b in:file", 0, &[], 1).await;
    answer(
        &server,
        "SECRET filename:.env path:/ repo:a/b in:file",
        0,
        &[],
        1,
    )
    .await;
    mount_json(
        &server,
        "/api/v3/repos/a/b",
        200,
        json!({"full_name": "a/b", "default_branch": "main", "private": false, "archived": false}),
    )
    .await;
    let out = search(
        &server,
        json_fixture(r#"{"owner": "a", "repo": "b", "keywords": ["SECRET"], "path": ".env"}"#),
    )
    .await;
    assert!(paths_of(&out).is_empty(), "{}", out.data);
}

/// A file path missing at the default branch's head, the one branch the
/// code index covers, answers the row from that read: empty, complete over
/// the commit read, with no code search (each searched file path spends two
/// of the quota's ten a minute) and no lead that reads or searches the
/// missing path again; the parent folder's listing is the one route on.
#[tokio::test]
async fn a_file_path_missing_at_the_default_branch_is_answered_with_no_code_search() {
    let server = MockServer::builder().start().await;
    no_code_search(&server).await;
    mount_ref(&server, "HEAD").await;
    mount_read(&server, "lib/request.js", "var trust = 1;\n").await;
    let out = search(
        &server,
        json_fixture(
            r#"{"owner": "a", "repo": "b", "keywords": ["trust"], "path": "lib/nope.js"}"#,
        ),
    )
    .await;
    assert_eq!(out.status, Some("empty"), "{}", out.data);
    assert!(paths_of(&out).is_empty(), "{}", out.data);
    assert_eq!(out.data["coverage"]["state"], "complete", "{}", out.data);
    let reason = out.data["coverage"]["reason"].as_str().unwrap_or_default();
    assert!(
        reason.contains("lib/nope.js") && reason.contains(TREE_SHA),
        "{reason}"
    );
    let text = out.data.to_string();
    for tool in ["ghGetFileContent", "ghSearchCode"] {
        assert!(
            !text.contains(tool),
            "a lead repeats the missing path: {text}"
        );
    }
    let tree = &out.data["related"][0];
    assert_eq!(tree["tool"], "ghStructure", "{text}");
    assert_eq!(
        tree["input"]["queries"][0],
        json!({"operation": "tree", "owner": "a", "repo": "b", "path": "lib", "ref": TREE_SHA}),
        "the listing is the folder that holds the miss, at the commit read: {text}"
    );
}

/// A file path whose folders are missing too lists the nearest folder the
/// commit holds, at that commit: a listing of a missing parent would only
/// fail again.
#[tokio::test]
async fn a_file_path_under_missing_folders_lists_the_nearest_folder() {
    let server = MockServer::builder().start().await;
    no_code_search(&server).await;
    mount_ref(&server, "HEAD").await;
    mount_read(&server, "lib/request.js", "var trust = 1;\n").await;
    let out = search(
        &server,
        json_fixture(
            r#"{"owner": "a", "repo": "b", "keywords": ["trust"], "path": "lib/gone/deeper/nope.js"}"#,
        ),
    )
    .await;
    assert_eq!(out.status, Some("empty"), "{}", out.data);
    assert_eq!(out.data["coverage"]["state"], "complete", "{}", out.data);
    assert_eq!(
        out.data["related"][0]["input"]["queries"][0],
        json!({"operation": "tree", "owner": "a", "repo": "b", "path": "lib", "ref": TREE_SHA}),
        "{}",
        out.data
    );
}

/// The code index matches a path in any case, folders included: a file
/// path whose folder differs only in case reads the file the commit holds.
#[tokio::test]
async fn a_file_path_in_another_case_of_its_folder_reads_that_file() {
    let server = MockServer::builder().start().await;
    no_code_search(&server).await;
    mount_ref(&server, "HEAD").await;
    mount_read(&server, "lib/request.js", "var trust = 1;\n").await;
    let out = search(
        &server,
        json_fixture(
            r#"{"owner": "a", "repo": "b", "keywords": ["trust"], "path": "LIB/request.js"}"#,
        ),
    )
    .await;
    assert_eq!(paths_of(&out), ["lib/request.js"], "{}", out.data);
}

/// GitHub's code search matches a path's case loosely: a file path missing
/// only in its name's case reads the file its folder lists under that name
/// any case, still with no code search.
#[tokio::test]
async fn a_file_path_in_another_case_reads_the_file_it_names() {
    let server = MockServer::builder().start().await;
    no_code_search(&server).await;
    mount_ref(&server, "HEAD").await;
    mount_read(&server, "lib/request.js", "var trust = 1;\n").await;
    let out = search(
        &server,
        json_fixture(
            r#"{"owner": "a", "repo": "b", "keywords": ["trust"], "path": "lib/Request.JS"}"#,
        ),
    )
    .await;
    assert_eq!(paths_of(&out), ["lib/request.js"], "{}", out.data);
    assert_eq!(
        listing(&out.data["files"][0]),
        json!(["1\tvar trust = 1;"]),
        "{}",
        out.data
    );
}
