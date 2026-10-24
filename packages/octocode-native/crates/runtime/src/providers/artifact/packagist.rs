//! Packagist: exact Composer releases and keyword discovery.
use super::http::{JSON, RegistryClient};
use super::util::{
    commit_sha, coordinate_path, date_prefix, keyword_rows, keyword_url, license, object_for,
    parse_url, repository_url, required, safe_url, string, total,
};
use super::{ArtifactError, ArtifactItem, ArtifactProviderPage, ArtifactProviderState};
use crate::providers::artifact::query::artifact_search_query;
use octocode_contract_types::{ArtifactSearchQuery, ArtifactType};
use serde_json::Value;

fn composer(row: &serde_json::Map<String, Value>) -> Result<ArtifactItem, ArtifactError> {
    let name = required(row.get("name"), ArtifactType::Packagist)?;
    let mut artifact = ArtifactItem::new(
        ArtifactType::Packagist,
        name.clone(),
        format!("https://packagist.org/packages/{}", coordinate_path(&name)),
    );
    artifact.version = string(row.get("version"));
    artifact.description = string(row.get("description"));
    artifact.license = license(row.get("license"));
    artifact.repository = row
        .get("source")
        .and_then(Value::as_object)
        .and_then(|value| repository_url(value.get("url")))
        .or_else(|| repository_url(row.get("repository")));
    artifact.source_ref = row
        .get("source")
        .and_then(Value::as_object)
        .and_then(|source| commit_sha(source.get("reference")));
    artifact.homepage = safe_url(row.get("homepage"));
    Ok(artifact)
}

/// An exact Packagist release: its row, the day it was tagged (`time`), and
/// its `require` entries as `name constraint` (platform ones such as `php`
/// included, as declared). The p2 document's first entry is complete, so a
/// missing `require` declares none.
fn composer_release(row: &serde_json::Map<String, Value>) -> Result<ArtifactItem, ArtifactError> {
    let mut artifact = composer(row)?;
    artifact.published_at = date_prefix(row.get("time"));
    artifact.dependencies = Some(
        row.get("require")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
            .map(|(name, constraint)| match constraint.as_str() {
                Some(constraint) => format!("{name} {constraint}"),
                None => name.clone(),
            })
            .collect(),
    );
    Ok(artifact)
}

pub(crate) async fn packagist(
    query: &ArtifactSearchQuery,
    state: &ArtifactProviderState,
    client: &RegistryClient<'_>,
) -> Result<ArtifactProviderPage, ArtifactError> {
    if let Some(package_name) = artifact_search_query::package_name(query) {
        let name = package_name.to_ascii_lowercase();
        let tagged_url = parse_url(&format!(
            "https://repo.packagist.org/p2/{}.json",
            coordinate_path(&name)
        ))?;
        let mut response = client
            .json(ArtifactType::Packagist, tagged_url, true, None, None, JSON)
            .await?;
        let mut entries = packagist_entries(response.as_ref(), &name)?;
        if entries.is_empty() {
            let dev_url = parse_url(&format!(
                "https://repo.packagist.org/p2/{}~dev.json",
                coordinate_path(&name)
            ))?;
            response = client
                .json(ArtifactType::Packagist, dev_url, true, None, None, JSON)
                .await?;
            entries = packagist_entries(response.as_ref(), &name)?;
        }
        return if let Some(first) = entries.first() {
            Ok(ArtifactProviderPage::single(composer_release(object_for(
                first,
                ArtifactType::Packagist,
            )?)?))
        } else {
            Ok(ArtifactProviderPage::empty(Some(0)))
        };
    }
    let page = state.page.unwrap_or(1);
    let size = artifact_search_query::page_size(query);
    let url = keyword_url("https://packagist.org/search.json", query, page, size)?;
    let (data, artifacts) =
        keyword_rows(client, ArtifactType::Packagist, url, "results", composer).await?;
    Ok(ArtifactProviderPage {
        next_state: string(data.get("next")).map(|_| ArtifactProviderState {
            page: Some(page + 1),
            ..Default::default()
        }),
        artifacts,
        total: total(data.get("total")),
        terminal_limit: None,
        registry: None,
    })
}

fn packagist_entries<'a>(
    response: Option<&'a Value>,
    name: &str,
) -> Result<&'a [Value], ArtifactError> {
    let Some(response) = response else {
        return Ok(&[]);
    };
    let packages = object_for(response, ArtifactType::Packagist)?
        .get("packages")
        .ok_or_else(|| super::util::invalid(ArtifactType::Packagist))?;
    Ok(packages
        .as_object()
        .and_then(|value| value.get(name))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]))
}

#[cfg(test)]
mod tests {
    use super::super::query::exact_query;
    use super::super::test_http::RouteHttp;
    use super::super::test_http::test_budget;
    use super::*;

    use serde_json::json;

    #[test]
    fn composer_source_reference_is_the_release_commit() {
        let row = json!({"name": "o/r", "version": "v1.0.0",
            "source": {"type": "git", "url": "https://github.com/o/r.git",
                       "reference": "0a2e291c0d9c0c7675d445703e51750363a549ef"}});
        let item = composer(row.as_object().unwrap()).unwrap();
        assert_eq!(
            item.source_ref.as_deref(),
            Some("0a2e291c0d9c0c7675d445703e51750363a549ef")
        );
    }

    /// An exact Packagist row states the release day (`time`) and the
    /// version's `require` entries as declared; a version entry without
    /// `require` declares none.
    #[tokio::test]
    async fn packagist_exact_rows_state_the_release_day_and_requirements() {
        let entry = json!({"name": "monolog/monolog", "version": "3.12.1",
            "time": "2026-09-29T19:21:12+00:00",
            "require": {"php": ">=8.1", "psr/log": "^2.0 || ^3.0"},
            "require-dev": {"phpunit/phpunit": "^10"}});
        let http = RouteHttp::always(json!({"packages": {"monolog/monolog": [entry]}}));
        let b = test_budget();
        let client = RegistryClient::uncached(&http, &b);
        let q = exact_query(ArtifactType::Packagist, "monolog/monolog");
        let state = ArtifactProviderState::default();
        let page = packagist(&q, &state, &client).await.expect("packagist");
        let item = &page.artifacts[0];
        assert_eq!(item.published_at.as_deref(), Some("2026-09-29"));
        assert_eq!(
            item.dependencies,
            Some(vec![
                "php >=8.1".to_owned(),
                "psr/log ^2.0 || ^3.0".to_owned()
            ])
        );
        let bare = RouteHttp::always(
            json!({"packages": {"monolog/monolog": [{"name": "monolog/monolog", "version": "1.0.0"}]}}),
        );
        let client = RegistryClient::uncached(&bare, &b);
        let page = packagist(&q, &state, &client).await.expect("packagist");
        assert_eq!(page.artifacts[0].dependencies, Some(Vec::new()));
        assert_eq!(page.artifacts[0].published_at, None);
    }
}
