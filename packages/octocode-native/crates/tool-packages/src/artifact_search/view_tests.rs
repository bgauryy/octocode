//! The two views of a registry row: the summary states what the next call
//! needs, `details` states the row whole, and the summary's lead asks for it.
use super::summary::{DETAIL_FIELDS, RESOLVE_FIELDS};
use super::*;
use crate::artifact::ArtifactProviderPage;
use octocode_test_support::json_fixture;

/// A row with every field a registry can state.
fn full_row() -> ArtifactItem {
    let mut item = ArtifactItem::new(
        ArtifactType::Npm,
        "vite".into(),
        "https://www.npmjs.com/package/vite".into(),
    );
    item.version = Some("5.4.1".into());
    item.description = Some("Native-ESM powered web dev build tool".into());
    item.license = Some("MIT".into());
    item.homepage = Some("https://vitejs.dev".into());
    item.repository = Some("https://github.com/vitejs/vite".into());
    item.repository_directory = Some("packages/vite".into());
    item.published_at = Some("2024-08-15".into());
    item.dependencies = Some(vec!["rollup@^4.13.0".into()]);
    item.optional_dependencies = Some(vec!["fsevents@~2.3.3".into()]);
    item.peer_dependencies = Some(vec!["less@*".into()]);
    item.requires_python = Some(">=3.10".into());
    item.rust_version = Some("1.56".into());
    item.downloads_monthly = Some(7);
    item.deprecated = Some("use 6".into());
    item.advisories = Some(vec![crate::artifact::Advisory {
        id: Some("GHSA-9wx4-h78v-vm56".into()),
        fixed_in: vec!["5.4.2".into()],
        ..Default::default()
    }]);
    item.source_ref = Some("b44c20cb60b56e626aa6dbec03889812de2549b8".into());
    item.source_attested = true;
    item
}

fn page() -> ArtifactProviderPage {
    ArtifactProviderPage {
        artifacts: vec![full_row()],
        next_state: None,
        total: None,
        terminal_limit: None,
        registry: None,
        any_keyword_total: None,
    }
}

fn rows(query: Value) -> Value {
    let query = typed(&query);
    let mut page = page();
    // A discovery row names no release ref: its resolve does.
    if artifact_search_query::package_name(&query).is_none() {
        page.artifacts[0].source_ref = None;
        page.artifacts[0].source_attested = false;
    }
    respond(&query, page).expect("rows")
}

fn fields(row: &Value) -> Vec<&str> {
    row.as_object()
        .map(|fields| fields.keys().map(String::as_str).collect())
        .unwrap_or_default()
}

/// The `related` call that asks the same query with `view:"details"`.
fn details_lead(data: &Value) -> Option<&Value> {
    data["related"]
        .as_array()?
        .iter()
        .find(|call| call["input"]["queries"][0]["view"] == "details")
}

/// A resolve names where the release's source lives; the name and version
/// the request pinned are its own, so neither view restates them. What the
/// package is, declares and depends on is `details`. A name the registry
/// spells otherwise is stated.
#[test]
fn a_resolve_summary_is_the_release_and_its_source() {
    let resolve = |view: &str| {
        json!({"operation": "resolve", "ecosystem": "npm", "packageName": "vite",
            "version": "5.4.1", "view": view})
    };
    let summary = rows(resolve("summary"));
    assert_eq!(
        summary["artifacts"][0],
        json_fixture(
            r#"{
                "owner": "vitejs",
                "repo": "vite",
                "repositoryDirectory": "packages/vite",
                "publishedAt": "2024-08-15",
                "deprecated": "use 6",
                "ref": "b44c20cb60b56e626aa6dbec03889812de2549b8",
                "verification": "provenance"
            }"#
        ),
        "{summary}"
    );
    let details = rows(resolve("details"));
    let (lean, whole) = (&summary["artifacts"][0], &details["artifacts"][0]);
    // `details` holds every summary field unchanged, and each field it adds
    // is one the summary names as a detail.
    for field in fields(lean) {
        assert_eq!(lean[field], whole[field], "{field}");
    }
    for field in fields(whole) {
        assert!(
            lean.get(field).is_some()
                || DETAIL_FIELDS.contains(&field)
                || ["registryUrl", "description"].contains(&field),
            "{field} is in neither view's account"
        );
    }
    for detail in DETAIL_FIELDS.iter().chain(&["registryUrl", "description"]) {
        if *detail == "engines" {
            continue;
        }
        assert!(whole.get(*detail).is_some(), "{detail}: {details}");
    }
    // No lead restates the query as `view:"details"`.
    assert!(details_lead(&summary).is_none(), "{summary}");
    assert_eq!(
        summary["related"].as_array().map(Vec::len),
        details["related"].as_array().map(Vec::len),
        "{summary}"
    );
    assert!(details_lead(&details).is_none(), "{details}");
    // The default is the summary.
    let mut unstated = resolve("summary");
    unstated.as_object_mut().expect("query").remove("view");
    let unstated: ArtifactSearchQuery = serde_json::from_value(unstated).expect("query");
    assert_eq!(respond(&unstated, page()).expect("rows"), summary);
    for data in [&summary, &details] {
        octocode_runtime_base::test_support::validate_sealed(
            octocode_runtime_base::id::ToolId::ArtifactSearch.as_str(),
            &json!({"results": [{"index": 0, "data": data}]}),
        )
        .expect("contract-valid row");
    }
}

/// The registry page follows from the ecosystem and the name, never the API
/// base the row was read from; a request that named a registry gets it stated.
#[test]
fn a_named_registry_is_stated_in_the_summary() {
    let summary = rows(json_fixture(
        r#"{
            "operation": "resolve",
            "ecosystem": "npm",
            "packageName": "vite",
            "registryUrl": "https://npm.example.test"
        }"#,
    ));
    assert_eq!(
        summary["artifacts"][0]["registryUrl"], "https://www.npmjs.com/package/vite",
        "{summary}"
    );
}

/// A search row is a name to resolve: the name, the version, what it is for.
#[test]
fn a_search_summary_is_the_name_the_version_and_the_description() {
    let search = |view: &str| json!({"operation": "search", "ecosystem": "npm", "keywords": ["vite"], "view": view});
    let summary = rows(search("summary"));
    assert_eq!(
        summary["artifacts"][0],
        json_fixture(
            r#"{
                "packageName": "vite",
                "version": "5.4.1",
                "description": "Native-ESM powered web dev build tool",
                "deprecated": "use 6"
            }"#
        ),
        "{summary}"
    );
    let details = rows(search("details"));
    let whole = &details["artifacts"][0];
    for field in [
        "owner",
        "repo",
        "license",
        "downloadsMonthly",
        "registryUrl",
    ] {
        assert!(whole.get(field).is_some(), "{field}: {details}");
    }
    for field in fields(whole) {
        assert!(
            summary["artifacts"][0].get(field).is_some()
                || DETAIL_FIELDS.contains(&field)
                || RESOLVE_FIELDS.contains(&field)
                || field == "registryUrl",
            "{field} is in neither view's account"
        );
    }
    // The details view is named in the tool description, not on the row.
    assert!(details_lead(&summary).is_none(), "{summary}");
    let mut later = search("summary");
    later["page"] = json!(2);
    assert!(details_lead(&rows(later)).is_none());
}

/// The next page of a discovery keeps the view the walk began with.
#[test]
fn the_next_page_keeps_the_view() {
    for (view, carried) in [("summary", Value::Null), ("details", json!("details"))] {
        let query = typed(
            &json!({"operation": "search", "ecosystem": "rubygems", "keywords": ["jwt"], "view": view}),
        );
        let mut page = page();
        page.next_state = Some(crate::artifact::ArtifactProviderState {
            offset: Some(1),
            ..Default::default()
        });
        let data = respond(&query, page).expect("rows");
        let next = &data["next"]["nextPage"]["input"];
        // The call is the query itself or a cursor that saves it: when the
        // query is stated, a default view is left out and another is kept.
        if let Some(row) = next["queries"].get(0) {
            assert_eq!(row["view"], carried, "{view}: {data}");
        } else {
            assert!(next["cursor"].is_string(), "{data}");
        }
    }
}

/// A resolve summary keeps a name the registry spells otherwise than the
/// request did (a normalized spelling): that name is the answer.
#[test]
fn a_resolve_summary_keeps_a_name_the_registry_spells_otherwise() {
    let summary = rows(json!({"operation": "resolve", "ecosystem": "pypi",
        "packageName": "Vite", "version": "5.4.1", "view": "summary"}));
    assert_eq!(summary["artifacts"][0]["packageName"], "vite", "{summary}");
}
