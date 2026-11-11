//! Leads from an exact registry row to its source on GitHub.
use crate::id::ToolId;
use crate::providers::artifact::query::artifact_search_query;
use crate::providers::artifact::{
    ArtifactError, ArtifactItem, RepositoryStatus, github_slug, page_out_of_range, release_ref_in,
};
use crate::response::result::{Continuation, query_row};
use crate::tools::leads::broaden::{Broaden, broaden};
use octocode_contract_types::{ArtifactSearchQuery, ArtifactType, ErrorCode};
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
            && artifact.repository_status != Some(RepositoryStatus::Missing)
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
    let repository = artifact
        .repository
        .as_deref()
        .and_then(github_slug)
        .map_or_else(|| "its repository".to_owned(), |(o, r)| format!("{o}/{r}"));
    let reason = match (artifact.repository_status, artifact.missing_ref.as_deref()) {
        (Some(RepositoryStatus::Missing), _) => format!(
            "GitHub has no repository {repository} (deleted, private, or never there), so no release ref or GitHub lead is given{}",
            artifact.successor_repository.as_deref().map_or_else(
                String::new,
                |successor| format!(
                    "; the registry's latest release names {successor}, which may be where the project moved"
                )
            )
        ),
        (Some(RepositoryStatus::Moved), _) => format!(
            "GitHub redirects {repository} to {} (renamed or transferred), so the registry's link is stale; GitHub tools follow the redirect",
            artifact
                .successor_repository
                .as_deref()
                .unwrap_or("another name")
        ),
        (None, Some(reference)) => {
            format!("release commit {reference} is not in {repository} (unpushed or rewritten)")
        }
        (None, None) if default_branch => tag_absence(artifact).map_or_else(
            || "the registry names no release ref".to_owned(),
            |absence| format!("the registry names no release ref, and {absence}"),
        ),
        (None, None) if artifact.declared_ref => {
            let reference = artifact.source_ref.as_deref().unwrap_or_default();
            let absence =
                tag_absence(artifact).unwrap_or_else(|| "no tag names this version".to_owned());
            format!(
                "{absence}; ref {reference} is the tag its metadata declares, unchecked against the release"
            )
        }
        (None, None) => {
            let release = artifact.tag_differs.as_ref()?;
            let reference = artifact.source_ref.as_deref().unwrap_or_default();
            format!(
                "the registry names commit {reference}, but release tag {} is commit {}",
                release.tag, release.commit
            )
        }
    };
    let package = match artifact.version.as_deref() {
        Some(version) => format!("{} {version}", artifact.package_name),
        None => artifact.package_name.clone(),
    };
    let source = if default_branch {
        "; its source on the default branch may be another version"
    } else {
        ""
    };
    Some(format!("{package}: {reason}{source}."))
}

/// What the lookup could say of the tags naming this version, when it is
/// less than "no tag names it": only another package's tags do, or the
/// lookup was cut short and claims only what it checked.
fn tag_absence(artifact: &ArtifactItem) -> Option<String> {
    use crate::providers::artifact::TagAbsence;
    Some(match artifact.tag_absence.as_ref()? {
        TagAbsence::OtherPackagesOnly(tags) => format!(
            "only tags of other packages ({}) name this version",
            tags.join(", ")
        ),
        TagAbsence::ListUnread(cause) => format!(
            "no tag of the shapes checked names this version; its tag list could not be read ({cause}), so a tag may still name it"
        ),
        TagAbsence::Unchecked(cause) => {
            format!("no tag could be checked on GitHub ({cause}), so a tag may name this version")
        }
    })
}

/// The warning of an exact row whose source is not a GitHub repository: the
/// registry (and, for Maven, the POM's parent chain) names none, or names one
/// elsewhere, so the row has no release ref and no GitHub lead.
pub(super) fn unknown_source_warning(artifact: &ArtifactItem) -> Option<String> {
    if artifact
        .repository
        .as_deref()
        .and_then(github_slug)
        .is_some()
    {
        return None;
    }
    let package = match artifact.version.as_deref() {
        Some(version) => format!("{} {version}", artifact.package_name),
        None => artifact.package_name.clone(),
    };
    Some(match artifact.repository.as_deref() {
        Some(repository) => format!(
            "{package}: its source repository {repository} is not on GitHub, so no release ref or GitHub lead is given."
        ),
        None => format!(
            "{package}: the registry names no source repository, so no release ref or GitHub lead is given."
        ),
    })
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
        Some(version) => format!("{} {version}", artifact.package_name),
        None => artifact.package_name.clone(),
    };
    Some(format!(
        "{package}: dependency versions {} are properties neither its POM nor the parent POMs read define; shown as declared.",
        names.join(", ")
    ))
}

/// The warning of a Go version its module's latest `go.mod` retracts: the
/// module's own word that the version should not be used, with its reason.
pub(super) fn retraction_warning(artifact: &ArtifactItem) -> Option<String> {
    let retraction = artifact.retraction.as_ref()?;
    let reason = retraction
        .reason
        .as_deref()
        .map_or(String::new(), |reason| {
            format!(": {}", reason.trim_end_matches('.'))
        });
    Some(format!(
        "{} {} is retracted by its module's latest go.mod{reason}.",
        artifact.package_name,
        artifact.version.as_deref().unwrap_or_default()
    ))
}

/// The warning for a row that names a prerelease only because the registry has
/// no stable release: nothing the caller asked for selected it.
pub(super) fn prerelease_warning(artifact: &ArtifactItem) -> Option<String> {
    artifact.only_prerelease.then(|| {
        format!(
            "{} {}: the registry lists no stable release, so this is its newest prerelease.",
            artifact.package_name,
            artifact.version.as_deref().unwrap_or_default()
        )
    })
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
    let (owner, repo) = match artifact.repository_status {
        Some(RepositoryStatus::Missing) => return leads,
        // GitHub redirects the registry's link: the lead names the
        // repository as GitHub does now, when its metadata named it.
        Some(RepositoryStatus::Moved) => artifact
            .successor_repository
            .as_deref()
            .and_then(|successor| successor.split_once('/'))
            .map_or((owner, repo), |(owner, repo)| {
                (owner.to_owned(), repo.to_owned())
            }),
        None => (owner, repo),
    };
    let directory = artifact
        .repository_directory
        .as_deref()
        .filter(|directory| !directory.is_empty());
    // The root at depth one is ghStructure's default listing.
    let at = crate::tools::gh_shared::RepoPath {
        owner: &owner,
        repo: &repo,
        path: directory.unwrap_or_default(),
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

/// The exact row's lead to `securityCheck`: the advisories OSV lists for this
/// package at the version the row resolved, for every registry (the row itself
/// carries CVE data on a few only). Absent on a row without a version. The
/// runtime drops it where `securityCheck` is not offered.
pub(super) fn advisory_lead(artifact: &ArtifactItem) -> Option<Value> {
    let version = artifact.version.as_deref()?;
    Some(
        Continuation::related(
            ToolId::SecurityCheck,
            json!({
                "operation": "package",
                "ecosystem": crate::providers::artifact::query::artifact_type::osv_ecosystem(artifact.artifact_type).to_string(),
                "packageName": artifact.package_name,
                "version": version,
            }),
        )
        .build(),
    )
}

/// The query as a lead copies it: every field the caller set except the
/// brief and the walk position (every cursor field).
fn copied(query: &ArtifactSearchQuery) -> Map<String, Value> {
    let fields = crate::contracts::state_fields(ToolId::ArtifactSearch);
    let omit = fields.iter().map(String::as_str).collect::<Vec<_>>();
    match query_row(query, &omit) {
        Value::Object(row) => row,
        _ => Map::new(),
    }
}

/// A failure's one repair lead, when a call fixes it: a discovery page past
/// the paging range resumes the walk at the last reachable page (`repair`,
/// continued work: `page` is private walk state, so only its cursor keeps
/// it; a related call would restart at page 1); an unknown exact
/// package, or a name that is no exact coordinate of the registry, searches
/// its name as keywords (`broadenSearch`; PyPI has no search); a missing
/// version reads the nearest published ones, else the latest (`repair`); a
/// version the ecosystem cannot select reads the latest (`repair`).
pub(super) fn with_repair(mut error: ArtifactError, query: &ArtifactSearchQuery) -> ArtifactError {
    // A discovery page past the paging range: the error names the bound and
    // its one repair continues the walk at the last page this page size
    // reaches.
    if error.code == ErrorCode::InvalidInput
        && let Some(limit) = page_out_of_range(query)
    {
        error.message = limit.warning;
        if let Some(lead) = crate::response::paging::next_page_call(
            ToolId::ArtifactSearch,
            Value::Object(copied(query)),
            "page",
            super::last_reachable_page(query),
            &[],
        ) {
            error.calls = Some(Box::new(json!({ "repair": lead })));
        }
        return error;
    }
    let Some(name) = artifact_search_query::package_name(query) else {
        return error;
    };
    let mut exact = copied(query);
    exact.insert("packageName".into(), json!(name));
    exact.remove("version");
    let searchable = error.code == ErrorCode::NotFound || error.searchable_name;
    let lead = match error.code {
        _ if searchable && artifact_search_query::artifact_type(query) != ArtifactType::Pypi => {
            exact.remove("packageName");
            exact.insert("operation".into(), json!("search"));
            exact.insert("keywords".into(), json!([name]));
            Some((
                "broadenSearch",
                Continuation::related(ToolId::ArtifactSearch, Value::Object(exact)).build(),
            ))
        }
        ErrorCode::VersionNotFound if !error.nearest.is_empty() => {
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
                Continuation::related_input(ToolId::ArtifactSearch, json!({ "queries": queries }))
                    .build(),
            ))
        }
        ErrorCode::VersionNotFound | ErrorCode::CapabilityUnavailable
            if artifact_search_query::version(query).is_some() =>
        {
            Some((
                "repair",
                Continuation::related(ToolId::ArtifactSearch, Value::Object(exact)).build(),
            ))
        }
        _ => None,
    };
    match lead {
        Some((name, lead)) => error.calls = Some(Box::new(json!({ name: lead }))),
        // PyPI has no keyword search to broaden an unknown name into.
        None if error.code == ErrorCode::NotFound && error.hints.is_empty() => {
            error.hints.push(format!(
                "Check the exact packageName spelling; {} has no keyword search.",
                artifact_search_query::artifact_type(query)
            ));
        }
        None => {}
    }
    error
}

/// An empty keyword discovery, broadened ([`broaden`]): one variant per
/// dropped keyword (a lone multi-word keyword: per dropped word), run as one
/// batch, so the variant that returns rows names what emptied the search;
/// the note names what each variant drops.
pub(super) fn broaden_empty(data: &mut Value, query: &ArtifactSearchQuery) {
    let searched = Value::Object(copied(query));
    let drop = Broaden {
        strip: &[],
        groups: &[],
        split_lone_keyword: true,
    };
    let rows = crate::response::result::rows_per_call(ToolId::ArtifactSearch);
    if let Some(lead) = broaden(ToolId::ArtifactSearch, &searched, &drop, rows) {
        crate::tools::leads::broaden::set_broadened(data, &searched, lead);
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod successor_tests {
    use super::*;

    #[test]
    fn a_missing_repository_warning_names_the_successor_when_known() {
        let mut artifact =
            ArtifactItem::new(ArtifactType::Npm, "pkg".into(), "https://r/pkg".into());
        artifact.version = Some("1.0.0".into());
        artifact.repository = Some("git://github.com/old/pkg.git".into());
        artifact.repository_status = Some(RepositoryStatus::Missing);
        let plain = source_warning(&artifact).expect("warning");
        assert!(!plain.contains("latest release"), "{plain}");
        artifact.successor_repository = Some("new/pkg".into());
        let named = source_warning(&artifact).expect("warning");
        assert!(named.contains("latest release names new/pkg"), "{named}");
    }
}
