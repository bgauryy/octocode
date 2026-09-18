use super::http::RegistryClient;
use super::util::{
    coordinate_path, endpoint, license, object_for, parse_url, required, rows, safe_url, string,
    total,
};
use super::{
    ArtifactError, ArtifactItem, ArtifactProviderPage, ArtifactProviderState, ArtifactQuery,
    ArtifactType,
};
use serde_json::Value;

pub(crate) async fn pypi(
    query: &ArtifactQuery,
    client: &RegistryClient<'_>,
) -> Result<ArtifactProviderPage, ArtifactError> {
    let Some(package_name) = query.package_name.as_deref() else {
        return Err(ArtifactError::new(
            "unsupported_capability",
            "PyPI does not provide keyword search. Use type:\"pypi\" with an exact packageName.",
        )
        .with_hint("Use type:pypi with packageName for exact Python package lookup."));
    };
    let url = parse_url(&format!(
        "https://pypi.org/pypi/{}/json",
        super::util::encode_component(package_name)
    ))?;
    let Some(response) = client.json(ArtifactType::PyPi, url, true, None).await? else {
        return Ok(ArtifactProviderPage::empty(Some(0)));
    };
    let info = object_for(
        object_for(&response, ArtifactType::PyPi)?
            .get("info")
            .ok_or_else(|| super::util::invalid(ArtifactType::PyPi))?,
        ArtifactType::PyPi,
    )?;
    let name = required(info.get("name"), ArtifactType::PyPi)?;
    let mut artifact = ArtifactItem::new(
        ArtifactType::PyPi,
        name.clone(),
        format!(
            "https://pypi.org/project/{}/",
            super::util::encode_component(&name)
        ),
    );
    artifact.version = string(info.get("version"));
    artifact.description = string(info.get("summary"));
    artifact.license =
        string(info.get("license_expression")).or_else(|| string(info.get("license")));
    let links = info.get("project_urls").and_then(Value::as_object);
    artifact.homepage = safe_url(info.get("home_page"))
        .or_else(|| links.and_then(|value| safe_url(value.get("Homepage"))));
    artifact.repository = links.and_then(|links| {
        links.iter().find_map(|(label, value)| {
            matches!(
                label.to_ascii_lowercase().as_str(),
                "source" | "source code" | "repository" | "code"
            )
            .then(|| safe_url(Some(value)))
            .flatten()
        })
    });
    Ok(single(artifact))
}

fn crate_item(row: &serde_json::Map<String, Value>) -> Result<ArtifactItem, ArtifactError> {
    let name = required(
        row.get("name").or_else(|| row.get("id")),
        ArtifactType::Crates,
    )?;
    let mut artifact = ArtifactItem::new(
        ArtifactType::Crates,
        name.clone(),
        format!(
            "https://crates.io/crates/{}",
            super::util::encode_component(&name)
        ),
    );
    artifact.version =
        string(row.get("max_stable_version")).or_else(|| string(row.get("max_version")));
    artifact.description = string(row.get("description"));
    artifact.license = license(row.get("license"));
    artifact.homepage = safe_url(row.get("homepage"));
    artifact.repository = safe_url(row.get("repository"));
    Ok(artifact)
}

pub(crate) async fn crates(
    query: &ArtifactQuery,
    state: &ArtifactProviderState,
    client: &RegistryClient<'_>,
) -> Result<ArtifactProviderPage, ArtifactError> {
    if let Some(name) = query.package_name.as_deref() {
        let url = parse_url(&format!(
            "https://crates.io/api/v1/crates/{}",
            super::util::encode_component(name)
        ))?;
        let Some(response) = client.json(ArtifactType::Crates, url, true, None).await? else {
            return Ok(ArtifactProviderPage::empty(Some(0)));
        };
        let row = object_for(
            object_for(&response, ArtifactType::Crates)?
                .get("crate")
                .ok_or_else(|| super::util::invalid(ArtifactType::Crates))?,
            ArtifactType::Crates,
        )?;
        return Ok(single(crate_item(row)?));
    }
    let page = state.page.unwrap_or(1);
    let size = query.page_size.unwrap_or(10);
    let url = endpoint(
        "https://crates.io/api/v1/crates",
        &[
            ("q", Some(terms(query))),
            ("page", Some(page.to_string())),
            ("per_page", Some(size.to_string())),
        ],
    )?;
    let response = client
        .json(ArtifactType::Crates, url, false, None)
        .await?
        .ok_or_else(|| super::util::invalid(ArtifactType::Crates))?;
    let data = object_for(&response, ArtifactType::Crates)?;
    let artifacts = rows(
        data.get("crates")
            .ok_or_else(|| super::util::invalid(ArtifactType::Crates))?,
        ArtifactType::Crates,
    )?
    .iter()
    .map(|row| crate_item(object_for(row, ArtifactType::Crates)?))
    .collect::<Result<Vec<_>, _>>()?;
    let count = data
        .get("meta")
        .and_then(Value::as_object)
        .and_then(|meta| total(meta.get("total")));
    paged(artifacts, count, page, size, ArtifactType::Crates)
}

pub(crate) async fn go(
    query: &ArtifactQuery,
    state: &ArtifactProviderState,
    client: &RegistryClient<'_>,
) -> Result<ArtifactProviderPage, ArtifactError> {
    if let Some(name) = query.package_name.as_deref() {
        let path = coordinate_path(name);
        let module_url = parse_url(&format!("https://pkg.go.dev/v1/module/{path}"))?;
        let mut response = client
            .json(ArtifactType::Go, module_url, true, None)
            .await?;
        let mut is_package = false;
        if response.is_none() {
            let package_url = parse_url(&format!("https://pkg.go.dev/v1/package/{path}"))?;
            response = client
                .json(ArtifactType::Go, package_url, true, None)
                .await?;
            is_package = true;
        }
        let Some(response) = response else {
            return Ok(ArtifactProviderPage::empty(Some(0)));
        };
        let row = object_for(&response, ArtifactType::Go)?;
        let returned = required(row.get("path"), ArtifactType::Go)?;
        let mut artifact = ArtifactItem::new(
            ArtifactType::Go,
            returned.clone(),
            format!("https://pkg.go.dev/{}", coordinate_path(&returned)),
        );
        artifact.version = string(row.get("version"));
        artifact.description = string(row.get("synopsis"));
        artifact.repository = safe_url(row.get("repoUrl"));
        artifact.module_path = if is_package {
            string(row.get("modulePath"))
        } else {
            Some(returned.clone())
        };
        artifact.package_path = is_package.then_some(returned);
        return Ok(single(artifact));
    }
    let url = endpoint(
        "https://pkg.go.dev/v1/search",
        &[
            ("q", Some(terms(query))),
            ("limit", Some(query.page_size.unwrap_or(10).to_string())),
            ("token", state.token.clone()),
        ],
    )?;
    let response = client
        .json(ArtifactType::Go, url, false, None)
        .await?
        .ok_or_else(|| super::util::invalid(ArtifactType::Go))?;
    let data = object_for(&response, ArtifactType::Go)?;
    let artifacts = rows(
        data.get("items")
            .ok_or_else(|| super::util::invalid(ArtifactType::Go))?,
        ArtifactType::Go,
    )?
    .iter()
    .map(|value| {
        let row = object_for(value, ArtifactType::Go)?;
        let name = required(row.get("packagePath"), ArtifactType::Go)?;
        let mut artifact = ArtifactItem::new(
            ArtifactType::Go,
            name.clone(),
            format!("https://pkg.go.dev/{}", coordinate_path(&name)),
        );
        artifact.module_path = string(row.get("modulePath"));
        artifact.package_path = Some(name);
        artifact.version = string(row.get("version"));
        artifact.description = string(row.get("synopsis"));
        Ok(artifact)
    })
    .collect::<Result<Vec<_>, ArtifactError>>()?;
    Ok(ArtifactProviderPage {
        artifacts,
        next_state: string(data.get("nextPageToken")).map(|token| ArtifactProviderState {
            token: Some(token),
            ..Default::default()
        }),
        total: total(data.get("total")),
        terminal_limit: None,
        registry: None,
    })
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
    artifact.repository = row
        .get("source")
        .and_then(Value::as_object)
        .and_then(|value| safe_url(value.get("url")))
        .or_else(|| safe_url(row.get("repository")));
    artifact.homepage = safe_url(row.get("homepage"));
    Ok(artifact)
}

pub(crate) async fn packagist(
    query: &ArtifactQuery,
    state: &ArtifactProviderState,
    client: &RegistryClient<'_>,
) -> Result<ArtifactProviderPage, ArtifactError> {
    if let Some(package_name) = query.package_name.as_deref() {
        let name = package_name.to_ascii_lowercase();
        let tagged_url = parse_url(&format!(
            "https://repo.packagist.org/p2/{}.json",
            coordinate_path(&name)
        ))?;
        let mut response = client
            .json(ArtifactType::Packagist, tagged_url, true, None)
            .await?;
        let mut entries = packagist_entries(response.as_ref(), &name)?;
        if entries.is_empty() {
            let dev_url = parse_url(&format!(
                "https://repo.packagist.org/p2/{}~dev.json",
                coordinate_path(&name)
            ))?;
            response = client
                .json(ArtifactType::Packagist, dev_url, true, None)
                .await?;
            entries = packagist_entries(response.as_ref(), &name)?;
        }
        return if let Some(first) = entries.first() {
            Ok(single(composer(object_for(
                first,
                ArtifactType::Packagist,
            )?)?))
        } else {
            Ok(ArtifactProviderPage::empty(Some(0)))
        };
    }
    let page = state.page.unwrap_or(1);
    let size = query.page_size.unwrap_or(10);
    let url = endpoint(
        "https://packagist.org/search.json",
        &[
            ("q", Some(terms(query))),
            ("page", Some(page.to_string())),
            ("per_page", Some(size.to_string())),
        ],
    )?;
    let response = client
        .json(ArtifactType::Packagist, url, false, None)
        .await?
        .ok_or_else(|| super::util::invalid(ArtifactType::Packagist))?;
    let data = object_for(&response, ArtifactType::Packagist)?;
    let artifacts = rows(
        data.get("results")
            .ok_or_else(|| super::util::invalid(ArtifactType::Packagist))?,
        ArtifactType::Packagist,
    )?
    .iter()
    .map(|row| composer(object_for(row, ArtifactType::Packagist)?))
    .collect::<Result<Vec<_>, _>>()?;
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
    artifact.repository = safe_url(row.get("source_code_uri")).or_else(|| {
        row.get("metadata")
            .and_then(Value::as_object)
            .and_then(|value| safe_url(value.get("source_code_uri")))
    });
    Ok(artifact)
}

pub(crate) async fn rubygems(
    query: &ArtifactQuery,
    state: &ArtifactProviderState,
    client: &RegistryClient<'_>,
) -> Result<ArtifactProviderPage, ArtifactError> {
    if let Some(name) = query.package_name.as_deref() {
        let url = parse_url(&format!(
            "https://rubygems.org/api/v1/gems/{}.json",
            super::util::encode_component(name)
        ))?;
        let Some(response) = client.json(ArtifactType::Rubygems, url, true, None).await? else {
            return Ok(ArtifactProviderPage::empty(Some(0)));
        };
        return Ok(single(gem(object_for(&response, ArtifactType::Rubygems)?)?));
    }
    let page = state.page.unwrap_or(1);
    let url = endpoint(
        "https://rubygems.org/api/v1/search.json",
        &[
            ("query", Some(terms(query))),
            ("page", Some(page.to_string())),
        ],
    )?;
    let response = client
        .json(ArtifactType::Rubygems, url, false, None)
        .await?
        .ok_or_else(|| super::util::invalid(ArtifactType::Rubygems))?;
    let artifacts = rows(&response, ArtifactType::Rubygems)?
        .iter()
        .map(|row| gem(object_for(row, ArtifactType::Rubygems)?))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ArtifactProviderPage {
        next_state: (!artifacts.is_empty()).then(|| ArtifactProviderState {
            page: Some(page + 1),
            ..Default::default()
        }),
        artifacts,
        total: None,
        terminal_limit: None,
        registry: None,
    })
}

fn single(artifact: ArtifactItem) -> ArtifactProviderPage {
    ArtifactProviderPage {
        artifacts: vec![artifact],
        next_state: None,
        total: Some(1),
        terminal_limit: None,
        registry: None,
    }
}

fn terms(query: &ArtifactQuery) -> String {
    query
        .keywords
        .as_ref()
        .map(|v| v.join(" "))
        .unwrap_or_default()
}

fn paged(
    artifacts: Vec<ArtifactItem>,
    total: Option<u64>,
    page: u64,
    size: usize,
    artifact_type: ArtifactType,
) -> Result<ArtifactProviderPage, ArtifactError> {
    let more = total
        .map(|count| page.saturating_mul(size as u64) < count)
        .unwrap_or(artifacts.len() == size);
    let terminal_limit = (more && artifacts.is_empty()).then(|| {
        format!(
            "{} returned an empty page before its reported total.",
            if artifact_type == ArtifactType::Crates {
                "crates.io"
            } else {
                artifact_type.as_str()
            }
        )
    });
    Ok(ArtifactProviderPage {
        next_state: (more && !artifacts.is_empty()).then(|| ArtifactProviderState {
            page: Some(page + 1),
            ..Default::default()
        }),
        artifacts,
        total,
        terminal_limit,
        registry: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::RequestBudget;
    use crate::providers::artifact::http::{
        ArtifactHttp, ArtifactHttpFuture, ArtifactHttpRequest, ArtifactHttpResponse,
    };
    use serde_json::json;
    use std::time::Duration;

    struct MockHttp {
        status: u16,
        body: Vec<u8>,
    }

    impl MockHttp {
        fn ok(body: serde_json::Value) -> Self {
            Self {
                status: 200,
                body: serde_json::to_vec(&body).expect("registry test data should serialize"),
            }
        }
    }

    impl ArtifactHttp for MockHttp {
        fn get<'a>(
            &'a self,
            _req: ArtifactHttpRequest,
            _budget: &'a RequestBudget,
        ) -> ArtifactHttpFuture<'a> {
            let body = self.body.clone();
            let status = self.status;
            Box::pin(async move { Ok(ArtifactHttpResponse { status, body }) })
        }
    }

    fn budget() -> RequestBudget {
        RequestBudget::with_timeout(Duration::from_secs(10), 10_000_000)
    }

    fn exact_query(artifact_type: ArtifactType, name: &str) -> ArtifactQuery {
        ArtifactQuery {
            artifact_type,
            package_name: Some(name.into()),
            keywords: None,
            page_size: None,
            cursor: None,
            registry: None,
        }
    }

    #[tokio::test]
    async fn pypi_parses_exact_lookup() {
        let http = MockHttp::ok(json!({
            "info": {
                "name": "requests",
                "version": "2.31.0",
                "summary": "Python HTTP for Humans.",
                "license": "Apache-2.0",
                "home_page": "https://requests.readthedocs.io",
                "project_urls": {"Source": "https://github.com/psf/requests"}
            }
        }));
        let b = budget();
        let client = RegistryClient {
            http: &http,
            budget: &b,
        };
        let q = exact_query(ArtifactType::PyPi, "requests");
        let page = pypi(&q, &client).await.expect("pypi");
        assert_eq!(page.artifacts.len(), 1);
        let item = &page.artifacts[0];
        assert_eq!(item.name, "requests");
        assert_eq!(item.version.as_deref(), Some("2.31.0"));
        assert!(
            item.registry_url.contains("pypi.org"),
            "{}",
            item.registry_url
        );
    }

    #[tokio::test]
    async fn crates_parses_exact_lookup() {
        let http = MockHttp::ok(json!({
            "crate": {
                "id": "serde",
                "name": "serde",
                "max_stable_version": "1.0.200",
                "description": "A generic serialization/deserialization framework",
                "license": "MIT OR Apache-2.0",
                "repository": "https://github.com/serde-rs/serde"
            }
        }));
        let b = budget();
        let client = RegistryClient {
            http: &http,
            budget: &b,
        };
        let q = exact_query(ArtifactType::Crates, "serde");
        let page = crates(&q, &ArtifactProviderState::default(), &client)
            .await
            .expect("crates");
        assert_eq!(page.artifacts.len(), 1);
        let item = &page.artifacts[0];
        assert_eq!(item.name, "serde");
        assert_eq!(item.version.as_deref(), Some("1.0.200"));
        assert!(
            item.registry_url.contains("crates.io"),
            "{}",
            item.registry_url
        );
    }

    #[tokio::test]
    async fn go_parses_exact_module_lookup() {
        let http = MockHttp::ok(json!({
            "path": "github.com/gin-gonic/gin",
            "version": "v1.9.1",
            "synopsis": "HTTP web framework for Go"
        }));
        let b = budget();
        let client = RegistryClient {
            http: &http,
            budget: &b,
        };
        let q = exact_query(ArtifactType::Go, "github.com/gin-gonic/gin");
        let page = go(&q, &ArtifactProviderState::default(), &client)
            .await
            .expect("go");
        assert_eq!(page.artifacts.len(), 1);
        let item = &page.artifacts[0];
        assert_eq!(item.name, "github.com/gin-gonic/gin");
        assert_eq!(item.version.as_deref(), Some("v1.9.1"));
        assert!(
            item.registry_url.contains("pkg.go.dev"),
            "{}",
            item.registry_url
        );
    }

    #[tokio::test]
    async fn packagist_parses_exact_lookup() {
        // p2/{name}.json response with entries for the tagged URL
        let http = MockHttp::ok(json!({
            "packages": {
                "laravel/framework": [
                    {
                        "name": "laravel/framework",
                        "version": "10.0.0",
                        "description": "The Laravel Framework.",
                        "license": ["MIT"]
                    }
                ]
            }
        }));
        let b = budget();
        let client = RegistryClient {
            http: &http,
            budget: &b,
        };
        let q = exact_query(ArtifactType::Packagist, "laravel/framework");
        let page = packagist(&q, &ArtifactProviderState::default(), &client)
            .await
            .expect("packagist");
        assert_eq!(page.artifacts.len(), 1);
        let item = &page.artifacts[0];
        assert_eq!(item.name, "laravel/framework");
        assert_eq!(item.version.as_deref(), Some("10.0.0"));
        assert!(
            item.registry_url.contains("packagist.org"),
            "{}",
            item.registry_url
        );
    }

    #[tokio::test]
    async fn rubygems_parses_exact_lookup() {
        let http = MockHttp::ok(json!({
            "name": "rails",
            "version": "7.0.6",
            "info": "Full-stack web application framework.",
            "homepage_uri": "https://rubyonrails.org",
            "source_code_uri": "https://github.com/rails/rails"
        }));
        let b = budget();
        let client = RegistryClient {
            http: &http,
            budget: &b,
        };
        let q = exact_query(ArtifactType::Rubygems, "rails");
        let page = rubygems(&q, &ArtifactProviderState::default(), &client)
            .await
            .expect("rubygems");
        assert_eq!(page.artifacts.len(), 1);
        let item = &page.artifacts[0];
        assert_eq!(item.name, "rails");
        assert_eq!(item.version.as_deref(), Some("7.0.6"));
        assert!(
            item.registry_url.contains("rubygems.org"),
            "{}",
            item.registry_url
        );
    }
}
