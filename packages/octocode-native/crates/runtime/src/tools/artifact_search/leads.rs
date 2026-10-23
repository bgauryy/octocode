//! Leads from an exact registry row to its source on GitHub.
use crate::id::ToolId;
use crate::providers::artifact::types::artifact_search_query;
use crate::providers::artifact::{
    ArtifactError, ArtifactItem, ArtifactSearchQuery, ArtifactType, github_repo_dir, github_slug,
    page_out_of_range, release_ref_in,
};
use crate::response::result::{Continuation, query_row};
use crate::tools::leads::broaden::{Broaden, broaden};
use octocode_contract_types::ErrorCode;
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
/// brief and the walk position (every cursor field).
fn copied(query: &ArtifactSearchQuery) -> Map<String, Value> {
    let omit = crate::response::result::CURSOR_FIELDS
        .into_iter()
        .chain(["mainGoal", "reasoning"])
        .collect::<Vec<_>>();
    match query_row(query, &omit) {
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
    if error.code == ErrorCode::InvalidInput
        && let Some(limit) = page_out_of_range(query)
    {
        let mut last = copied(query);
        last.insert("page".into(), json!(super::last_reachable_page(query)));
        error.message = limit.warning;
        let lead = Continuation::new(ToolId::ArtifactSearch, Value::Object(last)).build();
        error.next = Some(Box::new(json!({ "repair": lead })));
        return error;
    }
    let Some(name) = artifact_search_query::bare_package_name(query) else {
        return error;
    };
    let mut exact = copied(query);
    exact.insert("packageName".into(), json!(name));
    exact.remove("version");
    let searchable = error.code == ErrorCode::NotFound || error.searchable_name;
    let lead = match error.code {
        _ if searchable && artifact_search_query::artifact_type(query) != ArtifactType::Pypi => {
            exact.remove("packageName");
            exact.insert("keywords".into(), json!([name]));
            Some((
                "broadenSearch",
                Continuation::new(ToolId::ArtifactSearch, Value::Object(exact))
                    .approximate()
                    .build(),
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
                Continuation::input(ToolId::ArtifactSearch, json!({ "queries": queries })).build(),
            ))
        }
        ErrorCode::VersionNotFound | ErrorCode::CapabilityUnavailable
            if artifact_search_query::version(query).is_some() =>
        {
            Some((
                "repair",
                Continuation::new(ToolId::ArtifactSearch, Value::Object(exact)).build(),
            ))
        }
        _ => None,
    };
    match lead {
        Some((name, lead)) => error.next = Some(Box::new(json!({ name: lead }))),
        // PyPI has no keyword search to broaden an unknown name into.
        None if error.code == ErrorCode::NotFound && error.hints.is_empty() => {
            error.hints.push(format!(
                "Check the exact packageName spelling; {} has no keyword search.",
                artifact_search_query::artifact_type(query)
            ))
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
