//! Repository rows, empty pages and their broadening, and the missing-owner hint.
use super::*;
use octocode_test_support::json_fixture;

/// A missing owner is not told to broaden its keywords: the error names
/// the login, and no search change repairs it.

#[test]
fn zero_repositories_are_empty_and_only_a_complete_search_broadens() {
    let query: GhSearchRepoQuery = serde_json::from_value(json_fixture(
        r#"{
            "keywords": ["x"],
            "operation": "search",
            "filters": {"language": "rust"}
        }"#,
    ))
    .expect("query");
    let complete = repository_output(
        json_fixture(r#"{"repositories": []}"#),
        &query,
        true,
        false,
        false,
    );
    assert_eq!(complete.status, Some("empty"));
    let rows = &octocode_runtime_base::test_support::checks::related_call(
        &complete.data,
        "ghSearchRepo",
        |_| true,
    )["input"]["queries"];
    // The search keeps its subject: no variant drops the lone keyword to
    // list every repository of a language, which answers nothing asked.
    assert_eq!(rows.as_array().map(Vec::len), Some(1), "{}", complete.data);
    assert_eq!(rows[0]["keywords"], json_fixture(r#"["x"]"#));
    assert!(rows[0].get("filters").is_none());
    octocode_runtime_base::test_support::checks::assert_leads_valid(&complete.data);

    // GitHub's incomplete page: empty (exit 1), no broadening; its retry
    // is added with the partial marking.
    let incomplete = repository_output(
        json_fixture(r#"{"repositories": []}"#),
        &query,
        true,
        true,
        false,
    );
    assert_eq!(incomplete.status, Some("empty"));
    assert!(incomplete.data.get("next").is_none());
    // A capped result has rows beyond the cap: not empty.
    let capped = repository_output(
        json_fixture(r#"{"repositories": []}"#),
        &query,
        true,
        false,
        true,
    );
    assert_eq!(capped.status, None);
    assert!(capped.data.get("next").is_none());
}

/// A search past GitHub's 1,000-result window leads to the same search
/// split in two by creation date: the halves are adjacent at whole seconds
/// (disjoint, nothing between them), keep the search's own bounds, and a
/// half still capped splits again inside its own window.
#[test]
fn a_capped_search_splits_by_creation_date() {
    use serde_json::Value;
    let now = octocode_runtime_base::civil_date::unix_secs();
    let split = |query: &GhSearchRepoQuery| {
        let call = leads::split_by_created(query, now).expect("a split");
        let rows = call["input"]["queries"].as_array().expect("rows").clone();
        assert_eq!(rows.len(), 2, "{call}");
        let bound = |row: &Value, key: &str| {
            row["filters"]["createdAt"][key].as_str().map(|text| {
                octocode_runtime_base::civil_date::parse_rfc3339(text)
                    .expect("a date-time")
                    .0
            })
        };
        let (to, from) = (bound(&rows[0], "to"), bound(&rows[1], "from"));
        assert_eq!(from, to.map(|to| to + 1), "adjacent halves: {call}");
        for row in &rows {
            assert_eq!(row["keywords"], json!(["http client"]), "{call}");
        }
        rows
    };
    let query: GhSearchRepoQuery = serde_json::from_value(json!({
        "operation": "search", "keywords": ["http client"]
    }))
    .expect("query");
    let rows = split(&query);
    // The first split is open at both ends: every match falls in a half.
    assert!(
        rows[0]["filters"]["createdAt"].get("from").is_none(),
        "{rows:?}"
    );
    assert!(
        rows[1]["filters"]["createdAt"].get("to").is_none(),
        "{rows:?}"
    );
    // A half splits inside its own bounds.
    let lower: GhSearchRepoQuery = serde_json::from_value(rows[0].clone()).expect("half");
    let inner = split(&lower);
    assert_eq!(
        inner[1]["filters"]["createdAt"]["to"], rows[0]["filters"]["createdAt"]["to"],
        "{inner:?}"
    );
    // The lead rides a capped first page only.
    let capped = repository_output(
        json!({"repositories": [{"repo": "o/r"}]}),
        &query,
        false,
        false,
        true,
    );
    octocode_runtime_base::test_support::checks::related_call(
        &capped.data,
        "ghSearchRepo",
        |input| {
            input["queries"]
                .as_array()
                .is_some_and(|rows| rows.len() == 2)
        },
    );
    octocode_runtime_base::test_support::checks::assert_leads_valid(&capped.data);
    let listing: GhSearchRepoQuery =
        serde_json::from_value(json!({"operation": "list", "owner": "o"})).expect("listing");
    assert!(leads::split_by_created(&listing, now).is_none());
}

/// An empty search inside one repository with a lone keyword broadens
/// to that repository's own row; each filter drops by its wire name.
#[test]
fn a_lone_keyword_in_one_repository_broadens_to_its_row() {
    let query: GhSearchRepoQuery = serde_json::from_value(json_fixture(
        r#"{
            "owner": "o",
            "repo": "r",
            "keywords": ["x"],
            "operation": "search"
        }"#,
    ))
    .expect("query");
    let empty = repository_output(
        json_fixture(r#"{"repositories": []}"#),
        &query,
        true,
        false,
        false,
    );
    let row = &octocode_runtime_base::test_support::checks::related_call(
        &empty.data,
        "ghSearchRepo",
        |_| true,
    )["input"]["queries"][0];
    assert_eq!(row["owner"], "o");
    assert_eq!(row["repo"], "r");
    assert!(row.get("keywords").is_none());
    octocode_runtime_base::test_support::checks::assert_leads_valid(&empty.data);
    let matched: GhSearchRepoQuery = serde_json::from_value(json_fixture(
        r#"{
            "keywords": ["x"],
            "matchIn": ["name"],
            "operation": "search"
        }"#,
    ))
    .expect("query");
    let empty = repository_output(
        json_fixture(r#"{"repositories": []}"#),
        &matched,
        true,
        false,
        false,
    );
    let row = &octocode_runtime_base::test_support::checks::related_call(
        &empty.data,
        "ghSearchRepo",
        |_| true,
    )["input"]["queries"][0];
    assert_eq!(row["keywords"], json_fixture(r#"["x"]"#));
    assert!(row.get("matchIn").is_none());
    octocode_runtime_base::test_support::checks::assert_leads_valid(&empty.data);
}

/// Every topic GitHub returns is kept, one spelling the repository's own
/// name (vitejs/vite's `vite`) or its language included; topics a query
/// asked for come first, the rest in GitHub's order.
#[test]
fn every_topic_github_returns_is_kept() {
    let item: octocode_github::RepositorySearchItem = serde_json::from_value(json_fixture(
        r#"{
            "full_name": "vitejs/vite",
            "name": "vite",
            "html_url": "h",
            "default_branch": "main",
            "language": "TypeScript",
            "topics": ["vite", "hmr", "frontend", "build-tool", "typescript"]
        }"#,
    ))
    .expect("item");
    let mut missing_owner = item.clone();
    missing_owner.full_name = missing_owner.name.clone();
    assert_eq!(
        repository_row(missing_owner, &[], &[])
            .expect_err("missing owner")
            .kind,
        octocode_github::ProviderErrorKind::Decode
    );
    let row = repository_row(item, &["hmr".to_owned()], &[]).expect("repository");
    assert_eq!(
        row["topics"],
        json_fixture(r#"["hmr", "vite", "frontend", "build-tool", "typescript"]"#),
        "{row}"
    );
}

/// A repository only the token's account can see is marked with its
/// visibility (GitHub's `visibility`, else its `private` flag); a public
/// one carries no mark.
#[test]
fn a_repository_that_is_not_public_names_its_visibility() {
    let row = |extra: serde_json::Value| {
        let mut item = json_fixture(
            r#"{
                "full_name": "me/r",
                "name": "r",
                "html_url": "h",
                "default_branch": "main"
            }"#,
        );
        item.as_object_mut()
            .expect("object")
            .extend(extra.as_object().expect("object").clone());
        repository_row(serde_json::from_value(item).expect("item"), &[], &[]).expect("repository")
    };
    assert_eq!(
        row(json_fixture(
            r#"{"visibility": "private", "private": true}"#
        ))["visibility"],
        "private"
    );
    assert_eq!(
        row(json_fixture(r#"{"visibility": "internal"}"#))["visibility"],
        "internal"
    );
    assert_eq!(
        row(json_fixture(r#"{"private": true}"#))["visibility"],
        "private"
    );
    for public in [
        json_fixture(r#"{"visibility": "public", "private": false}"#),
        json!({}),
    ] {
        assert!(row(public.clone()).get("visibility").is_none(), "{public}");
    }
    let mut picked = json!({"repositories":[row(json_fixture(r#"{"visibility": "private"}"#))]});
    octocode_tools_shared::gh_shared::pick_rows(
        &mut picked,
        "repositories",
        REPO_ID,
        &["stars".into()],
    );
    assert_eq!(
        picked["repositories"][0]["visibility"], "private",
        "{picked}"
    );
}

/// A listed row answers with what chooses the repository (its name, stars,
/// description and marks); one repository's own row adds its branch,
/// language, license and last push. A `fields` pick names any other fact
/// back, on either.
#[test]
fn a_default_row_leaves_secondary_facts_to_a_fields_pick() {
    let item = || -> octocode_github::RepositorySearchItem {
        serde_json::from_value(json_fixture(
            r#"{
                "full_name": "psf/requests",
                "name": "requests",
                "html_url": "h",
                "default_branch": "main",
                "stargazers_count": 5,
                "forks_count": 2,
                "language": "Python",
                "archived": true,
                "license": {"spdx_id": "Apache-2.0"},
                "description": "HTTP for Humans",
                "topics": ["http"],
                "pushed_at": "2024-12-17T00:00:00Z",
                "created_at": "2012-02-25T00:00:00Z",
                "updated_at": "2026-10-01T00:00:00Z",
                "homepage": "https://requests.readthedocs.io",
                "open_issues_count": 4,
                "size": 9000
            }"#,
        ))
        .expect("item")
    };
    let shown = |detail: &[&str], fields: &[&str]| {
        let fields = fields
            .iter()
            .map(|field| (*field).to_owned())
            .collect::<Vec<_>>();
        let mut value =
            json!({"repositories":[repository_row(item(), &[], &fields).expect("repository")]});
        octocode_tools_shared::gh_shared::lean_rows(&mut value, "repositories", detail, &fields);
        octocode_tools_shared::gh_shared::pick_rows(&mut value, "repositories", REPO_ID, &fields);
        value["repositories"][0].clone()
    };
    assert_eq!(
        shown(LISTED_DETAIL, &[]),
        json_fixture(
            r#"{
                "owner": "psf",
                "repo": "requests",
                "stars": 5,
                "description": "HTTP for Humans",
                "archived": true
            }"#
        )
    );
    assert_eq!(
        shown(EXACT_DETAIL, &[]),
        json_fixture(
            r#"{
                "owner": "psf",
                "repo": "requests",
                "defaultBranch": "main",
                "stars": 5,
                "language": "Python",
                "license": "Apache-2.0",
                "pushedAt": "2024-12-17T00:00:00Z",
                "description": "HTTP for Humans",
                "archived": true
            }"#
        )
    );
    // Every fact a default row leaves out is one `fields` pick away.
    for detail in [LISTED_DETAIL, EXACT_DETAIL] {
        let picked = shown(detail, detail);
        for key in detail {
            assert!(picked.get(*key).is_some(), "{key} is unreachable: {picked}");
        }
    }
}

/// Every `fields` value the repository states is kept when picked, on a
/// listed row and on `get`'s row alike: a mark at its default (`archived`
/// and `fork` false, `visibility` public) and an empty topic list are facts,
/// so none is dropped, none is reported as a field no row has, and each
/// is a value the published row schema admits.
#[test]
fn every_picked_field_github_states_is_on_the_row() {
    let pickable = octocode_runtime_base::contracts::query_schema_value(
        ToolId::GhSearchRepo,
        Some("get"),
        "fields",
        "items",
    )
    .and_then(|items| items["enum"].as_array())
    .expect("fields enum")
    .iter()
    .map(|field| field.as_str().expect("field name").to_owned())
    .collect::<Vec<_>>();
    // A public, non-archived, non-fork repository without topics: GitHub
    // states every value, none of them set.
    let item = || -> octocode_github::RepositorySearchItem {
        serde_json::from_value(json!({
            "full_name": "o/plain",
            "name": "plain",
            "html_url": "https://github.com/o/plain",
            "default_branch": "main",
            "description": "d",
            "stargazers_count": 0,
            "forks_count": 0,
            "open_issues_count": 0,
            "size": 0,
            "visibility": "public",
            "private": false,
            "topics": [],
            "created_at": "2020-01-01T00:00:00Z",
            "updated_at": "2020-01-01T00:00:00Z",
            "pushed_at": "2020-01-01T00:00:00Z",
            "language": "Rust",
            "homepage": "https://example.com",
            "license": {"spdx_id": "MIT"},
            "archived": false,
            "fork": false
        }))
        .expect("repository")
    };
    for detail in [LISTED_DETAIL, EXACT_DETAIL] {
        for field in &pickable {
            let fields = std::slice::from_ref(field);
            let mut response = json!({
                "repositories": [repository_row(item(), &[], fields).expect("repository")]
            });
            lean_rows(&mut response, "repositories", detail, fields);
            pick_rows(&mut response, "repositories", REPO_ID, fields);
            assert!(
                response["repositories"][0].get(field.as_str()).is_some(),
                "{field} picked but not stated: {response}"
            );
            assert!(response.get("warnings").is_none(), "{field}: {response}");
            // The stated value is one the published row schema admits.
            octocode_runtime_base::test_support::assert_part_contract(
                ToolId::GhSearchRepo,
                &json!({ "data": response }),
            );
        }
    }
    // A server that sends neither `visibility` nor `private` states no
    // visibility: a pick leaves it absent rather than guessing public.
    let mut unknown = item();
    unknown.visibility = String::new();
    unknown.private = None;
    let row = repository_row(unknown, &[], &["visibility".to_owned()]).expect("repository");
    assert!(row.get("visibility").is_none(), "{row}");
}

/// An owner listing excludes archived repositories unless its filters ask
/// for them: no filters, empty filters and an omitted `archive` all exclude.
#[test]
fn an_owner_listing_excludes_archived_unless_its_filter_includes_them() {
    let archive = |filters: Option<serde_json::Value>| {
        let mut query = json_fixture(r#"{"operation": "list", "owner": "o"}"#);
        if let Some(filters) = filters {
            query["filters"] = filters;
        }
        let query: GhSearchRepoQuery = serde_json::from_value(query).expect("query");
        access::archive(&query)
    };
    assert_eq!(archive(None), GitHubArchive::Exclude);
    assert_eq!(archive(Some(json!({}))), GitHubArchive::Exclude);
    assert_eq!(
        archive(Some(json_fixture(r#"{"archive": "include"}"#))),
        GitHubArchive::Include
    );
    assert_eq!(
        archive(Some(json_fixture(r#"{"archive": "only"}"#))),
        GitHubArchive::Only
    );
}

/// A keyword spelled as a qualifier (`language:rust`, `stars:>1000`) matches
/// text, so a search would return repositories that merely mention it: the
/// call is refused, naming the field that carries each one. `user:` and `org:`
/// name `owner`, which search takes.
#[test]
fn a_qualifier_keyword_is_refused_naming_its_filter() {
    let refused =
        super::leads::reject_qualifier_keywords(&["language:rust", "stars:>1000", "parser"])
            .expect_err("refused");
    assert_eq!(refused.kind, octocode_github::ProviderErrorKind::Validation);
    let message = refused.to_string();
    assert!(message.contains("filters.language"), "{message}");
    assert!(message.contains("filters.stars"), "{message}");
    assert!(message.contains(r#"{"min":1001}"#), "{message}");
    assert!(!message.contains("`parser`"), "{message}");
    let owner = super::leads::reject_qualifier_keywords(&["org:pallets"]).expect_err("refused");
    let message = owner.to_string();
    assert!(
        message.contains("`owner`") && !message.contains("list"),
        "{message}"
    );
    super::leads::reject_qualifier_keywords(&["parser", "std::fs"]).expect("plain keywords run");
}
