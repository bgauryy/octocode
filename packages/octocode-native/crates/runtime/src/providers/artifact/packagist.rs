//! Packagist: exact Composer releases and keyword discovery.
mod exact;

use super::http::{JSON, RegistryClient};
use super::util::{
    commit_sha, coordinate_path, date_prefix, keyword_rows, keyword_url, license, object_for,
    parse_url, repository_url, required, safe_url, string, total,
};
use super::{ArtifactError, ArtifactItem, ArtifactProviderPage, ArtifactProviderState};
use crate::providers::artifact::query::artifact_search_query;
use octocode_contract_types::{ArtifactSearchQuery, ArtifactType};
use regex::Regex;
use serde_json::Value;
use std::sync::LazyLock;

/// A row's `abandoned`: Composer's deprecation of a package, naming the
/// replacement when the maintainer did.
fn abandoned(row: &serde_json::Map<String, Value>) -> Option<String> {
    match row.get("abandoned")? {
        Value::String(replacement) if !replacement.trim().is_empty() => {
            Some(format!("Abandoned; use {} instead.", replacement.trim()))
        }
        Value::String(_) | Value::Bool(true) => Some("Abandoned.".to_owned()),
        _ => None,
    }
}

/// Composer's `VersionParser::parseStability`: a version is stable unless it is
/// a dev branch or ends in an alpha, beta or RC modifier. `patch`, `pl` and `p`
/// modifiers are stable.
fn is_stable(version: &str) -> bool {
    static MODIFIER: LazyLock<Regex> = LazyLock::new(|| {
        #[expect(clippy::expect_used, reason = "static pattern, valid by construction")]
        Regex::new(r"[._-]?(?:(stable|beta|b|rc|alpha|a|patch|pl|p)((?:[.-]?\d+)*)?)?([.-]?dev)?(?:\+.*)?$")
            .expect("composer stability pattern")
    });
    let version = version.split('#').next().unwrap_or(version);
    if version.starts_with("dev-") || version.ends_with("-dev") {
        return false;
    }
    let version = version.to_ascii_lowercase();
    MODIFIER.captures(&version).is_none_or(|modifier| {
        modifier.get(3).is_none()
            && !matches!(
                modifier.get(1).map(|name| name.as_str()),
                Some("beta" | "b" | "alpha" | "a" | "rc")
            )
    })
}

/// The p2 entry at `index` whole. A `composer/2.0` document is minified: only
/// its first entry is complete, and each later one states what changed from the
/// entry before it, `__unset` removing a field (`MetadataMinifier::expand`).
fn expanded(
    entries: &[Value],
    index: usize,
    minified: bool,
) -> Result<serde_json::Map<String, Value>, ArtifactError> {
    let at = |entry: &Value| object_for(entry, ArtifactType::Packagist).cloned();
    let Some(chosen) = entries.get(index) else {
        return Err(super::util::invalid(ArtifactType::Packagist));
    };
    if !minified {
        return at(chosen);
    }
    let mut row = serde_json::Map::new();
    for entry in &entries[..=index] {
        for (key, value) in at(entry)? {
            if value.as_str() == Some("__unset") {
                row.remove(&key);
            } else {
                row.insert(key, value);
            }
        }
    }
    Ok(row)
}

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
    let source = row
        .get("source")
        .and_then(Value::as_object)
        .and_then(|value| repository_url(value.get("url")))
        .or_else(|| repository_url(row.get("repository")));
    if let Some(link) = source {
        artifact.set_source_link(&link);
    }
    artifact.source_ref = row
        .get("source")
        .and_then(Value::as_object)
        .and_then(|source| commit_sha(source.get("reference")));
    artifact.homepage = safe_url(row.get("homepage"));
    artifact.deprecated = abandoned(row);
    Ok(artifact)
}

/// An exact Packagist release, whole ([`expanded`]): its row, the day it was
/// tagged (`time`), and its `require` entries as `name constraint` (platform
/// ones such as `php` included, as declared). A missing `require` declares none.
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
        if entries.is_empty() {
            return Ok(ArtifactProviderPage::empty(Some(0)));
        }
        if let Some(wanted) = artifact_search_query::version(query).filter(|v| *v != "latest") {
            return exact::exact_release(&name, wanted, response.as_ref(), client).await;
        }
        // What Composer's default `minimum-stability` installs: entries run
        // newest first, and a package with no stable one names its newest.
        let stable = entries.iter().position(|entry| {
            string(entry.get("version")).is_some_and(|version| is_stable(&version))
        });
        let minified = response
            .as_ref()
            .is_some_and(|body| body.get("minified").is_some());
        let row = expanded(entries, stable.unwrap_or(0), minified)?;
        let mut artifact = composer_release(&row)?;
        artifact.only_prerelease = stable.is_none();
        return Ok(ArtifactProviderPage::single(artifact));
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
        let client = RegistryClient::new(&http, &b);
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
        let client = RegistryClient::new(&bare, &b);
        let page = packagist(&q, &state, &client).await.expect("packagist");
        assert_eq!(page.artifacts[0].dependencies, Some(Vec::new()));
        assert_eq!(page.artifacts[0].published_at, None);
    }

    /// The exact lookup of `o/r` against a p2 document with these entries.
    async fn lookup(entries: Value) -> ArtifactProviderPage {
        let http =
            RouteHttp::always(json!({"minified": "composer/2.0", "packages": {"o/r": entries}}));
        let b = test_budget();
        let client = RegistryClient::new(&http, &b);
        packagist(
            &exact_query(ArtifactType::Packagist, "o/r"),
            &ArtifactProviderState::default(),
            &client,
        )
        .await
        .expect("packagist")
    }

    /// Latest is what Composer's default `minimum-stability` installs: the
    /// newest stable entry. A p2 document is minified (only the first entry is
    /// complete; each later one states what changed, `__unset` removing a
    /// field), so the row is that entry with every change before it applied.
    #[tokio::test]
    async fn packagist_latest_is_the_newest_stable_release_with_its_inherited_fields() {
        let sha = |c: &str| c.repeat(40);
        let page = lookup(json!([
            {"name": "o/r", "version": "v8.2.0-BETA1", "description": "kept", "license": ["MIT"],
             "time": "2026-11-01T00:00:00+00:00", "homepage": "https://h.example",
             "source": {"type": "git", "url": "https://github.com/o/r.git", "reference": sha("a")},
             "require": {"php": ">=8.4", "beta/only": "^1"}},
            {"version": "v8.1.11", "time": "2026-10-01T00:00:00+00:00",
             "source": {"type": "git", "url": "https://github.com/o/r.git", "reference": sha("b")},
             "require": {"php": ">=8.2"}, "homepage": "__unset"},
            {"version": "v8.1.10",
             "source": {"type": "git", "url": "https://github.com/o/r.git", "reference": sha("c")}}
        ]))
        .await;
        let item = &page.artifacts[0];
        assert_eq!(item.version.as_deref(), Some("v8.1.11"));
        assert_eq!(item.source_ref.as_deref(), Some(sha("b").as_str()));
        assert_eq!(item.published_at.as_deref(), Some("2026-10-01"));
        assert_eq!(item.dependencies, Some(vec!["php >=8.2".to_owned()]));
        // Inherited from the complete first entry, and removed by `__unset`.
        assert_eq!(item.description.as_deref(), Some("kept"));
        assert_eq!(item.license.as_deref(), Some("MIT"));
        assert_eq!(item.homepage, None);
        // A package with no stable entry names its newest one.
        let only = lookup(json!([
            {"name": "o/r", "version": "2.0.0-RC2"},
            {"version": "2.0.0-beta1"}
        ]))
        .await;
        assert_eq!(only.artifacts[0].version.as_deref(), Some("2.0.0-RC2"));
    }

    /// Stability as Composer's `VersionParser::parseStability` reads a version.
    #[tokio::test]
    async fn packagist_reads_stability_the_way_composer_does() {
        for (version, stable) in [
            ("v8.2.0-BETA1", false),
            ("1.0.0-beta", false),
            ("1.0.0beta2", false),
            ("1.0.0-b1", false),
            ("1.0.0-alpha.3", false),
            ("1.0.0a1", false),
            ("3.0.0-RC2", false),
            ("3.0.0.rc1", false),
            ("dev-main", false),
            ("2.x-dev", false),
            ("1.0.0-beta1-dev", false),
            ("1.0.0", true),
            ("v1.2.3", true),
            ("1.2.3.4", true),
            ("1.0.0-stable", true),
            ("1.0.0-patch1", true),
            ("1.0.0-p1", true),
            ("1.0.0-pl2", true),
            ("1.0.0+build.5", true),
        ] {
            let page = lookup(json!([
                {"name": "o/r", "version": version},
                {"version": "0.9.0"}
            ]))
            .await;
            let expected = if stable { version } else { "0.9.0" };
            assert_eq!(
                page.artifacts[0].version.as_deref(),
                Some(expected),
                "{version}"
            );
        }
    }

    /// `abandoned` is Composer's deprecation of a package, with its
    /// replacement when the maintainer named one.
    #[tokio::test]
    async fn an_abandoned_package_states_it_and_its_replacement() {
        let replaced = lookup(json!([
            {"name": "o/r", "version": "6.3.0", "abandoned": "symfony/mailer"}
        ]))
        .await;
        let deprecated = replaced.artifacts[0].deprecated.as_deref().unwrap_or("");
        assert!(deprecated.contains("symfony/mailer"), "{deprecated}");
        let bare = lookup(json!([{"name": "o/r", "version": "6.3.0", "abandoned": true}])).await;
        assert!(bare.artifacts[0].deprecated.is_some());
        let live = lookup(json!([{"name": "o/r", "version": "6.3.0", "abandoned": false}])).await;
        assert_eq!(live.artifacts[0].deprecated, None);
    }
}
