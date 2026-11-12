//! Owner listing order, archive filtering, pagination and empty-scope diagnostics.
use super::*;
use crate::response::result::ToolData;

#[tokio::test]
async fn owner_listing_updated_sort_tracks_pushes_and_excludes_archived() {
    let server = MockServer::builder().start().await;
    recently_pushed_org_repos(
        &server,
        json!([repo_item("live", false), repo_item("old", true)]),
    )
    .await;
    let out = run(
        &server,
        json!({"operation":"list","owner":"o","sort":{"by":"pushedAt","order":"desc"}}),
    )
    .await
    .expect("owner listing");
    let names = out.data["repositories"]
        .as_array()
        .expect("rows")
        .iter()
        .map(|row| row["repo"].as_str().unwrap_or_default().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["live".to_owned()], "{}", out.data);
    assert!(
        out.data.get("order").is_none(),
        "the description states the order"
    );
}

/// The listing API's own order is creation (oldest first); the
/// default best-match listing asks for the most recently pushed.
#[tokio::test]
async fn owner_listing_default_sort_is_recent_activity_not_creation() {
    let server = MockServer::builder().start().await;
    recently_pushed_org_repos(&server, json!([repo_item("live", false)])).await;
    let out = run(&server, json!({"operation":"list","owner":"o"}))
        .await
        .expect("owner listing");
    assert_eq!(out.data["repositories"][0]["repo"], "live", "{}", out.data);
    assert!(out.data.get("order").is_none(), "{}", out.data);
}

#[tokio::test]
async fn owner_listing_reads_past_archived_pages_and_follows_the_link_cursor() {
    // A full provider page of mostly archived repositories plus a Link
    // next must keep paging instead of ending with a fabricated total.
    let server = MockServer::builder().start().await;
    let link = |page: usize| {
        format!(
            "<{}/api/v3/orgs/o/repos?page={page}&per_page=3>; rel=\"next\"",
            server.uri()
        )
    };
    for (page, rows, next) in [
        (
            1,
            vec![
                repo_item("a", false),
                repo_item("x1", true),
                repo_item("x2", true),
            ],
            Some(2),
        ),
        (
            2,
            vec![
                repo_item("x3", true),
                repo_item("b", false),
                repo_item("c", false),
            ],
            Some(3),
        ),
        (3, vec![repo_item("d", false)], None),
    ] {
        let mut response = ResponseTemplate::new(200).set_body_json(json!(rows));
        if let Some(next) = next {
            response = response.insert_header("link", link(next).as_str());
        }
        Mock::given(method("GET"))
            .and(path("/api/v3/orgs/o/repos"))
            .and(query_param("page", page.to_string()))
            .respond_with(response)
            .mount(&server)
            .await;
    }
    let names = |out: &ToolData| {
        out.data["repositories"]
            .as_array()
            .expect("rows")
            .iter()
            .map(|row| row["repo"].as_str().unwrap_or_default().to_owned())
            .collect::<Vec<_>>()
    };
    let first = run(&server, json!({"operation":"list","owner":"o"}))
        .await
        .expect("owner listing");
    assert_eq!(names(&first), vec!["a", "b", "c", "d"], "{}", first.data);
    assert!(
        first.data["pagination"].get("providerPagesRead").is_none(),
        "pagination has the one shape: {}",
        first.data
    );
    assert_eq!(first.data["pagination"]["hasMore"], false);
    assert!(first.data["next"].get("nextPage").is_none());
}

/// An owner whose listing answers with no repositories (an organization that
/// moved its repositories away) says exactly that, and never offers the
/// keyword hint of a search; an owner whose repositories are all archived
/// names the archive filter instead.
#[tokio::test]
async fn an_owner_with_no_repositories_says_so() {
    let server = MockServer::builder().start().await;
    recently_pushed_org_repos(&server, json!([])).await;
    let empty = run(&server, json!({"operation":"list","owner":"o"}))
        .await
        .expect("owner listing");
    assert_eq!(empty.status, Some("empty"));
    let hints = empty.data["hints"].to_string();
    assert!(
        hints.contains("`o` exists but lists no repositories"),
        "{hints}"
    );
    assert!(!hints.contains("ANDed"), "{hints}");

    let archived = MockServer::builder().start().await;
    recently_pushed_org_repos(&archived, json!([repo_item("old", true)])).await;
    let filtered = run(&archived, json!({"operation":"list","owner":"o"}))
        .await
        .expect("owner listing");
    assert!(
        !filtered.data["hints"]
            .to_string()
            .contains("exists but lists no repositories"),
        "{}",
        filtered.data
    );
}
