//! An empty search's targeted leads (the content search of a path search,
//! the OR split) beside the broadened search, each keeping the full scope.
//! Leads omit fields at their contract default, so a content search's row
//! carries no `matchIn` (its default is `["content"]`).
use super::*;

/// The scope every row here searches: `octo/demo`, TypeScript under `src`,
/// files named like `greeter`; `match_in` `None` is the default, content.
fn scoped(keywords: &[&str], match_in: Option<&str>) -> serde_json::Value {
    let mut row = json!({
        "owner":"octo","repo":"demo","extensions":["ts"],"pathPrefix":"src",
        "nameContains":"greeter","keywords":keywords
    });
    if let Some(match_in) = match_in {
        row["matchIn"] = json!([match_in]);
    }
    row
}

/// The broadened search is offered too: the targeted lead is not its
/// replacement.
fn assert_broadened(data: &serde_json::Value) {
    assert!(
        data["related"].as_array().is_some_and(|calls| calls
            .iter()
            .filter(|call| call["tool"] == "ghSearchCode")
            .count()
            >= 2),
        "{data}"
    );
}

/// An empty code search for `row` against the live repository `octo/demo`.
async fn run_empty(row: serde_json::Value) -> crate::response::result::ToolData {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"total_count":0,"incomplete_results":false,"items":[]})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/octo/demo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"full_name":"octo/demo","default_branch":"main","archived":false}),
        ))
        .mount(&server)
        .await;
    let provider = one_shot_provider(&server);
    let query: GhSearchCodeQuery = serde_json::from_value(row).expect("query");
    execute(&provider, &query, &default_context(), &Passthrough)
        .await
        .expect("empty search")
}

/// The related ghSearchCode call whose rows search exactly `variants`.
fn search_of<'a>(
    data: &'a serde_json::Value,
    variants: &[&[&str]],
) -> Option<&'a serde_json::Value> {
    crate::tools::test_support::checks::find_related_call(data, "ghSearchCode", |input| {
        input["queries"].as_array().is_some_and(|rows| {
            rows.len() == variants.len()
                && rows
                    .iter()
                    .zip(variants)
                    .all(|(row, keywords)| row["keywords"] == json!(keywords))
        })
    })
}

/// A path search of code (`greet(`) leads to the same search of file
/// contents, with every scope field kept, beside the broadened search.
#[tokio::test]
async fn an_empty_path_search_of_code_leads_to_the_content_search() {
    let out = run_empty(scoped(&["greet("], Some("path"))).await;
    assert_broadened(&out.data);
    let found = search_of(&out.data, &[&["greet("]]).unwrap_or_else(|| panic!("{}", out.data));
    assert_eq!(
        found["input"]["queries"],
        json!([scoped(&["greet("], None)]),
        "{}",
        out.data
    );
}

/// Keywords holding OR lead to one related call with a row per
/// alternative, each keeping the full scope, beside the broadened search.
#[tokio::test]
async fn an_empty_or_search_leads_to_one_row_per_alternative() {
    let out = run_empty(scoped(&["greet OR Greeter"], Some("content"))).await;
    assert_broadened(&out.data);
    let split =
        search_of(&out.data, &[&["greet"], &["Greeter"]]).unwrap_or_else(|| panic!("{}", out.data));
    assert_eq!(
        split["input"]["queries"],
        json!([scoped(&["greet"], None), scoped(&["Greeter"], None)]),
        "{}",
        out.data
    );
}

/// A ref naming the default branch was searched: no "not searched" hint,
/// and a ref that differs names the branch the index covers.
#[tokio::test]
async fn a_ref_hint_appears_only_off_the_default_branch() {
    let row = |reference: &str| json!({"owner":"octo","repo":"demo","ref":reference,"keywords":["zzqq"],"pathPrefix":"src"});
    let hints = |out: &crate::response::result::ToolData| out.data["hints"].to_string();
    let out = run_empty(row("main")).await;
    assert!(!hints(&out).contains("indexes only"), "{}", out.data);
    let out = run_empty(row("dev")).await;
    assert!(
        hints(&out).contains("indexes only the default branch `main`; `dev` was not searched"),
        "{}",
        out.data
    );
}

/// An owner-wide empty search is unproven absence too, and leads to the
/// owner's repositories (there is no one repository to list or clone).
#[tokio::test]
async fn an_empty_owner_search_states_unproven_absence_and_lists_repositories() {
    let out = run_empty(json!({"owner":"octo","keywords":["zzqq"]})).await;
    assert!(
        out.diagnostics
            .hints
            .iter()
            .any(|hint| hint.contains("unproven absence") && hint.contains("ghSearchRepo")),
        "{:?}",
        out.diagnostics.hints
    );
    let listing =
        crate::tools::test_support::checks::related_call(&out.data, "ghSearchRepo", |_| true);
    assert_eq!(
        listing["input"]["queries"][0],
        json!({"operation":"list","owner":"octo"}),
        "{}",
        out.data
    );
}
