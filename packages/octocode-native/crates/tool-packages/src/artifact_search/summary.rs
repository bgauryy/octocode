//! The `summary` view of a registry row: what the next call needs. A resolve
//! row keeps the name and version the registry read when they are not the ones
//! the request named, where its source lives and how the release ref was tied
//! to the version; a search row keeps the name, the version and the registry's
//! description. Everything else the registry said is the `details` view
//! (`view:"details"`), named in the tool description.
use crate::artifact::query::artifact_search_query;
use octocode_contract_types::ArtifactSearchQuery;
use serde_json::Value;

/// Row fields only `details` states, on a resolve and a search row alike.
/// The advisories a registry lists (PyPI, NuGet) are `securityCheck`'s
/// question: the summary leads there, and `details` lists them.
pub(super) const DETAIL_FIELDS: [&str; 10] = [
    "advisories",
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
        // A name or version the request named is restated; a range or tag it
        // resolved, or a spelling the registry normalized, is the answer.
        Some(_) => {
            fields.shift_remove("description");
            drop_echoes(fields, query);
        }
        None => {
            for resolved in RESOLVE_FIELDS {
                fields.shift_remove(resolved);
            }
        }
    }
}

/// Drops the name and version of a resolve row that the request named:
/// both views leave them out, as the caller has them.
pub(super) fn drop_echoes(
    fields: &mut serde_json::Map<String, Value>,
    query: &ArtifactSearchQuery,
) {
    let Some(name) = artifact_search_query::package_name(query) else {
        return;
    };
    if fields.get("packageName").and_then(Value::as_str) == Some(name) {
        fields.shift_remove("packageName");
    }
    let asked = artifact_search_query::version(query);
    if asked.is_some() && fields.get("version").and_then(Value::as_str) == asked {
        fields.shift_remove("version");
    }
}
