//! crates.io: exact and default crate versions, their dependencies, the
//! packaging commit (or release tag), and keyword discovery.
use super::http::{JSON, RegistryClient};
use super::util::{
    date_prefix, keyword_rows, keyword_url, license, object_for, paged, parse_url, repository_url,
    required, safe_url, string, total,
};
use super::versions::{VersionSpec, cargo_resolve};
use super::{
    ArtifactError, ArtifactItem, ArtifactProviderPage, ArtifactProviderState, ArtifactSearchQuery,
    ArtifactType,
};
use crate::providers::artifact::types::artifact_search_query;
use serde_json::Value;

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
    artifact.repository = repository_url(row.get("repository"));
    Ok(artifact)
}

/// One version's facts on its crate row.
fn apply_crate_version(artifact: &mut ArtifactItem, entry: &Value) {
    artifact.version = string(entry.get("num"));
    artifact.published_at = date_prefix(entry.get("created_at"));
    artifact.yanked = (entry.get("yanked") == Some(&Value::Bool(true))).then_some(true);
    artifact.rust_version = string(entry.get("rust_version"));
    artifact.license = license(entry.get("license")).or(artifact.license.take());
}

/// An exact crate version from its own record plus the crate's metadata
/// without its version list (`include=`): the same row the crate document
/// gives, without downloading every version. `None` when either is unknown,
/// so the crate-document read reports not-found or the nearest versions.
async fn crate_release(
    name: &str,
    version: &str,
    client: &RegistryClient<'_>,
) -> Result<Option<ArtifactItem>, ArtifactError> {
    let encoded = super::util::encode_component(name);
    let metadata = parse_url(&format!(
        "https://crates.io/api/v1/crates/{encoded}?include="
    ))?;
    let record = parse_url(&format!(
        "https://crates.io/api/v1/crates/{encoded}/{}",
        super::util::encode_component(version)
    ))?;
    let (metadata, record) = tokio::join!(
        client.json(ArtifactType::Crates, metadata, true, None, None, JSON),
        client.json(ArtifactType::Crates, record, true, None, None, JSON)
    );
    let (Some(metadata), Some(record)) = (metadata?, record?) else {
        return Ok(None);
    };
    let row = object_for(
        object_for(&metadata, ArtifactType::Crates)?
            .get("crate")
            .ok_or_else(|| super::util::invalid(ArtifactType::Crates))?,
        ArtifactType::Crates,
    )?;
    let entry = record
        .get("version")
        .filter(|entry| entry.get("num").and_then(Value::as_str) == Some(version))
        .ok_or_else(|| super::util::invalid(ArtifactType::Crates))?;
    let mut artifact = crate_item(row)?;
    apply_crate_version(&mut artifact, entry);
    Ok(Some(artifact))
}

/// A crate's latest release from its metadata plus the one version record
/// crates.io names its default (`include=default_version`): the highest
/// stable version, else the highest prerelease, else the highest yanked
/// one, which the row then marks yanked. The same row the crate document
/// gives, without downloading every version. `None` when the crate is
/// unknown or names no default, so the crate-document read decides.
async fn crate_default_release(
    name: &str,
    client: &RegistryClient<'_>,
) -> Result<Option<ArtifactItem>, ArtifactError> {
    let url = parse_url(&format!(
        "https://crates.io/api/v1/crates/{}?include=default_version",
        super::util::encode_component(name)
    ))?;
    let Some(response) = client
        .json(ArtifactType::Crates, url, true, None, None, JSON)
        .await?
    else {
        return Ok(None);
    };
    let body = object_for(&response, ArtifactType::Crates)?;
    let row = object_for(
        body.get("crate")
            .ok_or_else(|| super::util::invalid(ArtifactType::Crates))?,
        ArtifactType::Crates,
    )?;
    let default = row.get("default_version").and_then(Value::as_str);
    let Some(entry) = body
        .get("versions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|entry| default.is_some() && entry.get("num").and_then(Value::as_str) == default)
    else {
        return Ok(None);
    };
    // This view counts no versions (`max_version` is "0.0.0"): the
    // version is the default entry's.
    let mut artifact = crate_item(row)?;
    apply_crate_version(&mut artifact, entry);
    Ok(Some(artifact))
}

/// The commit a GitHub-hosted crate version was packaged from (cargo's VCS
/// info, with the crate's directory in the repository), else its upstream
/// release tag.
async fn crate_source_ref(artifact: &mut ArtifactItem, client: &RegistryClient<'_>) {
    let (Some(repository), Some(version)) = (artifact.repository.clone(), artifact.version.clone())
    else {
        return;
    };
    if !repository.contains("github.com") {
        return;
    }
    if let Some((sha, directory)) =
        super::release_ref::crate_vcs(&artifact.name, &version, client).await
    {
        artifact.source_ref = Some(sha);
        artifact.repository_directory = directory.filter(|directory| !directory.is_empty());
        return;
    }
    let name = artifact.name.clone();
    super::release_ref::pin_release_tag(artifact, &name, &version, client).await;
}

/// A crate version's normal dependencies as `name req`, plus `; target`
/// for a platform-specific one (a crate may declare one dependency per
/// target, like a PEP 508 marker): the required ones, and the optional ones
/// (features, like PyPI extras) apart. A failed read leaves both unknown;
/// it never fails the lookup.
async fn crate_dependencies(
    name: &str,
    version: &str,
    client: &RegistryClient<'_>,
) -> Option<(Vec<String>, Vec<String>)> {
    let url = parse_url(&format!(
        "https://crates.io/api/v1/crates/{}/{}/dependencies",
        super::util::encode_component(name),
        super::util::encode_component(version)
    ))
    .ok()?;
    let listing = client
        .json(ArtifactType::Crates, url, true, None, None, JSON)
        .await
        .ok()??;
    let (optional, required): (Vec<_>, Vec<_>) = listing
        .get("dependencies")?
        .as_array()?
        .iter()
        .filter(|dependency| dependency.get("kind").and_then(Value::as_str) == Some("normal"))
        .filter_map(|dependency| {
            let name = dependency.get("crate_id").and_then(Value::as_str)?;
            let mut entry = match dependency.get("req").and_then(Value::as_str) {
                Some(req) => format!("{name} {req}"),
                None => name.to_owned(),
            };
            if let Some(target) = dependency.get("target").and_then(Value::as_str) {
                entry = format!("{entry}; {target}");
            }
            Some((
                dependency.get("optional") == Some(&Value::Bool(true)),
                entry,
            ))
        })
        .partition(|(optional, _)| *optional);
    let entries = |rows: Vec<(bool, String)>| rows.into_iter().map(|(_, entry)| entry).collect();
    Some((entries(required), entries(optional)))
}

/// Release facts of a resolved crate version that need their own reads:
/// the source ref and the dependency lists, read concurrently.
async fn crate_release_facts(artifact: &mut ArtifactItem, client: &RegistryClient<'_>) {
    let Some(version) = artifact.version.clone() else {
        return;
    };
    let name = artifact.name.clone();
    let (dependencies, ()) = tokio::join!(
        crate_dependencies(&name, &version, client),
        crate_source_ref(artifact, client)
    );
    if let Some((required, optional)) = dependencies {
        artifact.dependencies = Some(required);
        artifact.optional_dependencies = (!optional.is_empty()).then_some(optional);
    }
}

pub(crate) async fn crates(
    query: &ArtifactSearchQuery,
    state: &ArtifactProviderState,
    client: &RegistryClient<'_>,
) -> Result<ArtifactProviderPage, ArtifactError> {
    if let Some(name) = artifact_search_query::bare_package_name(query) {
        let release = match artifact_search_query::version(query).map(VersionSpec::parse) {
            Some(VersionSpec::Exact(version)) => crate_release(name, &version, client).await?,
            None => crate_default_release(name, client).await?,
            Some(VersionSpec::Tag(tag)) if tag == "latest" => {
                crate_default_release(name, client).await?
            }
            Some(_) => None,
        };
        if let Some(mut artifact) = release {
            crate_release_facts(&mut artifact, client).await;
            return Ok(ArtifactProviderPage::single(artifact));
        }
        let url = parse_url(&format!(
            "https://crates.io/api/v1/crates/{}",
            super::util::encode_component(name)
        ))?;
        let Some(response) = client
            .json(ArtifactType::Crates, url, true, None, None, JSON)
            .await?
        else {
            return Ok(ArtifactProviderPage::empty(Some(0)));
        };
        let body = object_for(&response, ArtifactType::Crates)?;
        let row = object_for(
            body.get("crate")
                .ok_or_else(|| super::util::invalid(ArtifactType::Crates))?,
            ArtifactType::Crates,
        )?;
        let mut artifact = crate_item(row)?;
        // The crate response lists every version: exact, tag, and range
        // lookups resolve here without another request.
        let versions = body
            .get("versions")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let num = |entry: &Value| entry.get("num").and_then(Value::as_str).map(str::to_owned);
        let all = versions.iter().filter_map(num).collect::<Vec<_>>();
        let wanted = match artifact_search_query::version(query).map(VersionSpec::parse) {
            None => artifact.version.clone(),
            Some(VersionSpec::Tag(tag)) if tag == "latest" => artifact.version.clone(),
            Some(VersionSpec::Exact(version)) => {
                if !all.contains(&version) {
                    return Err(super::versions::version_not_found(
                        &artifact.name,
                        &version,
                        &all,
                    ));
                }
                Some(version)
            }
            Some(VersionSpec::Tag(spec) | VersionSpec::Range(spec)) => {
                let releases = versions
                    .iter()
                    .filter(|entry| entry.get("yanked") != Some(&Value::Bool(true)))
                    .filter_map(num)
                    .collect::<Vec<_>>();
                Some(
                    cargo_resolve(&spec, releases.iter().map(String::as_str)).ok_or_else(|| {
                        super::versions::version_not_found(&artifact.name, &spec, &all)
                    })?,
                )
            }
        };
        if let Some(entry) = wanted.as_deref().and_then(|wanted| {
            versions
                .iter()
                .find(|entry| entry.get("num").and_then(Value::as_str) == Some(wanted))
        }) {
            apply_crate_version(&mut artifact, entry);
        }
        crate_release_facts(&mut artifact, client).await;
        return Ok(ArtifactProviderPage::single(artifact));
    }
    let page = state.page.unwrap_or(1);
    let size = artifact_search_query::page_size(query);
    let url = keyword_url("https://crates.io/api/v1/crates", query, page, size)?;
    let (data, artifacts) =
        keyword_rows(client, ArtifactType::Crates, url, "crates", crate_item).await?;
    let count = data
        .get("meta")
        .and_then(Value::as_object)
        .and_then(|meta| total(meta.get("total")));
    paged(artifacts, count, page, size, ArtifactType::Crates)
}

/// crates.io serves a relevance-sorted keyword search only below this many
/// results (`RELEVANCE_CANDIDATE_LIMIT`; a page starting at or past it is
/// HTTP 400): the shared page bounds end discovery there.
pub(super) const CRATES_RELEVANCE_LIMIT: u64 = 1000;

#[cfg(test)]
mod tests {
    use super::super::test_http::{KnownTags, RouteHttp};
    use super::super::types::{exact_query, test_budget, versioned};
    use super::*;
    use octocode_io::budget::RequestBudget;
    use serde_json::json;

    fn serde_crate_document() -> Value {
        json!({
            "crate": {"name": "serde", "max_stable_version": "1.0.228",
                "default_version": "1.0.228", "description": "serialization", "homepage": "https://serde.rs",
                "repository": "https://github.com/serde-rs/serde"},
            "versions": [
                {"num": "1.0.228", "created_at": "2025-09-27T00:00:00Z", "yanked": false, "license": "MIT OR Apache-2.0", "rust_version": "1.61"},
                {"num": "1.0.200", "created_at": "2024-05-01T00:00:00Z", "yanked": true, "license": "MIT OR Apache-2.0", "rust_version": "1.31"},
                {"num": "1.0.100", "created_at": "2019-09-08T01:56:06Z", "yanked": false, "license": "MIT OR Apache-2.0"},
                {"num": "1.0.99", "created_at": "2019-08-01T00:00:00Z", "yanked": false}
            ]
        })
    }

    #[tokio::test]
    async fn crates_exact_versions_carry_the_packaging_commit_or_release_tag() {
        let info =
            br#"{"git":{"sha1":"b6a77c4413f902523646be0d7f5520631df53ff6"},"path_in_vcs":"serde"}"#;
        let http = RouteHttp::routes(vec![("/api/v1/crates/serde", serde_crate_document())]).bytes(
            "/crates/serde/serde-1.0.100.crate",
            super::super::release_ref::test_archive(&[
                ("serde-1.0.100/Cargo.toml", b"[package]", b'0'),
                ("serde-1.0.100/.cargo_vcs_info.json", info, b'0'),
            ]),
        );
        let tags = KnownTags::new(vec!["serde-rs/serde@v1.0.228"]);
        let b = test_budget();
        let client = RegistryClient {
            http: &http,
            budget: &b,
            cache: None,
            tags: Some(&tags),
        };
        let state = ArtifactProviderState::default();
        let pinned = crates(
            &versioned(ArtifactType::Crates, "serde", "1.0.100"),
            &state,
            &client,
        )
        .await
        .expect("pinned")
        .artifacts
        .remove(0);
        assert_eq!(
            pinned.source_ref.as_deref(),
            Some("b6a77c4413f902523646be0d7f5520631df53ff6")
        );
        assert_eq!(pinned.repository_directory.as_deref(), Some("serde"));
        let latest = crates(&exact_query(ArtifactType::Crates, "serde"), &state, &client)
            .await
            .expect("latest")
            .artifacts
            .remove(0);
        assert_eq!(latest.source_ref.as_deref(), Some("v1.0.228"));
        assert!(latest.source_tag, "an upstream tag is checked to exist");
        assert!(!pinned.source_tag, "cargo VCS info is not a tag");
        assert_eq!(latest.repository_directory, None);
    }

    /// A crate row lists its runtime dependencies like the other
    /// ecosystems: normal ones, required apart from optional, on both
    /// exact-version paths.
    #[tokio::test]
    async fn crates_rows_list_runtime_dependencies() {
        let dependencies = json!({"dependencies": [
            {"crate_id": "a", "kind": "normal", "optional": false, "req": "^1.0"},
            {"crate_id": "b", "kind": "normal", "optional": true},
            {"crate_id": "c", "kind": "dev", "optional": false},
            {"crate_id": "d", "kind": "build", "optional": false},
            {"crate_id": "e", "kind": "normal", "optional": false},
            {"crate_id": "f", "kind": "normal", "optional": false, "req": "^2", "target": "cfg(unix)"},
            {"crate_id": "f", "kind": "normal", "optional": false, "req": "^2", "target": "cfg(windows)"}
        ]});
        let document = serde_crate_document();
        let mut metadata = json!({"crate": document["crate"].clone(), "versions": null});
        metadata["crate"]["max_stable_version"] = Value::Null;
        let record = json!({"version": document["versions"][2].clone()});
        let small = RouteHttp::routes(vec![
            ("/api/v1/crates/serde?include=", metadata),
            ("/api/v1/crates/serde/1.0.100", record),
            (
                "/api/v1/crates/serde/1.0.100/dependencies",
                dependencies.clone(),
            ),
        ]);
        let whole = RouteHttp::routes(vec![
            ("/api/v1/crates/serde", document),
            ("/api/v1/crates/serde/1.0.228/dependencies", dependencies),
        ]);
        let b = test_budget();
        let pinned = crate_lookup(
            &small,
            &b,
            &versioned(ArtifactType::Crates, "serde", "1.0.100"),
        )
        .await
        .expect("pinned")
        .artifacts
        .remove(0);
        assert_eq!(pinned.dependencies.as_ref().map(Vec::len), Some(4));
        let latest = crate_lookup(&whole, &b, &exact_query(ArtifactType::Crates, "serde"))
            .await
            .expect("latest")
            .artifacts
            .remove(0);
        // The names ride the row; the list is the one dependency field.
        let row = serde_json::to_value(&latest).expect("row");
        // One dependency per target stays two entries, each with its target.
        assert_eq!(
            row["dependencies"],
            json!(["a ^1.0", "e", "f ^2; cfg(unix)", "f ^2; cfg(windows)"])
        );
        assert_eq!(row["optionalDependencies"], json!(["b"]));
    }

    #[tokio::test]
    async fn crates_versions_resolve_from_the_crate_response() {
        let http = RouteHttp::routes(vec![("/api/v1/crates/serde", serde_crate_document())]);
        let b = test_budget();
        let client = RegistryClient::uncached(&http, &b);
        let state = ArtifactProviderState::default();
        let lookup = |query: ArtifactSearchQuery| {
            let client = &client;
            let state = &state;
            async move { crates(&query, state, client).await }
        };
        let latest = lookup(exact_query(ArtifactType::Crates, "serde"))
            .await
            .expect("latest");
        assert_eq!(latest.artifacts[0].rust_version.as_deref(), Some("1.61"));
        let range = lookup(versioned(
            ArtifactType::Crates,
            "serde",
            ">=1.0.100, <1.0.228",
        ))
        .await
        .expect("range");
        assert_eq!(
            range.artifacts[0].version.as_deref(),
            Some("1.0.100"),
            "yanked 1.0.200 is skipped"
        );
        // The latest reads the default version's view and the range the
        // crate document; only the resolved version's dependency list is a
        // second read.
        assert!(
            http.targets()
                .iter()
                .filter(|path| path.starts_with("/api/"))
                .all(|path| path == "/api/v1/crates/serde"
                    || path == "/api/v1/crates/serde?include=default_version"
                    || path.ends_with("/dependencies")),
            "{:?}",
            http.targets()
        );
        // A version the registry does not know reads the crate document for
        // its nearest versions.
        let missing = lookup(versioned(ArtifactType::Crates, "serde", "1.0.101"))
            .await
            .expect_err("missing");
        assert_eq!(missing.code, "versionNotFound");
        assert!(
            missing.nearest.iter().any(|version| version == "1.0.100"),
            "{missing:?}"
        );
    }

    async fn crate_lookup(
        http: &RouteHttp,
        budget: &RequestBudget,
        query: &ArtifactSearchQuery,
    ) -> Result<ArtifactProviderPage, ArtifactError> {
        let client = RegistryClient::uncached(http, budget);
        crates(query, &ArtifactProviderState::default(), &client).await
    }

    /// An unpinned or `latest` lookup reads the crate's default version
    /// view, never the document listing every version, and returns the row
    /// that document gives. The default is crates.io's: the highest stable
    /// version, else the highest prerelease, else a yanked one, marked.
    #[tokio::test]
    async fn an_unpinned_crate_reads_only_its_default_version() {
        let slim = |default: &str, entry: Value| {
            json!({
                "crate": {"name": "serde", "max_stable_version": null, "max_version": "0.0.0",
                    "default_version": default, "description": "serialization",
                    "homepage": "https://serde.rs", "repository": "https://github.com/serde-rs/serde"},
                "versions": [entry]
            })
        };
        let document = serde_crate_document();
        let stable = slim("1.0.228", document["versions"][0].clone());
        let b = test_budget();
        let whole = RouteHttp::routes(vec![("/api/v1/crates/serde", document.clone())]);
        let from_document =
            crate_lookup(&whole, &b, &versioned(ArtifactType::Crates, "serde", "*"))
                .await
                .expect("document row")
                .artifacts
                .remove(0);
        for query in [
            exact_query(ArtifactType::Crates, "serde"),
            versioned(ArtifactType::Crates, "serde", "latest"),
        ] {
            let http = RouteHttp::routes(vec![(
                "/api/v1/crates/serde?include=default_version",
                stable.clone(),
            )]);
            let row = crate_lookup(&http, &b, &query)
                .await
                .expect("latest")
                .artifacts
                .remove(0);
            assert_eq!(row, from_document);
            assert!(
                !http
                    .targets()
                    .iter()
                    .any(|target| target == "/api/v1/crates/serde"),
                "{:?}",
                http.targets()
            );
        }
        for (default, entry, yanked) in [
            (
                "2.0.0-rc.1",
                json!({"num": "2.0.0-rc.1", "created_at": "2026-01-02T00:00:00Z", "yanked": false}),
                None,
            ),
            (
                "0.3.0",
                json!({"num": "0.3.0", "created_at": "2020-01-02T00:00:00Z", "yanked": true}),
                Some(true),
            ),
        ] {
            let http = RouteHttp::routes(vec![(
                "/api/v1/crates/serde?include=default_version",
                slim(default, entry),
            )]);
            let row = crate_lookup(&http, &b, &exact_query(ArtifactType::Crates, "serde"))
                .await
                .expect("default")
                .artifacts
                .remove(0);
            assert_eq!(
                (row.version.as_deref(), row.yanked),
                (Some(default), yanked)
            );
        }
    }

    /// An exact version reads its own record and the crate's metadata, not
    /// the crate document that lists every version, and returns the same
    /// row the document would.
    #[tokio::test]
    async fn crates_exact_versions_read_the_version_record_with_the_same_fields() {
        let document = serde_crate_document();
        let record = |num: &str| {
            json!({"version": document["versions"].as_array().expect("versions").iter()
                .find(|entry| entry["num"] == num).expect("entry")})
        };
        // `include=` leaves the version-derived fields empty, as crates.io does.
        let mut metadata = json!({"crate": document["crate"].clone(), "versions": null});
        metadata["crate"]["max_stable_version"] = Value::Null;
        metadata["crate"]["max_version"] = json!("0.0.0");
        let small = RouteHttp::routes(vec![
            ("/api/v1/crates/serde?include=", metadata),
            ("/api/v1/crates/serde/1.0.100", record("1.0.100")),
            ("/api/v1/crates/serde/1.0.200", record("1.0.200")),
        ]);
        let whole = RouteHttp::routes(vec![("/api/v1/crates/serde", document.clone())]);
        let b = test_budget();
        for (query, version) in [
            (
                versioned(ArtifactType::Crates, "serde", "1.0.100"),
                "1.0.100",
            ),
            (
                exact_query(ArtifactType::Crates, "serde@1.0.100"),
                "1.0.100",
            ),
            (
                versioned(ArtifactType::Crates, "serde", "1.0.200"),
                "1.0.200",
            ),
        ] {
            let item = crate_lookup(&small, &b, &query)
                .await
                .expect("exact")
                .artifacts
                .remove(0);
            assert_eq!(item.version.as_deref(), Some(version));
            let via_document = crate_lookup(&whole, &b, &query)
                .await
                .expect("document")
                .artifacts
                .remove(0);
            assert_eq!(
                serde_json::to_value(&item).expect("item"),
                serde_json::to_value(&via_document).expect("item"),
                "{version}"
            );
        }
        // Registry reads only (dependency lists aside); the release-ref
        // reads are lead sources.
        let mut seen: Vec<String> = small
            .targets()
            .into_iter()
            .filter(|path| path.starts_with("/api/") && !path.ends_with("/dependencies"))
            .collect();
        seen.sort();
        seen.dedup();
        assert_eq!(
            seen,
            [
                "/api/v1/crates/serde/1.0.100",
                "/api/v1/crates/serde/1.0.200",
                "/api/v1/crates/serde?include=",
            ]
        );
    }
}
