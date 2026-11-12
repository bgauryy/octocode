//! One named Packagist release.
use super::super::http::{JSON, RegistryClient};
use super::super::util::{coordinate_path, parse_url, string};
use super::super::{ArtifactError, ArtifactProviderPage};
use super::{composer_release, expanded, packagist_entries};
use octocode_contract_types::ArtifactType;
use serde_json::Value;

/// The release spelled `wanted`, with or without its leading `v`, from the
/// tagged document, then the `~dev` one. A version in neither names the nearest.
pub(super) async fn exact_release(
    name: &str,
    wanted: &str,
    tagged: Option<&Value>,
    client: &RegistryClient<'_>,
) -> Result<ArtifactProviderPage, ArtifactError> {
    if let Some(page) = release_in(tagged, name, wanted)? {
        return Ok(page);
    }
    let dev_url = parse_url(&format!(
        "https://repo.packagist.org/p2/{}~dev.json",
        coordinate_path(name)
    ))?;
    let dev = client
        .json(ArtifactType::Packagist, dev_url, true, None, None, JSON)
        .await?;
    if let Some(page) = release_in(dev.as_ref(), name, wanted)? {
        return Ok(page);
    }
    let mut published = Vec::new();
    for document in [tagged, dev.as_ref()] {
        let entries = packagist_entries(document, name)?;
        published.extend(
            entries
                .iter()
                .filter_map(|entry| string(entry.get("version"))),
        );
    }
    Err(super::super::versions::version_not_found(
        ArtifactType::Packagist,
        name,
        wanted,
        &published,
    ))
}

fn release_in(
    document: Option<&Value>,
    name: &str,
    wanted: &str,
) -> Result<Option<ArtifactProviderPage>, ArtifactError> {
    let entries = packagist_entries(document, name)?;
    let wanted = wanted.trim_start_matches('v');
    let Some(index) = entries.iter().position(|entry| {
        string(entry.get("version"))
            .is_some_and(|version| version.trim_start_matches('v') == wanted)
    }) else {
        return Ok(None);
    };
    let minified = document.is_some_and(|body| body.get("minified").is_some());
    let row = expanded(entries, index, minified)?;
    Ok(Some(ArtifactProviderPage::single(composer_release(&row)?)))
}
