//! Response anchors: exact rows, empty broadening and repair leads.
use super::*;
use crate::providers::artifact::ArtifactProviderPage;

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
    let query = typed(&json!({"ecosystem":"npm","packageName":"zod"}));
    let data = respond(&query, ArtifactProviderPage::single(exact_row())).expect("row");
    let row = data["artifacts"][0].as_object().expect("row");
    for anchor in [
        "name",
        "version",
        "owner",
        "repo",
        "sourceRef",
        "verification",
    ] {
        assert!(row.contains_key(anchor), "{anchor}: {data}");
    }
    assert_eq!(row["owner"], "colinhacks");
    assert_eq!(row["repo"], "zod");
    assert!(!row.contains_key("repository"), "{data}");
    assert_eq!(row["verification"], "provenance");
    assert!(
        !row.contains_key("ecosystem") && !row.contains_key("type"),
        "{data}"
    );
    let tree = &data["next"]["viewReleaseSource"]["query"]["queries"][0];
    assert_eq!(tree["ref"], row["sourceRef"], "{data}");
}

/// A discovery row whose registry names no repository but whose
/// homepage is a GitHub repository carries it, as `owner`/`repo`; the
/// homepage that restated it is gone.
#[test]
fn a_github_homepage_is_a_discovery_rows_repository() {
    let query = typed(&json!({"ecosystem":"rubygems","keywords":["jwt"]}));
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
    let query = typed(&json!({"ecosystem":"crates","keywords":["zzz","async","runtime"]}));
    let data = respond(&query, ArtifactProviderPage::empty(Some(0))).expect("empty");
    assert_eq!(data["status"], "empty");
    assert_eq!(
        data["hints"],
        json!([
            "broadenSearch rows: queries[0] drops keyword \"runtime\"; queries[1] drops keyword \"async\"; queries[2] drops keyword \"zzz\""
        ]),
        "{data}"
    );
    let variants = data["next"]["broadenSearch"]["query"]["queries"]
        .as_array()
        .expect("batch");
    assert_eq!(variants.len(), 3, "{data}");
    assert_eq!(variants[0]["keywords"], json!(["zzz", "async"]), "{data}");
    assert_eq!(variants[0]["ecosystem"], "crates", "{data}");
    // One multi-word keyword drops a word at a time, and says which.
    let phrase = typed(&json!({"ecosystem":"crates","keywords":["zzz async"]}));
    let data = respond(&phrase, ArtifactProviderPage::empty(Some(0))).expect("empty");
    assert_eq!(
        data["next"]["broadenSearch"]["query"]["queries"],
        json!([{"ecosystem":"crates","keywords":["zzz"]},{"ecosystem":"crates","keywords":["async"]}]),
        "{data}"
    );
    assert_eq!(
        data["hints"],
        json!([
            "broadenSearch rows: queries[0] drops word \"async\"; queries[1] drops word \"zzz\""
        ]),
        "{data}"
    );
    let single = typed(&json!({"ecosystem":"crates","keywords":["zzz"]}));
    let data = respond(&single, ArtifactProviderPage::empty(Some(0))).expect("empty");
    assert!(data.get("next").is_none(), "{data}");
}

/// A failure a call repairs carries exactly that one lead and no text.
#[test]
fn failures_carry_one_repair_lead() {
    let unknown = typed(&json!({"ecosystem":"crates","packageName":"no-such-crate@1.0.0"}));
    let error = respond(&unknown, ArtifactProviderPage::empty(Some(0))).expect_err("missing");
    let error = leads::with_repair(error, &unknown);
    let next = error.next.expect("lead");
    assert_eq!(next.as_object().map(|next| next.len()), Some(1), "{next}");
    assert_eq!(
        next["broadenSearch"]["query"]["queries"][0],
        json!({"ecosystem":"crates","keywords":["no-such-crate"]})
    );
    assert!(error.hints.is_empty());

    let pinned = typed(&json!({"ecosystem":"npm","packageName":"zod","version":"3.22.9"}));
    let mut missing = ArtifactError::new(ErrorCode::VersionNotFound, "no 3.22.9");
    missing.nearest = vec!["3.22.4".into(), "3.22.0".into()];
    let next = leads::with_repair(missing, &pinned).next.expect("lead");
    let versions = next["repair"]["query"]["queries"]
        .as_array()
        .expect("batch")
        .iter()
        .map(|row| row["version"].clone())
        .collect::<Vec<_>>();
    assert_eq!(versions, [json!("3.22.4"), json!("3.22.0")], "{next}");

    let latest = leads::with_repair(
        ArtifactError::new(ErrorCode::CapabilityUnavailable, "x"),
        &pinned,
    )
    .next
    .expect("lead");
    assert_eq!(
        latest["repair"]["query"]["queries"][0],
        json!({"ecosystem":"npm","packageName":"zod"})
    );
    // A bare Maven artifact name is no coordinate: its repair searches it.
    let bare = typed(&json!({"ecosystem":"maven","packageName":"guava"}));
    let error = ArtifactError {
        searchable_name: true,
        ..ArtifactError::new(ErrorCode::InvalidInput, "not a groupId:artifactId")
    };
    let next = leads::with_repair(error, &bare).next.expect("lead");
    assert_eq!(
        next["broadenSearch"]["query"]["queries"][0],
        json!({"ecosystem":"maven","keywords":["guava"]}),
        "{next}"
    );
    assert!(
        leads::with_repair(ArtifactError::new(ErrorCode::InvalidInput, "x"), &bare)
            .next
            .is_none()
    );
    // PyPI has no keyword search: an unknown package has no repair, and
    // its hint names the spelling, not the provider configuration.
    let pypi = typed(&json!({"ecosystem":"pypi","packageName":"nope"}));
    let error = leads::with_repair(ArtifactError::new(ErrorCode::NotFound, "x"), &pypi);
    assert!(error.next.is_none());
    assert_eq!(
        error.hints,
        ["Check the exact packageName spelling; pypi has no keyword search."]
    );
}
