//! `limit` on ghStructure: tree entries per page (the tree is held whole,
//! so no provider maximum applies) and refs per GitHub page (GitHub's
//! documented `per_page` maximum, 100). An omitted `limit` is the page size
//! each operation served before `limit` existed, read from the contract;
//! a small-`limit` walk lists every entry or ref exactly once.
use super::*;
use crate::contracts::query_limits::gh_structure as limits;
use wiremock::matchers::query_param;

/// Each `files` name a page lists, in order.
fn listed_files(data: &Value) -> Vec<String> {
    data["files"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|group| group["files"].as_array().into_iter().flatten())
        .map(|name| name.as_str().unwrap_or_default().to_owned())
        .collect()
}

/// A root of 5 files at `limit: 2` lists 2, 2 and 1 entries; each
/// `nextPage` carries the limit and the walk lists every file once.
#[tokio::test]
async fn a_small_tree_limit_walk_lists_every_entry_once() {
    let server = server().await;
    let names = (1..=5).map(|n| format!("f{n}.rs")).collect::<Vec<_>>();
    let contents = names
        .iter()
        .map(|name| json!({"name":name,"path":name,"type":"file","size":1}))
        .collect::<Vec<_>>();
    mount_json(&server, "/api/v3/repos/a/b/contents", 200, json!(contents)).await;
    let mut row = json!({"operation":"tree","owner":"a","repo":"b","limit":2});
    let mut listed = Vec::new();
    let mut sizes = Vec::new();
    for _ in 0..=names.len() {
        let out = list(&server, row.clone()).await.expect("listing");
        let page = listed_files(&out.data);
        sizes.push(page.len());
        listed.extend(page);
        let Some(next) = out.data.pointer("/next/nextPage/input/queries/0") else {
            break;
        };
        assert_eq!(next["limit"], 2, "the cursor carries limit: {}", out.data);
        row = next.clone();
    }
    assert_eq!(sizes, [2, 2, 1]);
    let expected = names
        .iter()
        .map(|name| format!("{name} (1)"))
        .collect::<Vec<_>>();
    assert_eq!(listed, expected);
}

/// The tree page holds `limit` entries, 300 when omitted (the contract
/// default, today's page size); any `limit` pages the held tree.
#[test]
fn a_tree_page_holds_limit_entries() {
    assert_eq!(limits::tree::LIMIT_DEFAULT, 300);
    let entries = (0..301)
        .map(|n| TreeEntry {
            path: format!("f{n:03}.rs"),
            kind: EntryKind::File,
            size: Some(1),
        })
        .collect::<Vec<_>>();
    let tree = |limit: Option<u64>| {
        let mut row = json!({"operation":"tree","owner":"a","repo":"b"});
        if let Some(limit) = limit {
            row["limit"] = json!(limit);
        }
        serde_json::from_value::<GhStructureQueryTree>(row).expect("tree query")
    };
    for (limit, rows, pages) in [(None, 300, 2), (Some(7), 7, 43), (Some(1_000), 301, 1)] {
        let query = tree(limit);
        let page = ListingPage::of(&query, &entries);
        assert_eq!(
            (page.entries.len(), page.total_pages),
            (rows, pages),
            "{limit:?}"
        );
    }
}

/// Branches and tags page at `limit` each: GitHub's branch pages are
/// asked `per_page=2`, the ordered tag list is cut into 2-tag pages, and
/// the walk lists every branch and tag once.
#[tokio::test]
async fn a_small_refs_limit_walk_lists_every_ref_once() {
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
        json!({"default_branch":"main","full_name":"a/b"}),
    )
    .await;
    let branches = ["b1", "b2", "b3"];
    for (index, chunk) in branches.chunks(2).enumerate() {
        let page = index + 1;
        let body = chunk
            .iter()
            .map(|name| json!({"name":name,"commit":{"sha":SHA}}))
            .collect::<Vec<_>>();
        let mut response = ResponseTemplate::new(200).set_body_json(json!(body));
        if page == 1 {
            let link = format!(
                "<{}/api/v3/repos/a/b/branches?page=2&per_page=2>; rel=\"next\"",
                server.uri()
            );
            response = response.insert_header("link", link.as_str());
        }
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/a/b/branches"))
            .and(query_param("page", page.to_string()))
            .and(query_param("per_page", "2"))
            .respond_with(response)
            .mount(&server)
            .await;
    }
    // Past the last branch page GitHub answers an empty page (tags run on).
    mount_json(&server, "/api/v3/repos/a/b/branches", 200, json!([])).await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/git/matching-refs/tags",
        200,
        json!(
            (1..=5)
                .map(|n| json!({"ref": format!("refs/tags/v{n}"), "object": {"sha": SHA}}))
                .collect::<Vec<_>>()
        ),
    )
    .await;
    let mut row = json!({"operation":"refs","owner":"a","repo":"b","limit":2});
    let (mut listed_branches, mut listed_tags) = (Vec::new(), Vec::new());
    for _ in 0..=5 {
        let out = list(&server, row.clone()).await.expect("refs page");
        let data = &out.data;
        let tags = data["tags"].as_array().cloned().unwrap_or_default();
        assert!(tags.len() <= 2, "{data}");
        listed_tags.extend(
            tags.iter()
                .map(|tag| tag.as_str().unwrap_or_default().to_owned()),
        );
        if let Some(page) = data["branches"].as_object() {
            assert!(page.len() <= 2, "{data}");
            listed_branches.extend(page.keys().cloned());
        }
        let Some(next) = data.pointer("/next/nextPage/input/queries/0") else {
            break;
        };
        assert_eq!(next["limit"], 2, "the cursor carries limit: {data}");
        row = next.clone();
    }
    assert_eq!(listed_branches, branches);
    assert_eq!(listed_tags, ["v5", "v4", "v3", "v2", "v1"]);
}

/// refs `limit` defaults to 100 (today's page size, GitHub's maximum) and
/// admission rejects past GitHub's documented `per_page` maximum; a tree
/// `limit` has no maximum (the held tree pages locally), and 0 is no page.
#[test]
fn admission_bounds_refs_by_github_and_leaves_tree_open() {
    assert_eq!(limits::refs::LIMIT_DEFAULT, 100);
    assert_eq!(limits::refs::LIMIT_MAXIMUM, octocode_github::MAX_PER_PAGE);
    let admit = |operation: &str, limit: Option<u64>| {
        let mut row = json!({"operation":operation,"owner":"a","repo":"b"});
        if let Some(limit) = limit {
            row["limit"] = json!(limit);
        }
        crate::contracts::validate_query("ghStructure", row)
    };
    assert_eq!(admit("refs", None).expect("default")["limit"], 100);
    assert_eq!(admit("tree", None).expect("default")["limit"], 300);
    assert!(admit("refs", Some(100)).is_ok());
    assert!(admit("refs", Some(101)).is_err());
    assert!(admit("tree", Some(100_000)).is_ok());
    for operation in ["refs", "tree"] {
        assert!(admit(operation, Some(0)).is_err(), "{operation}");
    }
}
