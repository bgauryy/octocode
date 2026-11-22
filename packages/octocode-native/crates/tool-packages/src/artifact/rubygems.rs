//! rubygems.org: exact gems and offset-paged discovery.
use super::http::{JSON, RegistryClient};
use super::util::{
    date_prefix, endpoint, license, object_for, parse_url, required, rows, safe_url, string,
};
use super::{ArtifactError, ArtifactItem, ArtifactProviderPage, ArtifactProviderState};
use crate::artifact::query::artifact_search_query;
use octocode_contract_types::{ArtifactSearchQuery, ArtifactType, ErrorCode};
use serde_json::Value;

/// A gem row. The exact (`gems/<name>.json`) document also states the
/// release day and the runtime dependencies (`name requirement`); a search
/// row names neither, so both stay unknown there. A source link into a
/// GitHub tree (`…/tree/v8.1.4`) is split into the repository, its
/// directory, and the release ref it names (the registry's claim).
fn gem(row: &serde_json::Map<String, Value>) -> Result<ArtifactItem, ArtifactError> {
    let name = required(row.get("name"), ArtifactType::Rubygems)?;
    let mut artifact = ArtifactItem::new(
        ArtifactType::Rubygems,
        name.clone(),
        format!(
            "https://rubygems.org/gems/{}",
            super::util::encode_component(&name)
        ),
    );
    artifact.version = string(row.get("version"));
    artifact.description = string(row.get("info"));
    artifact.license = license(row.get("licenses"));
    artifact.homepage = safe_url(row.get("homepage_uri"));
    let link = string(row.get("source_code_uri")).or_else(|| {
        row.get("metadata")
            .and_then(Value::as_object)
            .and_then(|value| string(value.get("source_code_uri")))
    });
    if let Some(link) = link {
        artifact.set_source_link(&link);
        artifact.source_ref = artifact
            .version
            .as_deref()
            .and_then(|version| super::util::release_ref_in(&link, version));
    }
    artifact.published_at = date_prefix(row.get("version_created_at"));
    artifact.dependencies = row
        .get("dependencies")
        .and_then(|dependencies| dependencies.get("runtime"))
        .and_then(Value::as_array)
        .map(|runtime| {
            runtime
                .iter()
                .filter_map(|dependency| {
                    let name = string(dependency.get("name"))?;
                    Some(match string(dependency.get("requirements")) {
                        Some(requirement) => format!("{name} {requirement}"),
                        None => name,
                    })
                })
                .collect()
        });
    Ok(artifact)
}

/// An exact gem as a page, its source pinned to the upstream release tag
/// (rubygems.org never names a commit). A tag its source link names
/// (`…/tree/v8.1.4`) is the gem's claim, checked first like any declared tag.
async fn pinned(mut artifact: ArtifactItem, client: &RegistryClient<'_>) -> ArtifactProviderPage {
    if let Some(version) = artifact.version.clone() {
        let name = artifact.package_name.clone();
        let declared = artifact.source_ref.take();
        super::release_ref::pin_release_tag(
            &mut artifact,
            &name,
            &version,
            declared.as_deref(),
            client,
        )
        .await;
    }
    ArtifactProviderPage::single(artifact)
}

/// Rows rubygems.org serves per search page: fixed, whatever `per_page`
/// asks (its API docs; 30 measured 2026-10-16). A discovery page's offset
/// names its API page and the rows within it directly.
const RUBYGEMS_PAGE_ROWS: usize = 30;

pub(crate) async fn rubygems(
    query: &ArtifactSearchQuery,
    state: &ArtifactProviderState,
    client: &RegistryClient<'_>,
) -> Result<ArtifactProviderPage, ArtifactError> {
    if let Some(name) = artifact_search_query::package_name(query) {
        if let Some(wanted) = artifact_search_query::version(query).filter(|v| *v != "latest") {
            return exact_release(name, wanted, client).await;
        }
        let url = parse_url(&format!(
            "https://rubygems.org/api/v1/gems/{}.json",
            super::util::encode_component(name)
        ))?;
        let Some(response) = client
            .json(ArtifactType::Rubygems, url, true, None, None, JSON)
            .await?
        else {
            return Ok(ArtifactProviderPage::empty(Some(0)));
        };
        let artifact = gem(object_for(&response, ArtifactType::Rubygems)?)?;
        return Ok(pinned(artifact, client).await);
    }
    // A page that crosses an API page boundary reads the next one; a short
    // API page is the last.
    let size = artifact_search_query::page_size(query);
    let offset = state
        .offset
        .map_or(0, octocode_engine::num::saturating_usize);
    let mut api_page = offset / RUBYGEMS_PAGE_ROWS + 1;
    let mut skip = offset % RUBYGEMS_PAGE_ROWS;
    let mut artifacts = Vec::new();
    let mut more = false;
    loop {
        let url = endpoint(
            "https://rubygems.org/api/v1/search.json",
            &[
                ("query", Some(artifact_search_query::terms(query))),
                ("page", Some(api_page.to_string())),
            ],
        )?;
        let response = client
            .json(ArtifactType::Rubygems, url, false, None, None, JSON)
            .await?
            .ok_or_else(|| super::util::invalid(ArtifactType::Rubygems))?;
        let fetched = rows(&response, ArtifactType::Rubygems)?;
        if fetched.len() > RUBYGEMS_PAGE_ROWS {
            // Offsets would name the wrong rows: fail loudly, never skip.
            return Err(ArtifactError::new(
                ErrorCode::ProviderError,
                format!(
                    "rubygems answered {} rows on one search page, more than the {RUBYGEMS_PAGE_ROWS} its paging is computed from.",
                    fetched.len()
                ),
            ));
        }
        let wanted = size - artifacts.len();
        for row in fetched.iter().skip(skip).take(wanted) {
            artifacts.push(gem(object_for(row, ArtifactType::Rubygems)?)?);
        }
        let last = fetched.len() < RUBYGEMS_PAGE_ROWS;
        if artifacts.len() == size {
            // Rows left on this API page, or a full one, may continue.
            more = fetched.len() > skip + wanted || !last;
            break;
        }
        if last {
            break;
        }
        skip = 0;
        api_page += 1;
    }
    let next_state = (more && !artifacts.is_empty()).then(|| ArtifactProviderState {
        offset: Some((offset + artifacts.len()) as u64),
        ..Default::default()
    });
    Ok(ArtifactProviderPage {
        next_state,
        artifacts,
        total: None,
        terminal_limit: None,
        registry: None,
        any_keyword_total: None,
    })
}

/// The gem at `wanted`, from that version's own document. A 404 there says
/// neither gem nor version is known: the gem's versions list tells them apart.
async fn exact_release(
    name: &str,
    wanted: &str,
    client: &RegistryClient<'_>,
) -> Result<ArtifactProviderPage, ArtifactError> {
    let gem_path = super::util::encode_component(name);
    let url = parse_url(&format!(
        "https://rubygems.org/api/v2/rubygems/{gem_path}/versions/{}.json",
        super::util::encode_component(wanted)
    ))?;
    if let Some(response) = client
        .json(ArtifactType::Rubygems, url, true, None, None, JSON)
        .await?
    {
        let artifact = gem(object_for(&response, ArtifactType::Rubygems)?)?;
        return Ok(pinned(artifact, client).await);
    }
    let listing = parse_url(&format!(
        "https://rubygems.org/api/v1/versions/{gem_path}.json"
    ))?;
    let Some(versions) = client
        .json(ArtifactType::Rubygems, listing, true, None, None, JSON)
        .await?
    else {
        return Ok(ArtifactProviderPage::empty(Some(0)));
    };
    let published = rows(&versions, ArtifactType::Rubygems)?
        .iter()
        .filter_map(|row| string(row.get("number")))
        .collect::<Vec<_>>();
    Err(super::versions::version_not_found(
        ArtifactType::Rubygems,
        name,
        wanted,
        &published,
    ))
}

#[cfg(test)]
mod tests;
