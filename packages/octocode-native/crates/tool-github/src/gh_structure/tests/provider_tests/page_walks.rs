//! A listing costs the listing: no request names a date, so none is asked of
//! GitHub, and a page holds the query's whole `limit`.
use super::*;
use octocode_runtime_base::contracts::query_limits::gh_structure as limits;
use octocode_test_support::json_fixture;

/// Each `files` entry of one page under its repo-relative path, in order.
fn entries(data: &Value) -> Vec<String> {
    listed_paths(data)
}

/// Requests the mock server saw to `route`.
async fn requests_to(server: &MockServer, route: &str) -> usize {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|request| request.url.path() == route)
        .count()
}

fn root_of(count: usize) -> Vec<Value> {
    (0..count)
        .map(|index| {
            let name = format!("f{index:04}.rs");
            json!({"name":name,"path":name,"type":"file","size":1})
        })
        .collect()
}

/// A listing inside the default `limit` is one page: every entry, its size
/// and nothing a date lookup would have added, with no GraphQL request.
#[tokio::test]
async fn a_listing_makes_no_date_lookup_and_honors_its_default_limit() {
    let server = server().await;
    let total = limits::tree::LIMIT_DEFAULT - 1;
    mount_folder_tree(
        &server,
        "/api/v3/repos/a/b",
        SHA,
        "",
        &json!(root_of(total)),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents",
        200,
        json!(root_of(total)),
    )
    .await;
    let row = json_fixture(
        r#"{
            "operation": "tree",
            "owner": "a",
            "repo": "b",
            "ref": "main",
            "maxDepth": 1
        }"#,
    );
    let out = list(&server, row).await.expect("tree page");
    let data = &out.data;
    assert_eq!(requests_to(&server, "/api/graphql").await, 0, "{data}");
    let expected = (0..total)
        .map(|index| format!("f{index:04}.rs (1)"))
        .collect::<Vec<_>>();
    assert_eq!(entries(data), expected);
    assert!(data.get("pagination").is_none(), "{data}");
    assert!(data.get("warnings").is_none(), "{data}");
    octocode_runtime_base::test_support::assert_row_contract(ToolId::GhStructure, data);
}

/// A walk past one page lists every entry exactly once at the requested
/// `limit`, pinned to the listed commit, with `offset` and `total` placing
/// each call it takes and no GraphQL request on any page.
#[tokio::test]
async fn a_walk_lists_every_entry_once_at_its_limit() {
    let server = server().await;
    let limit = limits::tree::LIMIT_DEFAULT;
    let total = limit * 2 + 7;
    mount_folder_tree(
        &server,
        "/api/v3/repos/a/b",
        SHA,
        "",
        &json!(root_of(total)),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents",
        200,
        json!(root_of(total)),
    )
    .await;
    let mut row = json_fixture(
        r#"{
            "operation": "tree",
            "owner": "a",
            "repo": "b",
            "ref": "main",
            "maxDepth": 1
        }"#,
    );
    let mut listed = Vec::new();
    let mut sizes = Vec::new();
    loop {
        let out = list(&server, row).await.expect("tree page");
        assert!(out.data.get("warnings").is_none(), "{}", out.data);
        let page = entries(&out.data);
        sizes.push(page.len());
        listed.extend(page);
        let Some(next) = out.data.pointer("/next/nextPage/input/queries/0") else {
            break;
        };
        assert_eq!(out.data["pagination"]["total"], total, "{}", out.data);
        assert_eq!(next["ref"], SHA);
        assert_eq!(next["maxDepth"], 1);
        assert!(sizes.len() < 4, "nonterminating tree: {}", out.data);
        row = next.clone();
    }
    assert_eq!(sizes, [limit, limit, 7]);
    let expected = (0..total)
        .map(|index| format!("f{index:04}.rs (1)"))
        .collect::<Vec<_>>();
    assert_eq!(listed, expected);
    assert_eq!(requests_to(&server, "/api/graphql").await, 0);
}

/// A materialize names no depth and takes every level; the pages of its
/// listing are that listing. Walked at any path, they hold every entry exactly
/// once and write nothing more.
#[tokio::test]
async fn a_materialize_without_a_depth_pages_its_whole_listing_once() {
    for root in ["", "crates/ignore"] {
        let server = server().await;
        let total_files = 23;
        let tree = (0..total_files)
            .map(
                |n| json!({"path":format!("src/deep/f{n:02}.rs"),"type":"blob","size":8,"sha":"0"}),
            )
            .chain(["src", "src/deep"].map(|dir| json!({"path":dir,"type":"tree","sha":"0"})))
            .collect::<Vec<_>>();
        let (route, prefix) = if root.is_empty() {
            (format!("/git/trees/{SHA}$"), String::new())
        } else {
            (
                format!("/git/trees/{SHA}(:|%3A){}$", root.replace('/', "(/|%2F)")),
                format!("{root}/"),
            )
        };
        Mock::given(method("GET"))
            .and(wiremock::matchers::path_regex(route))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"sha":SHA,"truncated":false,"tree":tree})),
            )
            .mount(&server)
            .await;
        for n in 0..total_files {
            let file = format!("{prefix}src/deep/f{n:02}.rs");
            crate::test_support::backed::mount_content(&server, &file, "fn u(){}").await;
        }
        let mut row = json!({"operation":"tree","owner":"a","repo":"b","ref":"main","path":root,
            "materialize":true,"limit":10});
        let mut listed = Vec::new();
        let mut pages = 0;
        loop {
            let out = list(&server, row).await.expect("tree page");
            pages += 1;
            listed.extend(entries(&out.data));
            let Some(next) = out.data.pointer("/next/nextPage/input/queries/0") else {
                break;
            };
            assert_eq!(out.data["pagination"]["total"], 25, "{}", out.data);
            assert_eq!(next["ref"], SHA);
            assert!(next.get("materialize").is_none(), "{next}");
            assert!(pages < 4, "nonterminating tree: {}", out.data);
            row = next.clone();
        }
        assert_eq!(pages, 3, "{root}");
        listed.sort();
        let mut expected = (0..total_files)
            .map(|n| format!("{prefix}src/deep/f{n:02}.rs (8)"))
            .chain(["src/", "src/deep/"].map(|dir| format!("{prefix}{dir}")))
            .collect::<Vec<_>>();
        expected.sort();
        assert_eq!(listed, expected, "{root}");
        let reads = server
            .received_requests()
            .await
            .unwrap_or_default()
            .iter()
            .filter(|request| request.url.path().contains("/contents/"))
            .count();
        assert_eq!(reads, total_files, "only the first page writes: {root}");
    }
}

/// An `include` that hits one file in most directories and several in a few
/// walks flat rows and groups together: across the pages every matched file
/// is listed exactly once under its repo-relative path, a directory with one
/// entry on a page is a flat row and a directory with several a group (a
/// directory a page boundary cuts is whichever its share of each page is).
#[tokio::test]
async fn a_walk_of_flat_rows_and_groups_lists_every_entry_once() {
    let server = server().await;
    let dirs = 9;
    // Every third package holds two matches, the others one.
    let files = (0..dirs)
        .flat_map(|n| {
            let dir = format!("packages/p{n}");
            let twin = (n % 3 == 0).then(|| format!("{dir}/Cargo.lock"));
            [Some(format!("{dir}/Cargo.toml")), twin]
        })
        .flatten()
        .collect::<Vec<_>>();
    let tree = files
        .iter()
        .map(|file| json!({"path":file,"type":"blob","size":7,"sha":"0"}))
        .chain(
            (0..dirs)
                .map(|n| format!("packages/p{n}"))
                .chain(["packages".to_owned()])
                .map(|dir| json!({"path":dir,"type":"tree","sha":"0"})),
        )
        .collect::<Vec<_>>();
    mount_json(
        &server,
        format!("/api/v3/repos/a/b/git/trees/{SHA}"),
        200,
        json!({"sha":SHA,"truncated":false,"tree":tree}),
    )
    .await;
    let limit = 5;
    let mut row = json!({"operation":"tree","owner":"a","repo":"b","ref":"main",
        "include":["**/Cargo.*"],"limit":limit});
    let mut listed = Vec::new();
    let (mut flat_rows, mut groups) = (0, 0);
    loop {
        let out = list(&server, row).await.expect("tree page");
        let data = &out.data;
        octocode_runtime_base::test_support::assert_row_contract(ToolId::GhStructure, data);
        let page = entries(data);
        assert!(page.len() <= limit, "{data}");
        let rows = data["files"].as_array().expect("files");
        flat_rows += rows.iter().filter(|row| row.is_string()).count();
        groups += rows.iter().filter(|row| row.is_object()).count();
        // No directory is stated twice on a page: one flat row or one group.
        let mut stated = rows
            .iter()
            .map(|row| match row.as_str() {
                Some(path) => octocode_runtime_base::response::groups::split(
                    octocode_tools_shared::selection::row_units::entry_name(path),
                )
                .0
                .to_owned(),
                None => row["dir"].as_str().expect("dir").to_owned(),
            })
            .collect::<Vec<_>>();
        let count = stated.len();
        stated.sort();
        stated.dedup();
        assert_eq!(stated.len(), count, "{data}");
        listed.extend(page);
        let Some(next) = data.pointer("/next/nextPage/input/queries/0") else {
            break;
        };
        assert!(listed.len() < files.len(), "nonterminating tree: {data}");
        row = next.clone();
    }
    let mut expected = files
        .iter()
        .map(|file| format!("{file} (7)"))
        .collect::<Vec<_>>();
    expected.sort();
    listed.sort();
    assert_eq!(listed, expected);
    assert!(
        flat_rows > 0 && groups > 0,
        "{flat_rows} flat, {groups} groups"
    );
}
