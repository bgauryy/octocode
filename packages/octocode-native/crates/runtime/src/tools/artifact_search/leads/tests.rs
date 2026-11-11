//! Release-source labels, source leads and repair leads.
use super::{advisory_lead, label_release_source, source_leads, source_warning};
use crate::providers::artifact::{ArtifactItem, TagAbsence};
use octocode_contract_types::ArtifactType;
use serde_json::json;

fn item(artifact_type: ArtifactType, repository: &str) -> ArtifactItem {
    let mut item = ArtifactItem::new(artifact_type, "x".into(), "https://registry/x".into());
    item.repository = Some(repository.into());
    item
}

/// The leads of an exact row, labeled the way `respond` labels it.
fn leads_of(row: &ArtifactItem) -> serde_json::Map<String, serde_json::Value> {
    let mut row = row.clone();
    label_release_source(&mut row, true);
    source_leads(&row)
}

#[test]
fn a_row_without_a_release_ref_leads_to_the_default_branch_only() {
    let leads = leads_of(&item(ArtifactType::Pypi, "https://github.com/psf/requests"));
    assert_eq!(leads.keys().collect::<Vec<_>>(), vec!["viewRepo"]);
    assert_eq!(
        leads["viewRepo"]["input"]["queries"][0],
        json!({"operation":"tree","owner":"psf","repo":"requests"})
    );
    // The row labels the default branch; the lead does not repeat it.
    assert!(leads["viewRepo"].get("verification").is_none());
    assert!(leads_of(&item(ArtifactType::Npm, "https://gitlab.com/o/r")).is_empty());
}

#[test]
fn the_release_lead_leads_to_the_package_directory() {
    let mut row = item(ArtifactType::Pypi, "https://github.com/o/r");
    row.source_ref = Some("v1.2.3".into());
    let leads = leads_of(&row);
    assert_eq!(leads.keys().collect::<Vec<_>>(), vec!["viewReleaseSource"]);
    assert_eq!(
        leads["viewReleaseSource"]["input"]["queries"][0],
        json!({"operation":"tree","owner":"o","repo":"r","ref":"v1.2.3"})
    );
    // The package directory is the registry's, never the built entry's.
    row.repository_directory = Some("packages/x".into());
    row.source_attested = true;
    let lead = &leads_of(&row)["viewReleaseSource"];
    assert_eq!(lead["input"]["queries"][0]["path"], "packages/x");
}

/// The row is the one source of the release ref's label: no lead at
/// that ref repeats it.
#[test]
fn leads_at_the_release_ref_do_not_repeat_the_rows_label() {
    let mut row = item(ArtifactType::Npm, "https://github.com/o/r");
    row.source_ref = Some("abc123".into());
    for lead in leads_of(&row).values() {
        assert!(lead.get("verification").is_none(), "{lead}");
    }
}

/// Rubygems names its release as a `/tree/<tag>` URL: the lead
/// reads that tag, not the default branch; a branch URL stays unpinned.
#[test]
fn a_repository_url_naming_the_versions_tag_pins_the_release_lead() {
    let mut row = item(
        ArtifactType::Rubygems,
        "https://github.com/rails/rails/tree/v8.1.4",
    );
    row.version = Some("8.1.4".into());
    let leads = leads_of(&row);
    let tree = &leads["viewReleaseSource"]["input"]["queries"][0];
    assert_eq!(
        tree,
        &json!({"operation":"tree","owner":"rails","repo":"rails","ref":"v8.1.4"})
    );
    row.repository = Some("https://github.com/rails/rails/tree/main".into());
    assert_eq!(leads_of(&row).keys().collect::<Vec<_>>(), vec!["viewRepo"]);
    row.repository = Some("https://github.com/rails/rails/tree/v8.1.3".into());
    assert_eq!(leads_of(&row).keys().collect::<Vec<_>>(), vec!["viewRepo"]);
}

/// A versioned row whose source resolves to the default branch (a
/// monorepo package without a release ref) warns that the branch may
/// hold another version; a row at a release ref, or a discovery row,
/// does not.
#[test]
fn a_default_branch_source_warns_that_it_may_be_another_version() {
    let mut row = item(
        ArtifactType::Npm,
        "https://github.com/DefinitelyTyped/DefinitelyTyped",
    );
    row.package_name = "@types/node".into();
    row.version = Some("20.19.43".into());
    row.repository_directory = Some("types/node".into());
    label_release_source(&mut row, true);
    assert_eq!(row.verification, Some("defaultBranch"));
    assert_eq!(
        source_warning(&row).as_deref(),
        Some(
            "@types/node 20.19.43: the registry names no release ref; its source on the default branch may be another version."
        )
    );
    let mut pinned = row.clone();
    pinned.source_ref = Some("abc".into());
    label_release_source(&mut pinned, true);
    assert_eq!(source_warning(&pinned), None);
    let mut discovery = row.clone();
    label_release_source(&mut discovery, false);
    assert_eq!(source_warning(&discovery), None);
}

/// A declared tag pinned because no tag names the version is the registry's
/// claim (`registryRef`), and the warning names it.
#[test]
fn a_declared_tag_pin_is_a_registry_ref_and_names_the_tag() {
    let mut row = item(
        ArtifactType::Maven,
        "https://github.com/apache/logging-log4j2",
    );
    row.package_name = "org.apache.logging.log4j:log4j-core".into();
    row.version = Some("2.14.1".into());
    row.source_ref = Some("log4j-2.14.1-rc1".into());
    row.declared_ref = true;
    label_release_source(&mut row, true);
    assert_eq!(row.verification, Some("registryRef"));
    assert_eq!(
        source_warning(&row).as_deref(),
        Some(
            "org.apache.logging.log4j:log4j-core 2.14.1: no tag names this version; ref log4j-2.14.1-rc1 is the tag its metadata declares, unchecked against the release."
        )
    );
}

/// A tag list that could not be read scopes the "no tag" claim to the
/// shapes checked and names why, on a declared pin and on a default-branch
/// row alike.
#[test]
fn an_unread_tag_list_scopes_the_no_tag_claim_and_names_its_cause() {
    let mut row = item(
        ArtifactType::Maven,
        "https://github.com/apache/logging-log4j2",
    );
    row.package_name = "org.apache.logging.log4j:log4j-core".into();
    row.version = Some("2.14.1".into());
    row.source_ref = Some("log4j-2.14.1-rc1".into());
    row.declared_ref = true;
    row.tag_absence = Some(TagAbsence::ListUnread(
        "GitHub's rate limit stopped it".into(),
    ));
    label_release_source(&mut row, true);
    assert_eq!(
        source_warning(&row).as_deref(),
        Some(
            "org.apache.logging.log4j:log4j-core 2.14.1: no tag of the shapes checked names this version; its tag list could not be read (GitHub's rate limit stopped it), so a tag may still name it; ref log4j-2.14.1-rc1 is the tag its metadata declares, unchecked against the release."
        )
    );
    let mut unpinned = item(ArtifactType::Npm, "https://github.com/o/r");
    unpinned.version = Some("1.0.0".into());
    unpinned.tag_absence = Some(TagAbsence::ListUnread(
        "GitHub's rate limit stopped it".into(),
    ));
    label_release_source(&mut unpinned, true);
    assert_eq!(
        source_warning(&unpinned).as_deref(),
        Some(
            "x 1.0.0: the registry names no release ref, and no tag of the shapes checked names this version; its tag list could not be read (GitHub's rate limit stopped it), so a tag may still name it; its source on the default branch may be another version."
        )
    );
    // No shape could be checked: none is claimed checked.
    unpinned.tag_absence = Some(TagAbsence::Unchecked(
        "GitHub's rate limit stopped it".into(),
    ));
    assert_eq!(
        source_warning(&unpinned).as_deref(),
        Some(
            "x 1.0.0: the registry names no release ref, and no tag could be checked on GitHub (GitHub's rate limit stopped it), so a tag may name this version; its source on the default branch may be another version."
        )
    );
    // Only another package's tags name it.
    unpinned.tag_absence = Some(TagAbsence::OtherPackagesOnly(vec![
        "a@1.0.0".into(),
        "b-v1.0.0".into(),
    ]));
    assert_eq!(
        source_warning(&unpinned).as_deref(),
        Some(
            "x 1.0.0: the registry names no release ref, and only tags of other packages (a@1.0.0, b-v1.0.0) name this version; its source on the default branch may be another version."
        )
    );
}

/// A moved repository's warning names where GitHub redirects it, when known.
#[test]
fn a_moved_repository_warning_names_the_redirect_target() {
    use crate::providers::artifact::RepositoryStatus;
    let mut row = item(ArtifactType::Npm, "https://github.com/a/b");
    row.version = Some("1.0.0".into());
    row.source_ref = Some("abc".into());
    row.repository_status = Some(RepositoryStatus::Moved);
    label_release_source(&mut row, true);
    let unnamed = source_warning(&row).expect("warning");
    assert!(
        unnamed.contains("redirects a/b to another name"),
        "{unnamed}"
    );
    row.successor_repository = Some("c/d".into());
    let named = source_warning(&row).expect("warning");
    assert!(named.contains("redirects a/b to c/d (renamed"), "{named}");
}

/// A moved repository's source lead names it as GitHub does now (the name
/// its redirect leads to), so the lead runs unchanged; while that name is
/// unknown the lead keeps the registry's, which GitHub redirects.
#[test]
fn a_moved_repository_leads_under_the_name_github_redirects_to() {
    use crate::providers::artifact::RepositoryStatus;
    let mut row = item(ArtifactType::Npm, "https://github.com/a/b");
    row.version = Some("1.0.0".into());
    row.source_ref = Some("abc".into());
    row.repository_directory = Some("packages/x".into());
    row.repository_status = Some(RepositoryStatus::Moved);
    let lead =
        |row: &ArtifactItem| leads_of(row)["viewReleaseSource"]["input"]["queries"][0].clone();
    assert_eq!(
        lead(&row),
        json!({"operation":"tree","owner":"a","repo":"b","path":"packages/x","ref":"abc"})
    );
    row.successor_repository = Some("c/d".into());
    assert_eq!(
        lead(&row),
        json!({"operation":"tree","owner":"c","repo":"d","path":"packages/x","ref":"abc"})
    );
}

/// The row is the one source of the label; a discovery row
/// without a ref gets none, and a rubygems tag URL is a registry ref.
#[test]
fn the_row_states_its_release_source_and_label() {
    let mut row = item(ArtifactType::Npm, "https://github.com/o/r");
    row.source_ref = Some("abc".into());
    label_release_source(&mut row, true);
    let encoded = serde_json::to_value(&row).expect("row");
    assert_eq!(encoded["ref"], "abc");
    assert!(encoded.get("sourceRef").is_none(), "{encoded}");
    assert_eq!(encoded["verification"], "registryRef");
    let mut discovery = item(ArtifactType::Npm, "https://github.com/o/r");
    label_release_source(&mut discovery, false);
    assert!(
        serde_json::to_value(&discovery)
            .expect("row")
            .get("verification")
            .is_none()
    );
    let mut gem = item(
        ArtifactType::Rubygems,
        "https://github.com/rails/rails/tree/v8.1.4",
    );
    gem.version = Some("8.1.4".into());
    label_release_source(&mut gem, true);
    assert_eq!(gem.source_ref.as_deref(), Some("v8.1.4"));
    assert_eq!(gem.verification, Some("registryRef"));
}

/// `dependencies` is always the declared list: an empty list
/// says the version declares none (evidence); unknown leaves it out.
#[test]
fn dependencies_are_one_list_shape() {
    let mut row = item(ArtifactType::Npm, "https://github.com/o/r");
    row.dependencies = Some(vec!["a@1".into(), "b@2".into()]);
    let encoded = serde_json::to_value(&row).expect("row");
    assert_eq!(encoded["dependencies"], json!(["a@1", "b@2"]));
    assert!(encoded.get("dependencyList").is_none(), "{encoded}");
    let mut none = item(ArtifactType::Npm, "https://github.com/o/r");
    none.dependencies = Some(Vec::new());
    assert_eq!(
        serde_json::to_value(&none).expect("row")["dependencies"],
        json!([])
    );
    none.dependencies = None;
    assert!(
        serde_json::to_value(&none)
            .expect("row")
            .get("dependencies")
            .is_none()
    );
}

/// `peerDependencies` is one list shape: the declared `name@range`
/// list, absent when there is none to show.
#[test]
fn peer_dependencies_are_one_list_shape() {
    let mut row = item(ArtifactType::Npm, "https://github.com/o/r");
    row.peer_dependencies = Some(vec!["react@^19".into()]);
    assert_eq!(
        serde_json::to_value(&row).expect("row")["peerDependencies"],
        json!(["react@^19"])
    );
    row.peer_dependencies = None;
    assert!(
        serde_json::to_value(&row)
            .expect("row")
            .get("peerDependencies")
            .is_none()
    );
}

/// An exact row leads to the advisories OSV lists for the version it resolved,
/// on every registry; a row with no version leads to none.
#[test]
fn an_exact_row_leads_to_its_advisories() {
    let mut row = item(ArtifactType::Pypi, "https://github.com/o/r");
    assert!(advisory_lead(&row).is_none());
    row.version = Some("1.2.3".into());
    let lead = advisory_lead(&row).expect("lead");
    assert_eq!(lead["tool"], "securityCheck", "{lead}");
    assert_eq!(
        lead["input"]["queries"][0],
        json!({"operation":"package","ecosystem":"PyPI","packageName":"x","version":"1.2.3"})
    );
}

/// The lead names OSV's case-sensitive ecosystem, never the registry's
/// spelling: every registry's lead is a valid `securityCheck` query
/// (building it asserts the contract in debug builds).
#[test]
fn every_registry_leads_with_an_osv_ecosystem() {
    for kind in [
        ArtifactType::Npm,
        ArtifactType::Pypi,
        ArtifactType::Crates,
        ArtifactType::Maven,
        ArtifactType::Nuget,
        ArtifactType::Go,
        ArtifactType::Packagist,
        ArtifactType::Rubygems,
    ] {
        let mut row = item(kind, "https://github.com/o/r");
        row.version = Some("1.2.3".into());
        let lead = advisory_lead(&row).expect("lead");
        let ecosystem = lead["input"]["queries"][0]["ecosystem"]
            .as_str()
            .expect("ecosystem");
        assert_eq!(
            ecosystem,
            crate::providers::artifact::query::artifact_type::osv_ecosystem(kind).to_string()
        );
    }
}
