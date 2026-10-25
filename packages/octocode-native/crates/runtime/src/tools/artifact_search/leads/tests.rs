//! Release-source labels, source leads and repair leads.
use super::{github_repo_dir, label_release_source, source_leads, source_warning};
use crate::providers::artifact::ArtifactItem;
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

#[test]
fn parses_monorepo_subdirectories() {
    let dir = |url| github_repo_dir(url);
    assert_eq!(
        dir("https://github.com/BurntSushi/ripgrep/tree/master/crates/ignore").as_deref(),
        Some("crates/ignore")
    );
    assert_eq!(
        dir("https://github.com/o/r/tree/main/packages/x/").as_deref(),
        Some("packages/x")
    );
    assert_eq!(
        dir("https://github.com/o/r/blob/main/crates/a/Cargo.toml").as_deref(),
        Some("crates/a")
    );
    assert_eq!(dir("https://github.com/o/r/tree/main"), None);
    assert_eq!(dir("https://github.com/o/r#readme"), None);
    assert_eq!(dir("git+https://github.com/o/r.git"), None);
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
