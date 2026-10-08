// Integration test crate — assertions use unwrap/expect/panic freely.
#![allow(clippy::expect_used, clippy::panic)]

//! ghStructure: materialize layout ownership, refs and languages listings.
use crate::support;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde_json::json;
use support::{Workspace, call, row_data, row_status};
use wiremock::matchers::{method, path, path_regex, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

/// A repository `a/b` whose default branch `main` is at [`SHA`] with
/// `one.rs` and `two.rs`; each file read is expected `reads` times.
async fn tree_server(reads: u64) -> MockServer {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"default_branch":"main"})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex("/api/v3/repos/a/b/commits/(main|HEAD)$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha": SHA})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents"))
        .and(query_param("ref", SHA))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"name":"one.rs","path":"one.rs","type":"file","size":10,"sha":"1".repeat(40)},
            {"name":"two.rs","path":"two.rs","type":"file","size":10,"sha":"2".repeat(40)}
        ])))
        .mount(&server)
        .await;
    for name in ["one.rs", "two.rs"] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/contents/{name}")))
            .and(query_param("ref", SHA))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "type": "file",
                "encoding": "base64",
                "content": STANDARD.encode(format!("// {name}\n")),
                "size": 10,
                "sha": "a".repeat(40),
                "path": name
            })))
            .expect(reads)
            .mount(&server)
            .await;
    }
    server
}

fn github(workspace: &Workspace, server: &MockServer) -> octocode_native::runtime::ToolRuntime {
    workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))])
}

/// A3: octocode-mcp 19.x rewrites `tmp/tree/<owner>/<repo>/<sha>/`; a
/// materialize never shares it, and a directory in its own layout that has
/// no manifest (foreign or partial) is cleared, not trusted.
#[tokio::test]
async fn materialize_never_trusts_a_foreign_directory() {
    let server = tree_server(1).await;
    let workspace = Workspace::new();
    let legacy = workspace.home.join("tmp/tree/a/b").join(SHA);
    std::fs::create_dir_all(&legacy).expect("legacy dir");
    std::fs::write(legacy.join("one.rs"), "prod 19.x content").expect("legacy file");
    let own = workspace.home.join("tmp/materialize/v2/a/b").join(SHA);
    std::fs::create_dir_all(own.join("stale")).expect("foreign dir");
    std::fs::write(own.join("one.rs"), "// foreign").expect("foreign file");
    std::fs::write(own.join("stale/ghost.rs"), "not at this commit").expect("foreign extra");

    let runtime = github(&workspace, &server);
    let outcome = call(
        &runtime,
        "ghStructure",
        json!({"owner": "a", "repo": "b", "materialize": true}),
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
    let local =
        std::path::PathBuf::from(data["location"]["localPath"].as_str().expect("localPath"));
    assert_eq!(local, own, "{data}");
    assert_eq!(data["location"]["complete"], true, "{data}");
    assert_eq!(
        std::fs::read_to_string(local.join("one.rs")).expect("one"),
        "// one.rs\n"
    );
    assert_eq!(
        std::fs::read_to_string(local.join("two.rs")).expect("two"),
        "// two.rs\n"
    );
    assert!(!local.join("stale").exists(), "a foreign file survived");
    assert_eq!(
        std::fs::read_to_string(legacy.join("one.rs")).expect("legacy kept"),
        "prod 19.x content",
        "another writer's cache is not touched"
    );
    // The manifest sits beside the checkout, so the checkout holds only
    // repository files.
    let manifest = own.with_file_name(format!("{SHA}.manifest.json"));
    let recorded: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&manifest).expect("manifest")).expect("json");
    assert_eq!(
        recorded["files"],
        json!({"one.rs": 10, "two.rs": 10}),
        "{recorded}"
    );
    runtime.close().await;
}

/// A file the manifest recorded at its listed size is reused; one changed on
/// disk is written again.
#[tokio::test]
async fn materialize_reuses_only_recorded_files_at_their_size() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"default_branch":"main"})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex("/api/v3/repos/a/b/commits/(main|HEAD)$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha": SHA})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents"))
        .and(query_param("ref", SHA))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"name":"one.rs","path":"one.rs","type":"file","size":10,"sha":"1".repeat(40)},
            {"name":"two.rs","path":"two.rs","type":"file","size":10,"sha":"2".repeat(40)}
        ])))
        .mount(&server)
        .await;
    for name in ["one.rs", "two.rs"] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/contents/{name}")))
            .and(query_param("ref", SHA))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "type": "file",
                "encoding": "base64",
                "content": STANDARD.encode(format!("// {name}\n")),
                "size": 10,
                "sha": "a".repeat(40),
                "path": name
            })))
            .mount(&server)
            .await;
    }
    let workspace = Workspace::new();
    let runtime = github(&workspace, &server);
    let query = json!({"owner": "a", "repo": "b", "ref": SHA, "materialize": true});
    let first = call(&runtime, "ghStructure", query.clone())
        .await
        .expect("first");
    let local = std::path::PathBuf::from(
        row_data(&first)["location"]["localPath"]
            .as_str()
            .expect("localPath"),
    );
    std::fs::write(local.join("two.rs"), "tampered").expect("tamper");
    // An old mtime on the reusable file shows whether it was rewritten.
    let old = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000);
    std::fs::File::options()
        .write(true)
        .open(local.join("one.rs"))
        .expect("open one")
        .set_modified(old)
        .expect("age one");
    let second = call(&runtime, "ghStructure", query).await.expect("second");
    assert_eq!(
        row_status(&second),
        "success",
        "{}",
        second.structured_content
    );
    assert_eq!(row_data(&second)["location"]["complete"], true);
    assert_eq!(
        std::fs::read_to_string(local.join("one.rs")).expect("one"),
        "// one.rs\n"
    );
    assert_eq!(
        std::fs::metadata(local.join("one.rs"))
            .and_then(|meta| meta.modified())
            .expect("mtime"),
        old,
        "a recorded file at its size is reused, not rewritten"
    );
    assert_eq!(
        std::fs::read_to_string(local.join("two.rs")).expect("two"),
        "// two.rs\n"
    );
    runtime.close().await;
}

/// Branches and tags page together: each page holds the same page number of
/// both lists, and `next.nextPage` continues while either has more, so every
/// ref is listed exactly once.
#[tokio::test]
async fn refs_list_every_branch_and_tag_once_with_the_default_branch() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"default_branch":"main"})))
        .mount(&server)
        .await;
    let sha = |n: u8| format!("{n}").repeat(40);
    let pages = [
        (
            "branches",
            "1",
            json!([{"name":"dev","commit":{"sha":sha(1)}},{"name":"main","commit":{"sha":sha(2)}}]),
            true,
        ),
        (
            "branches",
            "2",
            json!([{"name":"release","commit":{"sha":sha(3)}}]),
            false,
        ),
        (
            "tags",
            "1",
            json!([{"name":"v1.0.0","commit":{"sha":sha(4)}}]),
            false,
        ),
        ("tags", "2", json!([]), false),
    ];
    for (kind, page, body, more) in pages {
        let mut response = ResponseTemplate::new(200).set_body_json(body);
        if more {
            response = response.insert_header(
                "link",
                format!(
                    "<{}/api/v3/repos/a/b/{kind}?per_page=2&page=2>; rel=\"next\"",
                    server.uri()
                ),
            );
        }
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/{kind}")))
            .and(query_param("per_page", "2"))
            .and(query_param("page", page))
            .respond_with(response)
            .expect(1)
            .mount(&server)
            .await;
    }
    let workspace = Workspace::new();
    let runtime = github(&workspace, &server);
    let first = call(
        &runtime,
        "ghStructure",
        json!({"owner": "a", "repo": "b", "operation": "refs", "pageSize": 2}),
    )
    .await
    .expect("refs");
    assert_eq!(
        row_status(&first),
        "success",
        "{}",
        first.structured_content
    );
    let data = row_data(&first);
    assert_eq!(data["defaultBranch"], "main", "{data}");
    assert_eq!(
        data["branches"],
        json!({"dev":sha(1),"main":sha(2)}),
        "{data}"
    );
    assert_eq!(data["tags"], json!({"v1.0.0":sha(4)}), "{data}");
    let next = data["next"]["nextPage"]["query"]["queries"][0].clone();
    assert_eq!(next["operation"], "refs", "{data}");
    assert_eq!(next["page"], 2, "{data}");
    let second = call(&runtime, "ghStructure", next)
        .await
        .expect("refs page 2");
    let data = row_data(&second);
    assert_eq!(data["branches"], json!({"release":sha(3)}), "{data}");
    assert_eq!(data["tags"], json!({}), "{data}");
    assert!(data.get("next").is_none(), "{data}");
    runtime.close().await;
}

/// GS6a: refs page 30 by default (a tree page is 300 entries), and the
/// continuation does not restate the default.
#[tokio::test]
async fn refs_default_page_is_thirty() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"default_branch":"main"})))
        .mount(&server)
        .await;
    for kind in ["branches", "tags"] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/{kind}")))
            .and(query_param("per_page", "30"))
            .and(query_param("page", "1"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!([{"name":"main","commit":{"sha":"1".repeat(40)}}]))
                    .insert_header(
                        "link",
                        format!(
                            "<{}/api/v3/repos/a/b/{kind}?per_page=30&page=2>; rel=\"next\"",
                            server.uri()
                        ),
                    ),
            )
            .expect(1)
            .mount(&server)
            .await;
    }
    let workspace = Workspace::new();
    let runtime = github(&workspace, &server);
    let first = call(
        &runtime,
        "ghStructure",
        json!({"owner": "a", "repo": "b", "operation": "refs"}),
    )
    .await
    .expect("refs");
    let data = row_data(&first);
    assert_eq!(data["branches"], json!({"main":"1".repeat(40)}), "{data}");
    let next = &data["next"]["nextPage"]["query"]["queries"][0];
    assert_eq!(next["page"], 2, "{data}");
    assert!(next.get("pageSize").is_none(), "{data}");
    runtime.close().await;
}

/// Tree-only fields are rejected on refs and languages.
#[tokio::test]
async fn refs_and_languages_reject_tree_fields() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", "http://127.0.0.1:9/api/v3".into())]);
    for (operation, field) in [
        ("refs", json!({"path": "src"})),
        ("refs", json!({"materialize": true})),
        ("languages", json!({"ref": "main"})),
    ] {
        let mut query = json!({"owner": "a", "repo": "b", "operation": operation});
        for (key, value) in field.as_object().expect("field") {
            query[key] = value.clone();
        }
        let outcome = call(&runtime, "ghStructure", query).await;
        let text = match outcome {
            Ok(outcome) => outcome.structured_content.to_string(),
            Err(error) => format!("{error:?}"),
        };
        assert!(
            text.contains(&format!("only applies to tree; remove it from {operation}")),
            "{operation} {field}: {text}"
        );
    }
    runtime.close().await;
}

/// Languages: bytes of code per language, largest first.
#[tokio::test]
async fn languages_list_bytes_per_language_largest_first() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/languages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(r#"{"Shell":40,"Rust":9000,"TypeScript":700}"#)
                .insert_header("content-type", "application/json"),
        )
        .expect(1)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = github(&workspace, &server);
    let outcome = call(
        &runtime,
        "ghStructure",
        json!({"owner": "a", "repo": "b", "operation": "languages"}),
    )
    .await
    .expect("languages");
    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    let data = row_data(&outcome);
    assert_eq!(
        serde_json::to_string(&data["languages"]).expect("json"),
        r#"{"Rust":9000,"TypeScript":700,"Shell":40}"#,
        "{data}"
    );
    runtime.close().await;
}

/// Answers each `history` alias with `2025-03-<len(path)>` and the commit
/// with `2026-01-31`, so a test can tell which entry got which date.
struct DatesEcho;
impl wiremock::Respond for DatesEcho {
    fn respond(&self, request: &wiremock::Request) -> ResponseTemplate {
        let body: serde_json::Value = serde_json::from_slice(&request.body).expect("graphql body");
        let mut commit = serde_json::Map::new();
        commit.insert("committedDate".into(), json!("2026-01-31T23:59:59Z"));
        for (name, value) in body["variables"].as_object().expect("variables") {
            if name.starts_with('p') {
                let day = value.as_str().expect("path").len() % 28 + 1;
                commit.insert(
                    name.clone(),
                    json!({"nodes":[{"committedDate": format!("2025-03-{day:02}T12:00:00Z")}]}),
                );
            }
        }
        ResponseTemplate::new(200).set_body_json(json!({"data":{"repository":{"object": commit}}}))
    }
}

/// A repository `a/b` at [`SHA`] whose root lists `entries` (name, type).
async fn listing_server(entries: &[(String, &str)]) -> MockServer {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"default_branch":"main"})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex("/api/v3/repos/a/b/commits/(main|HEAD)$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha": SHA})))
        .mount(&server)
        .await;
    let rows = entries
        .iter()
        .map(|(name, kind)| json!({"name": name, "path": name, "type": kind, "size": 10}))
        .collect::<Vec<_>>();
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents"))
        .and(query_param("ref", SHA))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!(rows)))
        .mount(&server)
        .await;
    server
}

/// A listing entry `"<name> (<fields>)"` as its name and fields (the last
/// `" ("` opens them); a bare entry has none.
fn split_entry(text: &str) -> (&str, Vec<&str>) {
    match text
        .strip_suffix(')')
        .and_then(|rest| rest.rsplit_once(" ("))
    {
        Some((name, fields)) => (name, fields.split(", ").collect()),
        None => (text, Vec::new()),
    }
}

fn is_day(field: &str) -> bool {
    field.len() == 10 && field.as_bytes()[4] == b'-' && field.as_bytes()[7] == b'-'
}

/// A listing row's dates: each dated file or folder name → its day.
fn updated(row: &serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
    let mut out = serde_json::Map::new();
    for key in ["files", "folders"] {
        for text in row[key].as_array().into_iter().flatten() {
            let (name, fields) = split_entry(text.as_str().expect("entry"));
            if let Some(day) = fields.into_iter().find(|field| is_day(field)) {
                out.insert(name.to_owned(), json!(day));
            }
        }
    }
    out
}

/// A listing row's entries of `key` without their dates.
fn undated(row: &serde_json::Value, key: &str) -> serde_json::Value {
    row[key]
        .as_array()
        .into_iter()
        .flatten()
        .map(|text| {
            let (name, fields) = split_entry(text.as_str().expect("entry"));
            let kept = fields
                .into_iter()
                .filter(|field| !is_day(field))
                .collect::<Vec<_>>();
            if kept.is_empty() {
                json!(name)
            } else {
                json!(format!("{name} ({})", kept.join(", ")))
            }
        })
        .collect()
}

async fn graphql_requests(server: &MockServer) -> Vec<serde_json::Value> {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|request| request.url.path() == "/api/graphql")
        .map(|request| serde_json::from_slice(&request.body).expect("graphql body"))
        .collect()
}

/// Freshness: a tree page dates every listed file and folder at the listed
/// commit and states that commit's date, in one GraphQL request, by default.
#[tokio::test]
async fn a_tree_page_dates_every_entry_and_the_listed_commit() {
    let server = listing_server(&[
        ("one.rs".into(), "file"),
        ("src".into(), "dir"),
        ("a \"q\"\\b.md".into(), "file"),
    ])
    .await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(DatesEcho)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = github(&workspace, &server);
    let outcome = call(
        &runtime,
        "ghStructure",
        json!({"owner": "a", "repo": "b", "debug": false}),
    )
    .await
    .expect("listing");
    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    let data = row_data(&outcome);
    assert_eq!(data["commitDate"], "2026-01-31", "{data}");
    assert_eq!(
        json!(updated(&data["entries"][0])),
        json!({"one.rs": "2025-03-07", "src": "2025-03-04", "a \"q\"\\b.md": "2025-03-11"}),
        "{data}"
    );
    assert!(data.get("warnings").is_none(), "{data}");
    let requests = graphql_requests(&server).await;
    assert_eq!(requests.len(), 1, "one GraphQL request per page");
    assert_eq!(requests[0]["variables"]["oid"], SHA);
    runtime.close().await;
}

/// GS5: a page dates its first 100 entries in one GraphQL request; the
/// rest are named by count and reached by `next.expandDates` (the same
/// listing pinned to its SHA at `pageSize:100`, one row per undated 100),
/// whose every row dates its page in one request.
#[tokio::test]
async fn a_page_past_one_hundred_entries_dates_the_first_hundred_and_expands_the_rest() {
    let names = (0..250)
        .map(|n| (format!("f{n:03}.rs"), "file"))
        .collect::<Vec<_>>();
    let server = listing_server(&names).await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(DatesEcho)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = github(&workspace, &server);
    let outcome = call(&runtime, "ghStructure", json!({"owner": "a", "repo": "b"}))
        .await
        .expect("listing");
    let data = row_data(&outcome);
    let dates = updated(&data["entries"][0]);
    assert_eq!(dates.len(), 100, "{data}");
    assert!(dates.contains_key("f000.rs") && dates.contains_key("f099.rs"));
    assert!(data["entries"][0].get("updated").is_none(), "{data}");
    assert_eq!(
        graphql_requests(&server).await.len(),
        1,
        "one request per page"
    );
    let warning = data["warnings"][0].as_str().unwrap_or_default();
    assert!(
        warning.contains("150 more") && warning.contains("next.expandDates"),
        "{data}"
    );
    let expand = &data["next"]["expandDates"];
    assert_eq!(expand["tool"], "ghStructure", "{data}");
    let rows = expand["query"]["queries"].as_array().expect("rows");
    assert_eq!(
        rows.iter()
            .map(|row| (
                row["page"].clone(),
                row["pageSize"].clone(),
                row["ref"].clone()
            ))
            .collect::<Vec<_>>(),
        vec![
            (json!(2), json!(100), json!(SHA)),
            (json!(3), json!(100), json!(SHA))
        ],
        "{data}"
    );
    let replay = call(&runtime, "ghStructure", expand["query"].clone())
        .await
        .expect("expandDates replay");
    let results = replay.structured_content["results"]
        .as_array()
        .expect("rows");
    // Each replay row is one whole dated page (page 3 holds the last 50).
    let dated = results
        .iter()
        .map(|row| {
            let page = &row["data"];
            assert!(
                page.get("next")
                    .is_none_or(|next| next.get("expandDates").is_none()),
                "{page}"
            );
            updated(&page["entries"][0]).len()
        })
        .collect::<Vec<_>>();
    assert_eq!(dated, [100, 50], "{}", replay.structured_content);
    assert_eq!(
        graphql_requests(&server).await.len(),
        3,
        "one request per replay row"
    );
    runtime.close().await;
}

/// GS3: a listing states each file's size, in the structureSearch entry
/// form `"<name> (<bytes>)"`; folders stay bare and dates key bare names.
#[tokio::test]
async fn a_listing_states_each_file_size() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"default_branch":"main"})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex("/api/v3/repos/a/b/commits/(main|HEAD)$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha": SHA})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents"))
        .and(query_param("ref", SHA))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"name": "small.rs", "path": "small.rs", "type": "file", "size": 10},
            {"name": "big (1).bin", "path": "big (1).bin", "type": "file", "size": 400000},
            {"name": "src", "path": "src", "type": "dir", "size": 0}
        ])))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(DatesEcho)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = github(&workspace, &server);
    let outcome = call(&runtime, "ghStructure", json!({"owner": "a", "repo": "b"}))
        .await
        .expect("listing");
    let data = row_data(&outcome);
    assert_eq!(
        undated(&data["entries"][0], "files"),
        json!(["big (1).bin (400000)", "small.rs (10)"]),
        "{data}"
    );
    assert_eq!(
        undated(&data["entries"][0], "folders"),
        json!(["src"]),
        "{data}"
    );
    // The size comes first, then the day: each name is written once.
    let dates = updated(&data["entries"][0]);
    assert!(
        dates.contains_key("small.rs")
            && dates.contains_key("big (1).bin")
            && dates.contains_key("src"),
        "{data}"
    );
    assert!(data["entries"][0].get("updated").is_none(), "{data}");
    runtime.close().await;
}

/// A failed date request never fails the listing: every entry is listed,
/// no entry is dated, and one warning says the dates are missing.
#[tokio::test]
async fn a_graphql_failure_keeps_the_listing_and_warns_once() {
    let server = listing_server(&[("one.rs".into(), "file"), ("src".into(), "dir")]).await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": null,
            "errors": [{"message": "Something unexpected"}]
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = github(&workspace, &server);
    let outcome = call(
        &runtime,
        "ghStructure",
        json!({"owner": "a", "repo": "b", "debug": false}),
    )
    .await
    .expect("listing");
    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    let data = row_data(&outcome);
    assert_eq!(
        data["entries"][0]["files"],
        json!(["one.rs (10)"]),
        "{data}"
    );
    assert_eq!(data["entries"][0]["folders"], json!(["src"]), "{data}");
    assert!(updated(&data["entries"][0]).is_empty(), "{data}");
    assert!(data.get("commitDate").is_none(), "{data}");
    let warnings = data["warnings"].as_array().expect("warnings");
    assert_eq!(warnings.len(), 1, "{data}");
    assert!(
        warnings[0]
            .as_str()
            .is_some_and(|text| text.contains("2 entries")),
        "{data}"
    );
    runtime.close().await;
}

/// Materialize writes files for local tools; its listing stays undated.
#[tokio::test]
async fn materialize_sends_no_date_request() {
    let server = tree_server(1).await;
    let workspace = Workspace::new();
    let runtime = github(&workspace, &server);
    let outcome = call(
        &runtime,
        "ghStructure",
        json!({"owner": "a", "repo": "b", "materialize": true}),
    )
    .await
    .expect("materialize");
    let data = row_data(&outcome);
    assert!(updated(&data["entries"][0]).is_empty(), "{data}");
    assert!(graphql_requests(&server).await.is_empty());
    runtime.close().await;
}

/// GS1: a listing under `path` names each row's `dir` repo-relative (the
/// same base as `path` and `include`), and every listed name keeps its date.
#[tokio::test]
async fn a_scoped_listing_names_repo_relative_dirs_and_dates_every_name() {
    let server = listing_server(&[]).await;
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/a/b/git/trees/{SHA}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "sha": SHA, "truncated": false, "tree": [
                {"path":"pkg","type":"tree"},
                {"path":"pkg/a.rs","type":"blob","size":1},
                {"path":"pkg/src","type":"tree"},
                {"path":"pkg/src/lib.rs","type":"blob","size":1},
                {"path":"other.rs","type":"blob","size":1}
            ]
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(DatesEcho)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = github(&workspace, &server);
    let outcome = call(
        &runtime,
        "ghStructure",
        json!({"owner": "a", "repo": "b", "path": "pkg", "maxDepth": 2}),
    )
    .await
    .expect("listing");
    let data = row_data(&outcome);
    let entries = data["entries"].as_array().expect("entries");
    assert_eq!(entries[0]["dir"], "pkg", "{data}");
    assert_eq!(undated(&entries[0], "files"), json!(["a.rs (1)"]), "{data}");
    assert_eq!(undated(&entries[0], "folders"), json!(["src"]), "{data}");
    assert_eq!(entries[1]["dir"], "pkg/src", "{data}");
    // Dates key on the full path: `pkg/a.rs` (8) and `pkg/src` (7) and
    // `pkg/src/lib.rs` (14).
    assert_eq!(
        json!(updated(&entries[0])),
        json!({"a.rs": "2025-03-09", "src": "2025-03-08"}),
        "{data}"
    );
    assert_eq!(
        json!(updated(&entries[1])),
        json!({"lib.rs": "2025-03-15"}),
        "{data}"
    );
    runtime.close().await;
}

/// X9: a listing of a renamed repository at a ref (GitHub answers 301 to
/// `/repositories/<id>`) lists the canonical repository after one metadata
/// read, warns once, and leads under the canonical name.
#[tokio::test]
async fn a_renamed_repository_is_listed_and_continued_under_its_canonical_name() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/main"))
        .respond_with(ResponseTemplate::new(301).insert_header(
            "location",
            format!("{}/api/v3/repositories/1/commits/main", server.uri()),
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repositories/1/commits/main"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SHA))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"default_branch":"main","full_name":"c/d"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/c/d/contents"))
        .and(query_param("ref", SHA))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"name":"lib.rs","path":"lib.rs","type":"file","size":10}
        ])))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(DatesEcho)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = github(&workspace, &server);
    let outcome = call(
        &runtime,
        "ghStructure",
        json!({"owner": "a", "repo": "b", "ref": "main"}),
    )
    .await
    .expect("listing");
    let data = row_data(&outcome);
    assert_eq!(row_status(&outcome), "success", "{data}");
    assert_eq!(
        data["warnings"]
            .to_string()
            .matches("renamed to c/d")
            .count(),
        1,
        "{data}"
    );
    let read = &data["hints"]["read"]["query"]["queries"][0];
    assert_eq!(
        (read["owner"].as_str(), read["repo"].as_str()),
        (Some("c"), Some("d")),
        "{data}"
    );
    let dates = graphql_requests(&server).await;
    assert_eq!(dates[0]["variables"]["owner"], "c", "{dates:?}");
    runtime.close().await;
}

/// X9: without a ref, the default-branch lookup already names the
/// canonical repository: no extra metadata request.
#[tokio::test]
async fn a_renamed_repository_without_a_ref_costs_no_extra_metadata_read() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"default_branch":"main","full_name":"c/d"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/HEAD"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SHA))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/c/d/contents"))
        .and(query_param("ref", SHA))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"name":"lib.rs","path":"lib.rs","type":"file","size":10}
        ])))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(DatesEcho)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = github(&workspace, &server);
    let outcome = call(&runtime, "ghStructure", json!({"owner": "a", "repo": "b"}))
        .await
        .expect("listing");
    let data = row_data(&outcome);
    assert!(
        data["warnings"].to_string().contains("renamed to c/d"),
        "{data}"
    );
    assert_eq!(
        data["hints"]["read"]["query"]["queries"][0]["owner"], "c",
        "{data}"
    );
    runtime.close().await;
}
