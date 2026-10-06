//! Leads from an exact registry row to its source on GitHub.
use crate::providers::artifact::{ArtifactItem, ArtifactType};
use crate::tools::id::ToolId;
use crate::tools::result::Continuation;
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
            .and_then(|(url, version)| version_ref(url, version));
    }
    artifact.verification = if artifact.source_ref.is_none() {
        (exact
            && artifact
                .repository
                .as_deref()
                .and_then(github_repo)
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

/// The leads of an exact row whose repository is on GitHub: the
/// ghStructure tree at the package's directory (plus its entry directory
/// when the registry names one), and with a release ref the package
/// manifest at that ref. With a release ref the tree lead is
/// `viewReleaseSource`; the default-branch tree is that lead without `ref`,
/// so no second tree lead restates it. Every lead carries the row's
/// `verification` (leads are copied out of context).
pub(super) fn source_leads(artifact: &ArtifactItem) -> Map<String, Value> {
    let mut leads = Map::new();
    let Some((owner, repo)) = artifact.repository.as_deref().and_then(github_repo) else {
        return leads;
    };
    let directory = artifact
        .repository_directory
        .clone()
        .or_else(|| artifact.repository.as_deref().and_then(github_repo_dir))
        .filter(|directory| !directory.is_empty());
    let path = match (directory.as_deref(), artifact.entry_directory.as_deref()) {
        (Some(directory), Some(entry)) => Some(format!("{directory}/{entry}")),
        (directory, entry) => directory.or(entry).map(str::to_owned),
    };
    // The root at depth one is ghStructure's default listing.
    let mut query = json!({ "owner": owner, "repo": repo });
    if let Some(path) = path {
        query["path"] = json!(path);
    }
    match artifact.source_ref.as_deref() {
        None => {
            leads.insert(
                "viewRepo".into(),
                Continuation::new(ToolId::GhStructure, query)
                    .why("Default-branch code; not release evidence.")
                    .build(),
            );
        }
        Some(reference) => {
            query["ref"] = json!(reference);
            let tree = Continuation::new(ToolId::GhStructure, query)
                .why(match artifact.verification {
                    Some("provenance") => "Release commit attested by npm provenance.",
                    Some("tag") => "Upstream tag of this version.",
                    _ => "Registry release-ref lead; if unpushed, omit ref.",
                })
                .build();
            leads.insert("viewReleaseSource".into(), tree);
            if let Some(file) = manifest_file(artifact) {
                let path = directory.map_or_else(|| file.to_owned(), |dir| format!("{dir}/{file}"));
                leads.insert(
                    "readManifest".into(),
                    Continuation::new(
                        ToolId::GhGetFileContent,
                        json!({ "owner": owner, "repo": repo, "path": path, "ref": reference }),
                    )
                    .why("Dependency names at the release ref.")
                    .build(),
                );
            }
        }
    }
    if let Some(verification) = artifact.verification {
        for lead in leads.values_mut() {
            lead["verification"] = json!(verification);
        }
    }
    leads
}

/// The manifest that names a package's dependencies: the one the release
/// was checked to declare them in, else the ecosystem's fixed file name.
fn manifest_file(artifact: &ArtifactItem) -> Option<&'static str> {
    artifact.manifest.or(match artifact.artifact_type {
        ArtifactType::Npm => Some("package.json"),
        ArtifactType::Crates => Some("Cargo.toml"),
        ArtifactType::Go => Some("go.mod"),
        ArtifactType::Packagist => Some("composer.json"),
        ArtifactType::Pypi => Some("pyproject.toml"),
        _ => None,
    })
}

/// The ref of a `/tree/<ref>` or `/blob/<ref>` repository URL when it names
/// `version`'s release (`v8.1.4` for 8.1.4) and nothing after it: a branch
/// or a monorepo directory URL is not release evidence.
fn version_ref(url: &str, version: &str) -> Option<String> {
    let rest = url.split_once("github.com/")?.1;
    let rest = rest.split(['#', '?']).next()?;
    let mut parts = rest.split('/').filter(|part| !part.is_empty());
    let (_owner, _repo, kind, reference) =
        (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
    let tagged = !version.is_empty()
        && reference
            .strip_suffix(version)
            .is_some_and(|prefix| prefix.is_empty() || prefix.ends_with(['v', '-', '/', '@']));
    (matches!(kind, "tree" | "blob") && parts.next().is_none() && tagged)
        .then(|| reference.to_owned())
}

/// `owner/repo` from a GitHub repository URL in any common registry form
/// (`git+https://github.com/o/r.git`, `git@github.com:o/r`, `github.com/o/r/tree/…`).
fn github_repo(url: &str) -> Option<(String, String)> {
    let rest = url
        .split_once("github.com/")
        .or_else(|| url.split_once("github.com:"))?
        .1;
    let mut parts = rest.split(['/', '#', '?']);
    let owner = parts.next().filter(|part| !part.is_empty())?;
    let repo = parts.next()?.trim_end_matches(".git");
    (!repo.is_empty()).then(|| (owner.to_owned(), repo.to_owned()))
}

/// Monorepo subdirectory from a `/tree/<ref>/<dir>` or `/blob/<ref>/<file>`
/// repository URL. The ref is dropped: refs may contain slashes, and URL refs
/// are often stale; only viewReleaseSource pins the registry's source ref.
fn github_repo_dir(url: &str) -> Option<String> {
    let rest = url.split_once("github.com/")?.1;
    let rest = rest.split(['#', '?']).next()?;
    let mut parts = rest.split('/').filter(|part| !part.is_empty());
    let (_owner, _repo, kind, _ref) = (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
    let mut segments: Vec<&str> = parts.collect();
    match kind {
        "tree" => {}
        "blob" => {
            segments.pop();
        }
        _ => return None,
    }
    (!segments.is_empty()).then(|| segments.join("/"))
}

#[cfg(test)]
mod tests {
    use super::{github_repo, github_repo_dir, label_release_source, source_leads};
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
        // AR2: the default branch is labeled as such, never as a release.
        assert_eq!(leads["viewRepo"]["verification"], "defaultBranch");
        assert!(leads_of(&item(ArtifactType::Npm, "https://gitlab.com/o/r")).is_empty());
    }

    #[test]
    fn the_release_lead_joins_the_package_and_entry_directories() {
        let mut row = item(ArtifactType::Pypi, "https://github.com/o/r");
        row.source_ref = Some("v1.2.3".into());
        row.entry_directory = Some("src".into());
        let leads = leads_of(&row);
        // PyPI declares its dependencies in pyproject.toml.
        assert_eq!(
            leads.keys().collect::<Vec<_>>(),
            vec!["viewReleaseSource", "readManifest"]
        );
        assert_eq!(
            leads["readManifest"]["query"]["queries"][0]["path"],
            "pyproject.toml"
        );
        assert_eq!(
            leads["viewReleaseSource"]["query"]["queries"][0],
            json!({"owner":"o","repo":"r","path":"src","ref":"v1.2.3"})
        );
        row.repository_directory = Some("packages/x".into());
        row.source_attested = true;
        let lead = &leads_of(&row)["viewReleaseSource"];
        assert_eq!(lead["query"]["queries"][0]["path"], "packages/x/src");
        assert_eq!(lead["verification"], "provenance");
    }

    /// Every lead at the release ref states how that ref was checked: an
    /// npm `gitHead` without attestation is the registry's claim.
    #[test]
    fn the_manifest_lead_carries_the_release_refs_label() {
        let mut row = item(ArtifactType::Npm, "https://github.com/o/r");
        row.source_ref = Some("abc123".into());
        for lead in leads_of(&row).values() {
            assert_eq!(lead["verification"], "registryRef", "{lead}");
        }
        row.source_attested = true;
        assert_eq!(leads_of(&row)["readManifest"]["verification"], "provenance");
    }

    #[test]
    fn an_upstream_tag_and_an_unchecked_ref_are_labeled_apart() {
        let mut row = item(ArtifactType::Pypi, "https://github.com/psf/requests");
        row.source_ref = Some("v2.31.0".into());
        assert_eq!(
            leads_of(&row)["viewReleaseSource"]["verification"],
            "registryRef"
        );
        row.source_tag = true;
        let lead = &leads_of(&row)["viewReleaseSource"];
        assert_eq!(lead["verification"], "tag");
        assert!(
            lead["why"].as_str().is_some_and(|why| why.contains("tag")),
            "{lead}"
        );
    }

    #[test]
    fn a_release_ref_also_leads_to_the_manifest_at_that_ref() {
        let mut row = item(ArtifactType::Crates, "https://github.com/serde-rs/serde");
        row.source_ref = Some("b6a77c4".into());
        row.repository_directory = Some("serde".into());
        let leads = leads_of(&row);
        assert_eq!(
            leads.keys().collect::<Vec<_>>(),
            vec!["viewReleaseSource", "readManifest"]
        );
        let manifest = &leads["readManifest"];
        assert_eq!(manifest["tool"], "ghGetFileContent");
        assert_eq!(
            manifest["query"]["queries"][0],
            json!({"owner":"serde-rs","repo":"serde","path":"serde/Cargo.toml","ref":"b6a77c4"})
        );
        // The entry directory points the tree, never the manifest.
        let mut npm = item(ArtifactType::Npm, "https://github.com/o/r");
        npm.source_ref = Some("abc".into());
        npm.entry_directory = Some("lib".into());
        assert_eq!(
            leads_of(&npm)["readManifest"]["query"]["queries"][0]["path"],
            "package.json"
        );
    }

    /// E19: rubygems names its release as a `/tree/<tag>` URL: the lead
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

    #[test]
    fn parses_registry_repository_urls() {
        let expected = Some(("o".to_owned(), "r".to_owned()));
        for url in [
            "git+https://github.com/o/r.git",
            "https://github.com/o/r",
            "git@github.com:o/r.git",
            "https://github.com/o/r/tree/main/packages/x",
            "github.com/o/r#readme",
        ] {
            assert_eq!(github_repo(url), expected, "{url}");
        }
        assert_eq!(github_repo("https://gitlab.com/o/r"), None);
        assert_eq!(github_repo("https://github.com/o"), None);
    }

    /// AR2: the row is the one source of the label; a discovery row
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

    /// AR4: a release checked to declare its dependencies in setup.py
    /// points the manifest lead there.
    #[test]
    fn a_checked_manifest_points_the_manifest_lead() {
        let mut row = item(ArtifactType::Pypi, "https://github.com/psf/requests");
        row.source_ref = Some("v2.32.3".into());
        row.source_tag = true;
        row.manifest = Some("setup.py");
        assert_eq!(
            leads_of(&row)["readManifest"]["query"]["queries"][0]["path"],
            "setup.py"
        );
    }

    /// AR5: a complete dependency list replaces its own count; an absent
    /// list keeps the count (zero dependencies is evidence).
    #[test]
    fn a_complete_dependency_list_drops_its_count() {
        let mut row = item(ArtifactType::Npm, "https://github.com/o/r");
        row.dependency_list = vec!["a@1".into(), "b@2".into()];
        row.dependencies = Some(2);
        row.dedupe_dependency_count();
        let encoded = serde_json::to_value(&row).expect("row");
        assert!(encoded.get("dependencies").is_none(), "{encoded}");
        assert_eq!(encoded["dependencyList"], json!(["a@1", "b@2"]));
        let mut none = item(ArtifactType::Npm, "https://github.com/o/r");
        none.dependencies = Some(0);
        none.dedupe_dependency_count();
        assert_eq!(serde_json::to_value(&none).expect("row")["dependencies"], 0);
    }
}
