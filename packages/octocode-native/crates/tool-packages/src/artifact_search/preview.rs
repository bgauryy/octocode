//! A local plan uses the same registry selection as execution. The provider may
//! select additional endpoints or release refs in an answer we have not read.
use super::*;
use crate::artifact::preview::{destination, lookalike_destination, registries};

pub fn preview_destinations(
    query: &ArtifactSearchQuery,
    github: &str,
) -> Result<Vec<Value>, ArtifactError> {
    let kind = artifact_search_query::artifact_type(query);
    let name = artifact_search_query::package_name(query);
    let registry = selected_npm_registry(query)?;
    if let Some(registry) = &registry {
        crate::artifact::validate_npm_base(query, registry)?;
    }
    let mut sends = Vec::new();
    if let Some(name) = name {
        sends.push(format!("packageName={name}"));
    }
    if let Some(version) = artifact_search_query::version(query) {
        sends.push(format!("version={version}"));
    }
    sends.extend(
        artifact_search_query::keyword_list(query)
            .into_iter()
            .map(|term| format!("keyword={term}")),
    );
    if registry.is_some() {
        sends.push("npm authorization header when user credentials match this registry".into());
    }
    let mut out = registries(
        kind,
        registry.as_ref().map(|r| &r.base),
        name.is_some(),
        &sends,
    );
    if let Some(name) = name
        && let Some(count) = lookalike_destination(kind, name, registry.as_ref())
    {
        out.push(count);
    }
    out.push(destination(
        "github",
        Some(github),
        &["source repository and release candidates from registry metadata".into()],
        Some("registry metadata identifies a GitHub source whose release is verified"),
    ));
    Ok(out)
}
