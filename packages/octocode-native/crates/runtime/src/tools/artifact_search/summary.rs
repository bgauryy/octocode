//! The `summary` view of a registry row: what the next call needs. A resolve
//! row keeps the package, its version, where its source lives and how the
//! release ref was tied to the version; a search row keeps the name, the
//! version and the registry's description. Everything else the registry
//! said is the `details` view, one `related` call away.
use crate::id::ToolId;
use crate::providers::artifact::query::artifact_search_query;
use crate::response::result::Continuation;
use octocode_contract_types::{ArtifactSearchQuery, ArtifactSearchView};
use serde_json::{Value, json};

/// Row fields only `details` states, on a resolve and a search row alike.
pub(super) const DETAIL_FIELDS: [&str; 9] = [
    "license",
    "homepage",
    "dependencies",
    "optionalDependencies",
    "peerDependencies",
    "engines",
    "requiresPython",
    "rustVersion",
    "downloadsMonthly",
];

/// Row fields a search row leaves to the resolve of its package, which
/// states the repository with the release ref that pins it.
pub(super) const RESOLVE_FIELDS: [&str; 7] = [
    "repository",
    "owner",
    "repo",
    "repositoryDirectory",
    "packagePath",
    "modulePath",
    "publishedAt",
];

/// `row` as the summary states it for `query`.
pub(super) fn row(row: &mut Value, query: &ArtifactSearchQuery) {
    let Some(fields) = row.as_object_mut() else {
        return;
    };
    for detail in DETAIL_FIELDS {
        fields.shift_remove(detail);
    }
    // The registry page follows from the ecosystem and the name; a registry
    // the request named is where the row was read.
    if artifact_search_query::registry_url(query).is_none() {
        fields.shift_remove("registryUrl");
    }
    match artifact_search_query::package_name(query) {
        // The resolved release is the question: what it is for is a detail.
        Some(_) => {
            fields.shift_remove("description");
        }
        None => {
            for resolved in RESOLVE_FIELDS {
                fields.shift_remove(resolved);
            }
        }
    }
}

/// The same query with `view:"details"`, as the row's lead to everything the
/// summary left out. A later discovery page has none: its cursor continues
/// the view the walk began with.
pub(super) fn details_lead(data: &mut Value, query: &ArtifactSearchQuery) {
    if artifact_search_query::page(query) > 1 {
        return;
    }
    let Ok(mut whole) = serde_json::to_value(query) else {
        return;
    };
    whole["view"] = json!(ArtifactSearchView::Details);
    data["calls"]["details"] = Continuation::related(ToolId::ArtifactSearch, whole).build();
}
