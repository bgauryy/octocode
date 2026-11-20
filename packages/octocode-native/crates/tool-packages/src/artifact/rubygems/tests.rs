use super::super::query::{discovery, exact_query};
use super::super::test_http::RouteHttp;
use super::super::test_http::test_budget;
use super::*;

use octocode_test_support::json_fixture;
use serde_json::json;

/// `count` rubygems search rows named `gem<first>`…
fn gem_rows(first: usize, count: usize) -> Value {
    Value::Array(
        (first..first + count)
            .map(|n| json!({"name": format!("gem{n}"), "version": "1.0.0"}))
            .collect(),
    )
}

/// A rubygems discovery page reads the API pages its offset names
/// directly (30 rows each): page 3 of 50 starts at row 100, API page 4
/// row 10, and never re-reads pages 1–3. An API page with more rows
/// than that fails loudly instead of misplacing every later page.
#[tokio::test]
async fn rubygems_pages_read_only_the_api_pages_their_offset_names() {
    let search = |page: usize| format!("/api/v1/search.json?query=json&page={page}");
    let http = RouteHttp::new()
        .json(&search(4), gem_rows(90, 30))
        .json(&search(5), gem_rows(120, 30))
        .json(&search(6), gem_rows(150, 12));
    let b = test_budget();
    let client = RegistryClient::new(&http, &b);
    let query = discovery(ArtifactType::Rubygems, json_fixture(r#"{"page": 6}"#));
    let state = super::super::ArtifactProviderState {
        offset: Some(110),
        ..Default::default()
    };
    let page = rubygems(&query, &state, &client).await.expect("page 3");
    assert_eq!(http.targets(), [search(4), search(5)]);
    let names = page
        .artifacts
        .iter()
        .map(|row| row.package_name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(names.len(), 20);
    assert_eq!((names[0], names[19]), ("gem110", "gem129"));
    assert_eq!(page.next_state.and_then(|next| next.offset), Some(130));
    // The short last API page ends the results.
    let state = super::super::ArtifactProviderState {
        offset: Some(150),
        ..Default::default()
    };
    let last = rubygems(&query, &state, &client).await.expect("last page");
    assert_eq!(http.targets()[2..], [search(6)]);
    assert_eq!(last.artifacts.len(), 12);
    assert_eq!(last.next_state, None);
    let oversized = RouteHttp::always(gem_rows(0, 31));
    let client = RegistryClient::new(&oversized, &b);
    let error = rubygems(&query, &Default::default(), &client)
        .await
        .expect_err("31 rows");
    assert_eq!(error.code, ErrorCode::ProviderError);
    assert!(error.message.contains("31 rows"), "{}", error.message);
}

/// An exact gem row states its release day and runtime dependencies;
/// a source link into a GitHub tree is the repository root, with the
/// release ref it names.
#[tokio::test]
async fn rubygems_exact_rows_state_the_release_day_dependencies_and_repository() {
    let http = RouteHttp::always(json_fixture(
        r#"{
            "name": "rails",
            "version": "8.1.4",
            "info": "Full-stack web framework",
            "licenses": ["MIT"],
            "version_created_at": "2026-09-24T14:22:14.180Z",
            "source_code_uri": "https://github.com/rails/rails/tree/v8.1.4",
            "dependencies": {
                "development": [{"name": "dev", "requirements": ">= 0"}],
                "runtime": [
                    {"name": "actioncable", "requirements": "= 8.1.4"},
                    {"name": "bundler", "requirements": ">= 1.15.0"}
                ]
            }
        }"#,
    ));
    let b = test_budget();
    let client = RegistryClient::new(&http, &b);
    let page = rubygems(
        &exact_query(ArtifactType::Rubygems, "rails"),
        &Default::default(),
        &client,
    )
    .await
    .expect("rails");
    let item = &page.artifacts[0];
    assert_eq!(item.published_at.as_deref(), Some("2026-09-24"));
    assert_eq!(
        item.dependencies,
        Some(vec![
            "actioncable = 8.1.4".to_owned(),
            "bundler >= 1.15.0".to_owned()
        ])
    );
    assert_eq!(
        item.repository.as_deref(),
        Some("https://github.com/rails/rails")
    );
    assert_eq!(item.source_ref.as_deref(), Some("v8.1.4"));
    assert_eq!(item.repository_directory, None);
    // A search row names neither: both stay unknown.
    let row = gem(json_fixture(r#"{"name": "x", "version": "1"}"#)
        .as_object()
        .expect("row"))
    .expect("row");
    assert_eq!((row.dependencies, row.published_at), (None, None));
    let mut monorepo = [gem(json_fixture(
        r#"{
            "name": "x",
            "version": "1",
            "source_code_uri": "https://github.com/o/r/tree/main/gems/x"
        }"#,
    )
    .as_object()
    .expect("row"))
    .expect("row")];
    let tags = super::super::test_http::KnownTags::new(vec!["o/r@main"]);
    super::super::test_http::settle_links(&mut monorepo, &tags).await;
    let [monorepo] = monorepo;
    assert_eq!(
        monorepo.repository.as_deref(),
        Some("https://github.com/o/r")
    );
    assert_eq!(monorepo.repository_directory.as_deref(), Some("gems/x"));
    assert_eq!(monorepo.source_ref, None, "a branch is no release ref");
}

/// A gem whose source link names no release ref (nokogiri's repository
/// root) is pinned to the upstream tag of its version: `v1.19.4`.
#[tokio::test]
async fn a_gem_without_a_release_ref_is_pinned_to_its_upstream_tag() {
    let http = RouteHttp::always(json_fixture(
        r#"{
            "name": "nokogiri",
            "version": "1.19.4",
            "source_code_uri": "https://github.com/sparklemotion/nokogiri/tree/v1.19.4/lib",
            "metadata": {
                "source_code_uri": "https://github.com/sparklemotion/nokogiri"
            }
        }"#,
    ));
    let b = test_budget();
    let tags = super::super::test_http::KnownTags::new(vec!["sparklemotion/nokogiri@v1.19.4"]);
    let client = RegistryClient {
        tags: Some(&tags),
        ..RegistryClient::new(&http, &b)
    };
    for version in [None, Some("1.19.4")] {
        let mut query = json_fixture(
            r#"{
                "operation": "resolve",
                "ecosystem": "rubygems",
                "packageName": "nokogiri"
            }"#,
        );
        if let Some(version) = version {
            query["version"] = json!(version);
        }
        let page = rubygems(
            &super::super::query::artifact_query(query, None),
            &Default::default(),
            &client,
        )
        .await
        .expect("nokogiri");
        let item = &page.artifacts[0];
        assert_eq!(
            item.source_ref.as_deref(),
            Some("commit-of-v1.19.4"),
            "{version:?}"
        );
        assert_eq!(item.release_tag.as_deref(), Some("v1.19.4"));
        assert!(item.source_tag, "{version:?}");
    }
}

/// A source link naming the version's tag (`…/tree/v7.1.3`) is the gem's
/// claim, checked like any declared tag: the tag GitHub has pins the commit
/// it names (`verification: tag`), never the tag's name as the registry's
/// unchecked ref (LEFTOVERS P5); with no GitHub check, the claim stays.
#[tokio::test]
async fn a_source_link_naming_the_versions_tag_is_checked_against_github() {
    let http = RouteHttp::always(json_fixture(
        r#"{
            "name": "rails",
            "version": "7.1.3",
            "source_code_uri": "https://github.com/rails/rails/tree/v7.1.3"
        }"#,
    ));
    let b = test_budget();
    let tags = super::super::test_http::KnownTags::new(vec!["rails/rails@v7.1.3"]);
    let checked = RegistryClient {
        tags: Some(&tags),
        ..RegistryClient::new(&http, &b)
    };
    let page = rubygems(
        &exact_query(ArtifactType::Rubygems, "rails"),
        &Default::default(),
        &checked,
    )
    .await
    .expect("rails");
    let item = &page.artifacts[0];
    assert_eq!(item.source_ref.as_deref(), Some("commit-of-v7.1.3"));
    assert_eq!(item.release_tag.as_deref(), Some("v7.1.3"));
    assert!(item.source_tag);
    let unchecked = rubygems(
        &exact_query(ArtifactType::Rubygems, "rails"),
        &Default::default(),
        &RegistryClient::new(&http, &b),
    )
    .await
    .expect("rails");
    let item = &unchecked.artifacts[0];
    assert_eq!(item.source_ref.as_deref(), Some("v7.1.3"));
    assert!(!item.source_tag);
}

/// A search summary states no directory, so a row's source link below a ref
/// asks GitHub nothing (rack's `…/tree/main` link was one commit check per
/// search page): the registry's one request is the call's. The details view
/// still places the directory.
#[tokio::test]
async fn a_search_summary_asks_github_nothing_for_a_link_directory() {
    let row = json!([{"name": "rack", "version": "3.1.8",
        "source_code_uri": "https://github.com/rack/rack/tree/main/lib"}]);
    let http = RouteHttp::new().json("/api/v1/search.json?query=json&page=1", row);
    let tags = super::super::test_http::KnownTags::new(vec!["rack/rack@main"]);
    let summary = discovery(ArtifactType::Rubygems, json!({}));
    let page = super::super::test_http::execute(&http, &summary, Some(&tags))
        .await
        .expect("summary page");
    assert_eq!(page.artifacts.len(), 1);
    assert!(tags.seen().is_empty(), "{:?}", tags.seen());
    let details = discovery(ArtifactType::Rubygems, json!({"view": "details"}));
    let page = super::super::test_http::execute(&http, &details, Some(&tags))
        .await
        .expect("details page");
    assert_eq!(
        page.artifacts[0].repository_directory.as_deref(),
        Some("lib")
    );
    assert_eq!(tags.seen(), ["rack/rack@main"]);
}
