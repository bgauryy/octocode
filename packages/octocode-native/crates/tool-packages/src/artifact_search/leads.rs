//! Leads from an exact registry row to its source on GitHub.
use crate::artifact::query::artifact_search_query;
use crate::artifact::{
    ArtifactError, ArtifactItem, RepositoryStatus, github_slug, page_out_of_range, release_ref_in,
};
use octocode_contract_types::{ArtifactSearchQuery, ArtifactType, ErrorCode};
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::response::result::{Continuation, query_row};
use octocode_tools_shared::leads::broaden::{Broaden, broaden};
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
    // The verified tag is stated only for the verification it proves, and
    // only when `ref` is not already that tag.
    if artifact.verification != Some("tag") || artifact.release_tag == artifact.source_ref {
        artifact.release_tag = None;
    }
}

/// The warning a row's release source needs, one per row: a repository
/// GitHub lacks or redirects (the new name is what a citation needs), a
/// registry release commit GitHub does not have, a release tag naming
/// another commit than the registry's, and a default-branch row
/// (`verification:"defaultBranch"`) whose tag lookup was cut short or found
/// only other packages' tags. A plain default-branch row says nothing: its
/// `verification` already states that the branch may hold another version.
/// A redirect is said beside the ref's own reason, never instead of it.
pub(super) fn source_warning(artifact: &ArtifactItem) -> Option<String> {
    let default_branch = artifact.verification == Some("defaultBranch");
    let repository = artifact
        .repository
        .as_deref()
        .and_then(github_slug)
        .map_or_else(|| "its repository".to_owned(), |(o, r)| format!("{o}/{r}"));
    let moved = match artifact.repository_status {
        // Nothing of a repository GitHub lacks is read: its one reason.
        Some(RepositoryStatus::Missing) => {
            return Some(format!(
                "{}: GitHub has no repository {repository} (deleted, private, or never there), so no release ref or GitHub lead is given{}.",
                package_label(artifact),
                artifact.successor_repository.as_deref().map_or_else(
                    String::new,
                    |successor| format!(
                        "; the registry's latest release names {successor}, which may be where the project moved"
                    )
                )
            ));
        }
        Some(RepositoryStatus::Moved) => Some(format!(
            "GitHub redirects {repository} to {}",
            artifact
                .successor_repository
                .as_deref()
                .unwrap_or("another name")
        )),
        None => None,
    };
    let reference = match artifact.missing_ref.as_deref() {
        Some(reference) => Some(format!(
            "release commit {reference} is not in {repository} (unpushed or rewritten)"
        )),
        None if default_branch => tag_absence(artifact)
            .map(|absence| format!("the registry names no release ref, and {absence}")),
        None if artifact.declared_ref => {
            let reference = artifact.source_ref.as_deref().unwrap_or_default();
            let absence =
                tag_absence(artifact).unwrap_or_else(|| "no tag names this version".to_owned());
            Some(format!(
                "{absence}; ref {reference} is the tag its metadata declares, unchecked against the release"
            ))
        }
        None => artifact.tag_differs.as_ref().map(|release| {
            let reference = artifact.source_ref.as_deref().unwrap_or_default();
            format!(
                "the registry names commit {reference}, but release tag {} is commit {}",
                release.tag, release.commit
            )
        }),
    };
    let reasons = moved.into_iter().chain(reference).collect::<Vec<_>>();
    if reasons.is_empty() {
        return None;
    }
    let source = if default_branch {
        "; its source on the default branch may be another version"
    } else {
        ""
    };
    Some(format!(
        "{}: {}{source}.",
        package_label(artifact),
        reasons.join("; ")
    ))
}

/// The package a row's warning names: with its version when resolved.
fn package_label(artifact: &ArtifactItem) -> String {
    match artifact.version.as_deref() {
        Some(version) => format!("{} {version}", artifact.package_name),
        None => artifact.package_name.clone(),
    }
}

/// What the lookup could say of the tags naming this version, when it is
/// less than "no tag names it": only another package's tags do, or the
/// lookup was cut short and claims only what it checked.
fn tag_absence(artifact: &ArtifactItem) -> Option<String> {
    use crate::artifact::TagAbsence;
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

/// An exact row whose registry names no source repository at all: no field
/// shows the absence, so this says it. A repository on another host is the
/// row's `repository`, beside its ref, and needs no word.
pub(super) fn unknown_source_warning(artifact: &ArtifactItem) -> Option<String> {
    if artifact.repository.is_some() {
        return None;
    }
    let handoff = match artifact.source_ref {
        Some(_) => "the release ref is given without a GitHub lead",
        None => "no release ref or GitHub lead is given",
    };
    Some(format!(
        "{}: the registry names no source repository, so {handoff}.",
        package_label(artifact)
    ))
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
            format!(
                ": {}",
                octocode_runtime_base::security::quoted(reason.trim_end_matches('.'))
            )
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

/// The query as a lead copies it: every field the caller set except the
/// brief and the walk position (every cursor field).
fn copied(query: &ArtifactSearchQuery) -> Map<String, Value> {
    let fields = octocode_runtime_base::contracts::state_fields(ToolId::ArtifactSearch);
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
        if let Some(lead) = octocode_runtime_base::response::collection::continued_page(
            ToolId::ArtifactSearch,
            Value::Object(copied(query)),
            "page",
            super::last_reachable_page(query),
            &[],
        ) {
            error.calls = Some(Box::new(json!({ "repair": lead.build() })));
        }
        return error;
    }
    let Some(name) = artifact_search_query::package_name(query) else {
        return error;
    };
    let mut exact = copied(query);
    exact.insert("packageName".into(), json!(name));
    exact.remove("version");
    let searchable =
        (error.code == ErrorCode::NotFound && !error.missing_version) || error.searchable_name;
    let kind = artifact_search_query::artifact_type(query);
    let near = error
        .near
        .take()
        .filter(|near| searchable && !near.names.is_empty());
    // The registry has names one edit away: the spelling meant is most
    // likely one of them, most downloaded first.
    let near_lead = near.map(|near| {
        let limit =
            octocode_runtime_base::contracts::batch_limit(ToolId::ArtifactSearch).unwrap_or(1);
        let read = &near.names[..near.names.len().min(limit)];
        error.message = format!(
            "{}; {kind} has {} one edit away{}",
            error.message.trim_end_matches('.'),
            counted(read, near.measure),
            match near.names.len() - read.len() {
                0 => String::new(),
                more => format!(", and {more} more with fewer"),
            }
        );
        let mut row = exact.clone();
        row.remove("packageName");
        let queries = read
            .iter()
            .map(|(other, _)| {
                let mut row = row.clone();
                row.insert("packageName".into(), json!(other));
                Value::Object(row)
            })
            .collect::<Vec<_>>();
        (
            "repair",
            Continuation::related_input(ToolId::ArtifactSearch, json!({ "queries": queries }))
                .build(),
        )
    });
    let lead = match error.code {
        _ if near_lead.is_some() => near_lead,
        // npm names are lowercase (the registry refuses capitals in a new
        // name), so `@Angular/core` most likely means `@angular/core`.
        _ if searchable
            && kind == ArtifactType::Npm
            && name.chars().any(|c| c.is_ascii_uppercase()) =>
        {
            let mut row = copied(query);
            row.insert("packageName".into(), json!(name.to_ascii_lowercase()));
            error.hints.push(format!(
                "npm package names are lowercase: retry as {}.",
                name.to_ascii_lowercase()
            ));
            Some((
                "repair",
                Continuation::related_input(
                    ToolId::ArtifactSearch,
                    json!({ "queries": [Value::Object(row)] }),
                )
                .build(),
            ))
        }
        // A scoped npm name: the packages its scope holds, which npm's search
        // lists for the scope's name.
        _ if searchable && kind == ArtifactType::Npm && name.starts_with('@') => {
            let scope = name.split('/').next().unwrap_or(name);
            exact.remove("packageName");
            exact.insert("operation".into(), json!("search"));
            exact.insert("keywords".into(), json!([scope]));
            Some((
                "broadenSearch",
                Continuation::related(ToolId::ArtifactSearch, Value::Object(exact)).build(),
            ))
        }
        _ if searchable && kind != ArtifactType::Pypi => {
            exact.remove("packageName");
            exact.insert("operation".into(), json!("search"));
            exact.insert("keywords".into(), json!([name]));
            Some((
                "broadenSearch",
                Continuation::related(ToolId::ArtifactSearch, Value::Object(exact)).build(),
            ))
        }
        _ if error.missing_version && !error.nearest.is_empty() => {
            let limit =
                octocode_runtime_base::contracts::batch_limit(ToolId::ArtifactSearch).unwrap_or(1);
            let versions = crate::artifact::suggested(
                artifact_search_query::artifact_type(query),
                &error,
                limit,
            );
            // The repair reads the nearest published versions: no hint
            // tells the caller to discover them, and the one left names the
            // field to correct, never the package name the shared
            // not-found recovery would.
            error.point_at_version();
            let queries = versions
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
        _ if (error.missing_version || error.code == ErrorCode::CapabilityUnavailable)
            && artifact_search_query::version(query).is_some() =>
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
        None if error.code == ErrorCode::NotFound
            && !error.missing_version
            && error.hints.is_empty() =>
        {
            error.hints.push(format!(
                "Check the exact packageName spelling; {} has no keyword search.",
                artifact_search_query::artifact_type(query)
            ));
        }
        None => {}
    }
    error
}

/// `names` with their counts: `express (207355162 last-week downloads)`.
fn counted(names: &[(String, u64)], measure: &str) -> String {
    names
        .iter()
        .map(|(name, count)| format!("{name} ({count} {measure})"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// An exact row whose name has a far more downloaded name one edit away
/// (the policy's `lookalikeDownloadsRatio`) warns that it may be a
/// typosquat or a misspelling of that name, which the warning names; a
/// count that could not be read says no lookalike was checked.
pub(super) fn lookalike(
    data: &mut Value,
    query: &ArtifactSearchQuery,
    name: &str,
    near: Result<crate::artifact::lookalike::Near, ArtifactError>,
) {
    let near = match near {
        Ok(near) => near,
        Err(error) => {
            octocode_runtime_base::response::paging::push_warnings(
                data,
                [format!(
                    "{name}: the names one edit away could not be counted ({}), so no lookalike was checked.",
                    octocode_runtime_base::security::quoted(&error.message)
                )],
            );
            return;
        }
    };
    let Ok(contract) = octocode_runtime_base::contracts::security::security_contract() else {
        return;
    };
    let ratio = contract.policy.lookalike_downloads_ratio;
    let Some((other, count)) = near.lookalike(ratio) else {
        return;
    };
    let kind = artifact_search_query::artifact_type(query);
    octocode_runtime_base::response::paging::push_warnings(
        data,
        [format!(
            "{name}: {kind} has {other}, one edit away, with {count} {} against {} (at least {ratio} times, policy lookalikeDownloadsRatio): a possible typosquat or misspelling of {other}.",
            near.measure,
            near.own.unwrap_or(0)
        )],
    );
}

/// An empty first page of a keyword discovery with something to drop
/// carries the one broader call ([`broaden`]): one variant per dropped
/// keyword (a lone multi-word keyword: per dropped word), run as one batch,
/// so the variant that returns rows names what emptied the search; its hint
/// names the registry and the terms no package holds together. A lone term
/// has nothing to drop, and `status:"empty"` says all there is. A later page
/// ends a walk whose earlier pages hold the matches: it claims no absence
/// and offers nothing broader.
pub(super) fn broaden_empty(data: &mut Value, query: &ArtifactSearchQuery) {
    let searched = copied(query);
    let kind = artifact_search_query::artifact_type(query);
    let keywords = searched
        .get("keywords")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(|keyword| format!("\"{keyword}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let add_hint = octocode_runtime_base::response::result::add_hints::<[String; 1]>;
    if artifact_search_query::page(query) > 1 {
        add_hint(
            data,
            [format!(
                "The {kind} search for {keywords} ends here: the earlier pages hold every match."
            )],
        );
        return;
    }
    let drop = Broaden {
        strip: &[],
        groups: &[],
        split_lone_keyword: true,
    };
    let rows = octocode_runtime_base::response::result::rows_per_call(ToolId::ArtifactSearch);
    if let Some(lead) = broaden(
        ToolId::ArtifactSearch,
        &Value::Object(searched),
        &drop,
        rows,
    ) {
        octocode_runtime_base::response::result::attach_related(data, lead);
        add_hint(
            data,
            [format!(
                "No {kind} package matches every term of {keywords}."
            )],
        );
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod successor_tests;
