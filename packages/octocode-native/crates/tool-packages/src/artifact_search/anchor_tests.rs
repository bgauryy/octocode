//! Response anchors: exact rows, empty broadening and repair leads.
use super::*;
use crate::artifact::ArtifactProviderPage;
use octocode_runtime_base::test_support::checks::related_call;
use octocode_test_support::json_fixture;

fn exact_row() -> ArtifactItem {
    let mut item = ArtifactItem::new(
        ArtifactType::Npm,
        "zod".into(),
        "https://www.npmjs.com/package/zod".into(),
    );
    item.version = Some("4.6.5".into());
    item.repository = Some("https://github.com/colinhacks/zod".into());
    item.source_ref = Some("59bbc03e10c636b9eb3c393dfeb552819774ec21".into());
    item.source_attested = true;
    item
}

/// An exact row carries what the next hop takes: the package's name
/// and version, its repository as `owner`/`repo` for ghStructure, and
/// the release ref with how it was checked. No query field is echoed.
#[test]
fn an_exact_row_carries_the_release_source_anchors() {
    let query = typed(&json_fixture(
        r#"{
            "operation": "resolve",
            "ecosystem": "npm",
            "packageName": "zod"
        }"#,
    ));
    let data = respond(&query, ArtifactProviderPage::single(exact_row())).expect("row");
    let row = data["artifacts"][0].as_object().expect("row");
    for anchor in [
        "packageName",
        "version",
        "owner",
        "repo",
        "ref",
        "verification",
    ] {
        assert!(row.contains_key(anchor), "{anchor}: {data}");
    }
    assert_eq!(row["owner"], "colinhacks");
    assert_eq!(row["repo"], "zod");
    assert!(!row.contains_key("repository"), "{data}");
    assert!(!row.contains_key("sourceRef"), "{data}");
    assert_eq!(row["verification"], "provenance");
    assert!(
        !row.contains_key("ecosystem") && !row.contains_key("type"),
        "{data}"
    );
    // The row's own fields are the tree read's input: it carries no call.
    assert!(data.get("related").is_none(), "{data}");
}

/// A discovery row whose registry names no repository but whose
/// homepage is a GitHub repository carries it, as `owner`/`repo`; the
/// homepage that restated it is gone.
#[test]
fn a_github_homepage_is_a_discovery_rows_repository() {
    let query = typed(&json_fixture(
        r#"{
            "operation": "search",
            "ecosystem": "rubygems",
            "keywords": ["jwt"]
        }"#,
    ));
    let mut gem = ArtifactItem::new(
        ArtifactType::Rubygems,
        "json-jwt".into(),
        "https://rubygems.org/gems/json-jwt".into(),
    );
    gem.homepage = Some("https://github.com/nov/json-jwt".into());
    let data = respond(
        &query,
        ArtifactProviderPage {
            artifacts: vec![gem],
            next_state: None,
            total: None,
            terminal_limit: None,
            registry: None,
            any_keyword_total: None,
        },
    )
    .expect("rows");
    let row = &data["artifacts"][0];
    assert!(row.get("repository").is_none(), "{data}");
    assert_eq!(
        (&row["owner"], &row["repo"]),
        (&json!("nov"), &json!("json-jwt"))
    );
    assert!(row.get("homepage").is_none(), "{data}");
}

/// An empty discovery's one lead re-runs the search without each
/// keyword, as one batch; its only prose names what each row drops.
#[test]
fn an_empty_discovery_broadens_by_dropping_each_keyword() {
    let query = typed(&json_fixture(
        r#"{
            "operation": "search",
            "ecosystem": "crates",
            "keywords": ["zzz", "async", "runtime"]
        }"#,
    ));
    let data = respond(&query, ArtifactProviderPage::empty(Some(0))).expect("empty");
    assert_eq!(data["status"], "empty");
    let variants = related_call(&data, "artifactSearch", |_| true)["input"]["queries"]
        .as_array()
        .expect("batch");
    assert_eq!(variants.len(), 3, "{data}");
    assert_eq!(
        variants[0]["keywords"],
        json_fixture(r#"["zzz", "async"]"#),
        "{data}"
    );
    assert_eq!(variants[0]["ecosystem"], "crates", "{data}");
    // One multi-word keyword drops a word at a time, and says which.
    let phrase = typed(&json_fixture(
        r#"{
            "operation": "search",
            "ecosystem": "crates",
            "keywords": ["zzz async"]
        }"#,
    ));
    let data = respond(&phrase, ArtifactProviderPage::empty(Some(0))).expect("empty");
    let variants = related_call(&data, "artifactSearch", |_| true)["input"]["queries"]
        .as_array()
        .expect("variants");
    assert_eq!(
        variants
            .iter()
            .map(|row| row["keywords"].clone())
            .collect::<Vec<_>>(),
        vec![json_fixture(r#"["zzz"]"#), json_fixture(r#"["async"]"#)]
    );
    for row in variants {
        octocode_runtime_base::contracts::validate_query("artifactSearch", row.clone())
            .expect("canonical related row");
    }
    let single = typed(&json_fixture(
        r#"{
            "operation": "search",
            "ecosystem": "crates",
            "keywords": ["zzz"]
        }"#,
    ));
    let data = respond(&single, ArtifactProviderPage::empty(Some(0))).expect("empty");
    assert!(data.get("related").is_none(), "{data}");
}

/// An empty search with terms to drop names the registry and every term no
/// package holds together in one hint, beside its one broader call; the hint
/// never explains that call. A lone term has nothing to drop: `status`
/// says it all, and no hint restates it.
#[test]
fn an_empty_search_names_its_terms_only_when_it_broadens() {
    for keywords in [vec!["zzz"], vec!["zzz", "async"], vec!["zzz async"]] {
        let query = typed(&json!({
            "operation": "search",
            "ecosystem": "crates",
            "keywords": keywords,
        }));
        let data = respond(&query, ArtifactProviderPage::empty(Some(0))).expect("empty");
        assert_eq!(data["status"], "empty", "{data}");
        let calls = data["related"].as_array().map_or(0, Vec::len);
        assert!(calls <= 1, "{data}");
        let hints = data["hints"].as_array().map_or(&[][..], Vec::as_slice);
        assert_eq!(
            hints.len(),
            calls,
            "a hint only beside a broader call: {data}"
        );
        for hint in hints {
            let hint = hint.as_str().expect("text");
            assert!(hint.contains("crates"), "{hint}");
            for keyword in &keywords {
                assert!(hint.contains(keyword), "{keyword}: {hint}");
            }
            assert!(!hint.contains("related"), "{hint}");
        }
    }
}

/// A walk that ends on an empty later page found its matches on the pages
/// before: it claims no absence and offers no broader search.
#[test]
fn an_empty_last_page_of_a_walk_claims_no_absence() {
    let query = typed(&json!({
        "operation": "search",
        "ecosystem": "crates",
        "keywords": ["zzz", "async"],
        "page": 2,
    }));
    let data = respond(&query, ArtifactProviderPage::empty(Some(0))).expect("empty");
    assert_eq!(data["status"], "empty", "{data}");
    assert!(data.get("related").is_none(), "{data}");
    let hints = data["hints"].as_array().expect("hints");
    assert_eq!(hints.len(), 1, "{data}");
    assert!(
        !hints[0].as_str().expect("text").starts_with("No "),
        "{data}"
    );
}

/// A failure a call repairs carries exactly that one lead and no text. The
/// lead keeps the query's view, here the default, which a call leaves out.
#[test]
fn failures_carry_one_repair_lead() {
    let unknown = typed(&json_fixture(
        r#"{
            "operation": "resolve",
            "ecosystem": "crates",
            "packageName": "no-such-crate",
            "version": "1.0.0",
            "view": "summary"
        }"#,
    ));
    let error = respond(&unknown, ArtifactProviderPage::empty(Some(0))).expect_err("missing");
    let error = leads::with_repair(error, &unknown);
    let next = error.calls.expect("lead");
    assert_eq!(
        next.as_object().map(serde_json::Map::len),
        Some(1),
        "{next}"
    );
    assert_eq!(
        next["broadenSearch"]["input"]["queries"][0],
        json_fixture(
            r#"{
                "operation": "search",
                "ecosystem": "crates",
                "keywords": ["no-such-crate"]
            }"#
        )
    );
    assert!(error.hints.is_empty());

    let pinned = typed(&json_fixture(
        r#"{
            "operation": "resolve",
            "ecosystem": "npm",
            "packageName": "zod",
            "version": "3.22.9",
            "view": "summary"
        }"#,
    ));
    let mut missing = ArtifactError::version_not_found("no 3.22.9");
    missing.nearest = vec!["3.22.4".into(), "3.22.0".into()];
    let repaired = leads::with_repair(missing, &pinned);
    // The repair reads the published versions: no hint says to discover them.
    assert!(repaired.hints.is_empty(), "{:?}", repaired.hints);
    let next = repaired.calls.expect("lead");
    let versions = next["repair"]["input"]["queries"]
        .as_array()
        .expect("batch")
        .iter()
        .map(|row| row["version"].clone())
        .collect::<Vec<_>>();
    assert_eq!(versions, [json!("3.22.4"), json!("3.22.0")], "{next}");

    // No published version to suggest: the hint stays beside the latest read.
    let unranked = leads::with_repair(ArtifactError::version_not_found("no 3.22.9"), &pinned);
    assert_eq!(unranked.hints.len(), 1, "{:?}", unranked.hints);
    assert!(unranked.calls.is_some(), "{unranked:?}");
    let latest = leads::with_repair(
        ArtifactError::new(ErrorCode::CapabilityUnavailable, "x"),
        &pinned,
    )
    .calls
    .expect("lead");
    assert_eq!(
        latest["repair"]["input"]["queries"][0],
        json_fixture(
            r#"{
                "operation": "resolve",
                "ecosystem": "npm",
                "packageName": "zod"
            }"#
        )
    );
    assert!(
        octocode_runtime_base::contracts::validate_query(
            "artifactSearch",
            json_fixture(
                r#"{
                    "operation": "resolve",
                    "ecosystem": "maven",
                    "packageName": "guava"
                }"#
            )
        )
        .is_err()
    );
    // PyPI has no keyword search: an unknown package has no repair, and
    // its hint names the spelling, not the provider configuration.
    let pypi = typed(&json_fixture(
        r#"{
            "operation": "resolve",
            "ecosystem": "pypi",
            "packageName": "nope"
        }"#,
    ));
    let error = leads::with_repair(ArtifactError::new(ErrorCode::NotFound, "x"), &pypi);
    assert!(error.calls.is_none());
    assert_eq!(
        error.hints,
        ["Check the exact packageName spelling; pypi has no keyword search."]
    );

    // npm names are lowercase: a capitalized one is repaired to it.
    let capitals = typed(&json!({
        "operation": "resolve", "ecosystem": "npm", "packageName": "@Angular/core"
    }));
    let error = leads::with_repair(ArtifactError::new(ErrorCode::NotFound, "x"), &capitals);
    let next = error.calls.expect("lead");
    assert_eq!(
        next["repair"]["input"]["queries"][0]["packageName"], "@angular/core",
        "{next}"
    );
}

/// A missing version's repair reads as many nearest versions as one call
/// carries (the contract's batch limit), newest first; when the registry has
/// fewer, the call reads every one. Either way the call is the recovery, and
/// no hint restates it.
#[test]
fn a_missing_version_repair_fills_one_call_or_reads_every_version() {
    let limit = octocode_runtime_base::contracts::batch_limit(
        octocode_runtime_base::id::ToolId::ArtifactSearch,
    )
    .expect("batch limit");
    let pinned = typed(&json_fixture(
        r#"{
            "operation": "resolve",
            "ecosystem": "npm",
            "packageName": "vite",
            "version": "99.0.0"
        }"#,
    ));
    let published = (0..limit + 3)
        .map(|patch| format!("8.3.{patch}"))
        .collect::<Vec<_>>();
    let missing =
        crate::artifact::version_not_found(ArtifactType::Npm, "vite", "99.0.0", &published);
    let error = leads::with_repair(missing, &pinned);
    let versions = |error: &ArtifactError| {
        error.calls.as_ref().expect("lead")["repair"]["input"]["queries"]
            .as_array()
            .expect("batch")
            .iter()
            .map(|row| row["version"].as_str().unwrap_or_default().to_owned())
            .collect::<Vec<_>>()
    };
    let newest = (3..limit + 3)
        .rev()
        .map(|patch| format!("8.3.{patch}"))
        .collect::<Vec<_>>();
    assert_eq!(versions(&error), newest);
    assert!(error.hints.is_empty(), "{:?}", error.hints);

    let few =
        crate::artifact::version_not_found(ArtifactType::Npm, "vite", "99.0.0", &published[..2]);
    let error = leads::with_repair(few, &pinned);
    assert_eq!(versions(&error), ["8.3.1", "8.3.0"]);
    assert!(error.hints.is_empty(), "{:?}", error.hints);
}

/// A package the registry does not know gets one short recovery, naming the
/// field to correct; a related search beside it is its own (the hint never
/// explains it), and a refused registry host gets no text beside the error,
/// which names the one setting that permits it.
#[test]
fn the_recovery_hints_name_a_field_and_stay_short() {
    use octocode_runtime_base::response::output::ToolOutput;
    let hint = super::Output
        .error_hint(ErrorCode::NotFound, &json!({}))
        .expect("not-found hint");
    assert!(hint.contains("packageName"), "{hint}");
    assert!(!hint.contains("related"), "{hint}");
    assert!(hint.len() <= 150, "{} chars: {hint}", hint.len());
    let refused = octocode_runtime_base::response::rows::error_code_hint(
        &super::Output,
        ErrorCode::SecurityValidationFailed,
        &json!({}),
    );
    assert!(refused.is_none_or(str::is_empty), "{refused:?}");
}
