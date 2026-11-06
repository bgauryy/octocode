//! rubygems.org: exact gems and offset-paged discovery.
use super::http::{JSON, RegistryClient};
use super::util::{
    date_prefix, endpoint, license, object_for, parse_url, required, rows, safe_url, string,
};
use super::{ArtifactError, ArtifactItem, ArtifactProviderPage, ArtifactProviderState};
use crate::providers::artifact::query::artifact_search_query;
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

/// An exact gem as a page, its source pinned to the upstream release tag when
/// the registry named no ref (rubygems.org never names a commit).
async fn pinned(mut artifact: ArtifactItem, client: &RegistryClient<'_>) -> ArtifactProviderPage {
    if let Some(version) = artifact.version.clone() {
        let name = artifact.package_name.clone();
        super::release_ref::pin_release_tag(&mut artifact, &name, &version, None, client).await;
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
    Err(super::versions::version_not_found(name, wanted, &published))
}

#[cfg(test)]
mod tests {
    use super::super::query::{discovery, exact_query};
    use super::super::test_http::RouteHttp;
    use super::super::test_http::test_budget;
    use super::*;

    use serde_json::json;

    /// `count` rubygems search rows named `gem<first>`…
    fn gem_rows(first: usize, count: usize) -> Value {
        Value::Array(
            (first..first + count)
                .map(|n| json!({"name": format!("gem{n}"), "version": "1.0.0"}))
                .collect(),
        )
    }

    /// A rubygems discovery page reads the API pages its offset names
    /// directly (30 rows each): page 3 of 50 starts at row 100, API page 4
    /// row 10, and never re-reads pages 1–3. An API page with more rows
    /// than that fails loudly instead of misplacing every later page.
    #[tokio::test]
    async fn rubygems_pages_read_only_the_api_pages_their_offset_names() {
        let search = |page: usize| format!("/api/v1/search.json?query=json&page={page}");
        let http = RouteHttp::new()
            .json(&search(4), gem_rows(90, 30))
            .json(&search(5), gem_rows(120, 30))
            .json(&search(6), gem_rows(150, 12));
        let b = test_budget();
        let client = RegistryClient::new(&http, &b);
        let query = discovery(ArtifactType::Rubygems, json!({"page": 6}));
        let state = super::super::ArtifactProviderState {
            offset: Some(110),
            ..Default::default()
        };
        let page = rubygems(&query, &state, &client).await.expect("page 3");
        assert_eq!(http.targets(), [search(4), search(5)]);
        let names = page
            .artifacts
            .iter()
            .map(|row| row.package_name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names.len(), 20);
        assert_eq!((names[0], names[19]), ("gem110", "gem129"));
        assert_eq!(page.next_state.and_then(|next| next.offset), Some(130));
        // The short last API page ends the results.
        let state = super::super::ArtifactProviderState {
            offset: Some(150),
            ..Default::default()
        };
        let last = rubygems(&query, &state, &client).await.expect("last page");
        assert_eq!(http.targets()[2..], [search(6)]);
        assert_eq!(last.artifacts.len(), 12);
        assert_eq!(last.next_state, None);
        let oversized = RouteHttp::always(gem_rows(0, 31));
        let client = RegistryClient::new(&oversized, &b);
        let error = rubygems(&query, &Default::default(), &client)
            .await
            .expect_err("31 rows");
        assert_eq!(error.code, ErrorCode::ProviderError);
        assert!(error.message.contains("31 rows"), "{}", error.message);
    }

    /// An exact gem row states its release day and runtime dependencies;
    /// a source link into a GitHub tree is the repository root, with the
    /// release ref it names.
    #[tokio::test]
    async fn rubygems_exact_rows_state_the_release_day_dependencies_and_repository() {
        let http = RouteHttp::always(json!({
            "name": "rails", "version": "8.1.4", "info": "Full-stack web framework",
            "licenses": ["MIT"], "version_created_at": "2026-09-24T14:22:14.180Z",
            "source_code_uri": "https://github.com/rails/rails/tree/v8.1.4",
            "dependencies": {"development": [{"name": "dev", "requirements": ">= 0"}],
                "runtime": [{"name": "actioncable", "requirements": "= 8.1.4"},
                    {"name": "bundler", "requirements": ">= 1.15.0"}]}
        }));
        let b = test_budget();
        let client = RegistryClient::new(&http, &b);
        let page = rubygems(
            &exact_query(ArtifactType::Rubygems, "rails"),
            &Default::default(),
            &client,
        )
        .await
        .expect("rails");
        let item = &page.artifacts[0];
        assert_eq!(item.published_at.as_deref(), Some("2026-09-24"));
        assert_eq!(
            item.dependencies,
            Some(vec![
                "actioncable = 8.1.4".to_owned(),
                "bundler >= 1.15.0".to_owned()
            ])
        );
        assert_eq!(
            item.repository.as_deref(),
            Some("https://github.com/rails/rails")
        );
        assert_eq!(item.source_ref.as_deref(), Some("v8.1.4"));
        assert_eq!(item.repository_directory, None);
        // A search row names neither: both stay unknown.
        let row = gem(json!({"name": "x", "version": "1"})
            .as_object()
            .expect("row"))
        .expect("row");
        assert_eq!((row.dependencies, row.published_at), (None, None));
        let mut monorepo = [gem(json!({"name": "x", "version": "1",
            "source_code_uri": "https://github.com/o/r/tree/main/gems/x"})
        .as_object()
        .expect("row"))
        .expect("row")];
        let tags = super::super::test_http::KnownTags::new(vec!["o/r@main"]);
        super::super::test_http::settle_links(&mut monorepo, &tags).await;
        let [monorepo] = monorepo;
        assert_eq!(
            monorepo.repository.as_deref(),
            Some("https://github.com/o/r")
        );
        assert_eq!(monorepo.repository_directory.as_deref(), Some("gems/x"));
        assert_eq!(monorepo.source_ref, None, "a branch is no release ref");
    }

    /// A gem whose source link names no release ref (nokogiri's repository
    /// root) is pinned to the upstream tag of its version: `v1.19.4`.
    #[tokio::test]
    async fn a_gem_without_a_release_ref_is_pinned_to_its_upstream_tag() {
        let http = RouteHttp::always(json!({
            "name": "nokogiri", "version": "1.19.4",
            "source_code_uri": "https://github.com/sparklemotion/nokogiri/tree/v1.19.4/lib",
            "metadata": {"source_code_uri": "https://github.com/sparklemotion/nokogiri"}
        }));
        let b = test_budget();
        let tags = super::super::test_http::KnownTags::new(vec!["sparklemotion/nokogiri@v1.19.4"]);
        let client = RegistryClient {
            tags: Some(&tags),
            ..RegistryClient::new(&http, &b)
        };
        for version in [None, Some("1.19.4")] {
            let mut query =
                json!({"operation":"resolve","ecosystem":"rubygems","packageName":"nokogiri"});
            if let Some(version) = version {
                query["version"] = json!(version);
            }
            let page = rubygems(
                &super::super::query::artifact_query(query, None),
                &Default::default(),
                &client,
            )
            .await
            .expect("nokogiri");
            let item = &page.artifacts[0];
            assert_eq!(item.source_ref.as_deref(), Some("v1.19.4"), "{version:?}");
            assert!(item.source_tag, "{version:?}");
        }
    }
}
