//! Leads from an exact registry row to its source on GitHub.
use crate::id::ToolId;
use crate::providers::artifact::types::artifact_search_query;
use crate::providers::artifact::{
    ArtifactError, ArtifactItem, ArtifactSearchQuery, ArtifactType, github_repo_dir, github_slug,
    page_out_of_range, release_ref_in,
};
use crate::response::result::Confidence;
use crate::response::result::{Continuation, query_row};
use serde_json::{Map, Value, json};

/// Labels a row's release source, the one place the label is decided: a
/// repository URL naming this version's tag (rubygems `…/tree/v8.1.4`)
/// becomes the ref when the registry gave none; a provenance-attested ref
/// and an upstream tag checked to exist say so; every other ref is the
/// registry's unchecked claim (npm `gitHead`, Go/Composer/NuGet refs, crates
/// VCS info). An exact row on GitHub without a ref reads the default branch.
pub(super) fn label_release_source(artifact: &mut ArtifactItem, exact: bool) {
    if artifact.source_ref.is_none() {
        artifact.source_ref = artifact
            .repository
            .as_deref()
            .zip(artifact.version.as_deref())
            .and_then(|(url, version)| release_ref_in(url, version));
    }
    artifact.verification = if artifact.source_ref.is_none() {
        (exact
            && artifact
                .repository
                .as_deref()
                .and_then(github_slug)
                .is_some())
        .then_some("defaultBranch")
    } else if artifact.source_attested {
        Some("provenance")
    } else if artifact.source_tag {
        Some("tag")
    } else {
        Some("registryRef")
    };
}

/// The warning a row's release source needs, one per row: a registry
/// release commit GitHub does not have, and any row whose source reads the
/// default branch (`verification:"defaultBranch"`) rather than a ref of the
/// resolved version: that branch may hold another version, as a monorepo's
/// often does.
pub(super) fn source_warning(artifact: &ArtifactItem) -> Option<String> {
    let default_branch = artifact.verification == Some("defaultBranch");
    let reason = match artifact.missing_ref.as_deref() {
        Some(reference) => {
            let repository = artifact
                .repository
                .as_deref()
                .and_then(github_slug)
                .map_or_else(|| "its repository".to_owned(), |(o, r)| format!("{o}/{r}"));
            format!("release commit {reference} is not in {repository} (unpushed or rewritten)")
        }
        None if default_branch => "the registry names no release ref".to_owned(),
        None => return None,
    };
    let package = match artifact.version.as_deref() {
        Some(version) => format!("{} {version}", artifact.name),
        None => artifact.name.clone(),
    };
    let source = if default_branch {
        "; its source on the default branch may be another version"
    } else {
        ""
    };
    Some(format!("{package}: {reason}{source}."))
}

/// The warning of a row whose dependency versions name properties its
/// release metadata does not define: they are shown as declared, and this
/// says so instead of leaving a `${…}` to read as a version.
pub(super) fn property_warning(artifact: &ArtifactItem) -> Option<String> {
    let names = &artifact.unresolved_properties;
    if names.is_empty() {
        return None;
    }
    let package = match artifact.version.as_deref() {
        Some(version) => format!("{} {version}", artifact.name),
        None => artifact.name.clone(),
    };
    Some(format!(
        "{package}: dependency versions {} are properties its POM does not define (a parent POM sets them); shown as declared.",
        names.join(", ")
    ))
}

/// The leads of an exact row whose repository is on GitHub: the
/// ghStructure tree at the package's directory. A published entry point is
/// not a source path (a built package names output the repository lacks), so
/// only the directory the registry records for the package is led to. With a release ref the tree lead is
/// `viewReleaseSource`; the default-branch tree is that lead without `ref`,
/// so no second tree lead restates it. The row states how its ref was
/// checked (`verification`); its leads do not repeat it.
pub(super) fn source_leads(artifact: &ArtifactItem) -> Map<String, Value> {
    let mut leads = Map::new();
    let Some((owner, repo)) = artifact.repository.as_deref().and_then(github_slug) else {
        return leads;
    };
    let directory = artifact
        .repository_directory
        .clone()
        .or_else(|| artifact.repository.as_deref().and_then(github_repo_dir))
        .filter(|directory| !directory.is_empty());
    // The root at depth one is ghStructure's default listing.
    let at = crate::tools::gh_shared::RepoPath {
        owner: &owner,
        repo: &repo,
        path: directory.as_deref().unwrap_or_default(),
        reference: artifact.source_ref.as_deref(),
    };
    let name = if at.reference.is_some() {
        "viewReleaseSource"
    } else {
        "viewRepo"
    };
    leads.insert(name.into(), at.lead(ToolId::GhStructure).build());
    leads
}

/// The query as a lead copies it: every field the caller set except the
/// brief and the page cursor.
fn copied(query: &ArtifactSearchQuery) -> Map<String, Value> {
    match query_row(query, &["mainGoal", "reasoning", "page", "cursor"]) {
        Value::Object(row) => row,
        _ => Map::new(),
    }
}

/// A failure's one repair lead, when a call fixes it: a discovery page past
/// the paging range reads the last reachable page (`repair`); an unknown exact
/// package, or a name that is no exact coordinate of the registry, searches
/// its name as keywords (`broadenSearch`; PyPI has no search); a missing
/// version reads the nearest published ones, else the latest (`repair`); a
/// version the ecosystem cannot select reads the latest (`repair`).
pub(super) fn with_repair(mut error: ArtifactError, query: &ArtifactSearchQuery) -> ArtifactError {
    // A discovery page past the paging range: the error names the bound and
    // its one repair reads the last page this page size reaches.
    if error.code == "invalidInput"
        && let Some(limit) = page_out_of_range(query)
    {
        let mut last = copied(query);
        last.insert("page".into(), json!(super::last_reachable_page(query)));
        error.message = limit.warning;
        let lead = Continuation::new(ToolId::ArtifactSearch, Value::Object(last))
            .confidence(Confidence::Exact)
            .build();
        error.next = Some(Box::new(json!({ "repair": lead })));
        return error;
    }
    let Some(name) = artifact_search_query::bare_package_name(query) else {
        return error;
    };
    let mut exact = copied(query);
    exact.insert("packageName".into(), json!(name));
    exact.remove("version");
    let searchable = error.code == "notFound" || error.searchable_name;
    let lead = match error.code.as_str() {
        _ if searchable && artifact_search_query::artifact_type(query) != ArtifactType::Pypi => {
            exact.remove("packageName");
            exact.insert("keywords".into(), json!([name]));
            Some((
                "broadenSearch",
                Continuation::new(ToolId::ArtifactSearch, Value::Object(exact))
                    .confidence(Confidence::Low)
                    .build(),
            ))
        }
        "versionNotFound" if !error.nearest.is_empty() => {
            let queries = error
                .nearest
                .iter()
                .map(|version| {
                    let mut row = exact.clone();
                    row.insert("version".into(), json!(version));
                    Value::Object(row)
                })
                .collect::<Vec<_>>();
            Some((
                "repair",
                Continuation::input(ToolId::ArtifactSearch, json!({ "queries": queries }))
                    .confidence(Confidence::Medium)
                    .build(),
            ))
        }
        "versionNotFound" | "capabilityUnavailable"
            if artifact_search_query::version(query).is_some() =>
        {
            Some((
                "repair",
                Continuation::new(ToolId::ArtifactSearch, Value::Object(exact))
                    .confidence(Confidence::Medium)
                    .build(),
            ))
        }
        _ => None,
    };
    if let Some((name, lead)) = lead {
        error.next = Some(Box::new(json!({ name: lead })));
    }
    error
}

/// An empty keyword discovery, broadened: one variant per dropped keyword
/// (a single multi-word keyword: per dropped word), run as one batch, so
/// the variant that returns rows names what emptied the search.
pub(super) fn broaden_keywords(query: &ArtifactSearchQuery) -> Option<Value> {
    let terms = artifact_search_query::keyword_list(query);
    let words: Vec<&str> = match terms.as_slice() {
        [single] => single.split_whitespace().collect(),
        many => many.iter().map(String::as_str).collect(),
    };
    if words.len() < 2 {
        return None;
    }
    let base = copied(query);
    let variants = (0..words.len())
        .rev()
        .take(BROADEN_VARIANTS)
        .map(|dropped| {
            let mut row = base.clone();
            let kept = words
                .iter()
                .enumerate()
                .filter(|(index, _)| *index != dropped)
                .map(|(_, word)| *word)
                .collect::<Vec<_>>();
            let keywords = if terms.len() == 1 {
                json!([kept.join(" ")])
            } else {
                json!(kept)
            };
            row.insert("keywords".into(), keywords);
            Value::Object(row)
        })
        .collect::<Vec<_>>();
    Some(
        Continuation::input(ToolId::ArtifactSearch, json!({ "queries": variants }))
            .confidence(Confidence::Low)
            .build(),
    )
}

/// Broadened variants at most: one call's batch.
const BROADEN_VARIANTS: usize = 5;

#[cfg(test)]
mod tests {
    use super::{github_repo_dir, label_release_source, source_leads, source_warning};
    use crate::providers::artifact::{ArtifactItem, ArtifactType};
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
            leads["viewRepo"]["query"]["queries"][0],
            json!({"owner":"psf","repo":"requests"})
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
            leads["viewReleaseSource"]["query"]["queries"][0],
            json!({"owner":"o","repo":"r","ref":"v1.2.3"})
        );
        // The package directory is the registry's, never the built entry's.
        row.repository_directory = Some("packages/x".into());
        row.source_attested = true;
        let lead = &leads_of(&row)["viewReleaseSource"];
        assert_eq!(lead["query"]["queries"][0]["path"], "packages/x");
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
        let tree = &leads["viewReleaseSource"]["query"]["queries"][0];
        assert_eq!(
            tree,
            &json!({"owner":"rails","repo":"rails","ref":"v8.1.4"})
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
        row.name = "@types/node".into();
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
        assert_eq!(encoded["sourceRef"], "abc");
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
}
