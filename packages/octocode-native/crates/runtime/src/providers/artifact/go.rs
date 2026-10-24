//! Go: pkg.go.dev module and package lookups, the module proxy's `go.mod`
//! and release commit, and token-paged discovery.
use super::http::{JSON, RegistryClient};
use super::util::{
    commit_sha, coordinate_path, date_prefix, endpoint, object_for, parse_url, repository_url,
    required, rows, string, total,
};
use super::versions::VersionSpec;
use super::{ArtifactError, ArtifactItem, ArtifactProviderPage, ArtifactProviderState};
use crate::providers::artifact::query::artifact_search_query;
use octocode_contract_types::{ArtifactSearchQuery, ArtifactType, ErrorCode};
use octocode_io::retry::classify_status;
use serde_json::Value;

mod search;

pub(super) use search::GO_SEARCH_CAP;
use search::{go_cursor, go_page_key, go_search, go_token, go_walk};

/// An exact Go module or package lookup on pkg.go.dev, pinned to
/// `query.version()` when one is given.
async fn go_exact(
    name: &str,
    query: &ArtifactSearchQuery,
    client: &RegistryClient<'_>,
) -> Result<ArtifactProviderPage, ArtifactError> {
    let path = coordinate_path(name);
    let version = go_version(artifact_search_query::version(query))?;
    let lookup = |kind: &str| {
        endpoint(
            &format!("https://pkg.go.dev/v1/{kind}/{path}"),
            &[("version", version.clone())],
        )
    };
    // pkg.go.dev answers a package path on the module endpoint with HTTP 400
    // ("is a package, not a module") rather than 404; either sends the
    // lookup to the package endpoint.
    let (mut response, rejected) = match client
        .json(ArtifactType::Go, lookup("module")?, true, None, None, JSON)
        .await
    {
        Ok(response) => (response, None),
        Err(error) if error.status == Some(400) => (None, Some(error)),
        Err(error) => return Err(error),
    };
    let mut is_package = false;
    if response.is_none() {
        response = client
            .json(ArtifactType::Go, lookup("package")?, true, None, None, JSON)
            .await?;
        is_package = true;
        // Neither endpoint knows the path: the module rejection names why.
        if response.is_none()
            && let Some(rejected) = rejected
        {
            return Err(rejected);
        }
    }
    // A pinned lookup answers with that version or not at all.
    let response = response.filter(|row| {
        version
            .as_deref()
            .is_none_or(|version| row.get("version").and_then(Value::as_str) == Some(version))
    });
    let Some(response) = response else {
        // A pinned version the module does not publish is not an
        // unknown module: name the nearest published versions.
        if let Some(version) = version.as_deref() {
            let versions_url = parse_url(&format!("https://pkg.go.dev/v1/versions/{path}"))?;
            if let Some(listing) = client
                .json(ArtifactType::Go, versions_url, true, None, None, JSON)
                .await?
            {
                let published = listing
                    .get("items")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|item| string(item.get("version")))
                    .map(|published| published.trim_start_matches('v').to_owned())
                    .collect::<Vec<_>>();
                if !published.is_empty() {
                    return Err(super::versions::version_not_found(
                        name,
                        version.trim_start_matches('v'),
                        &published,
                    ));
                }
            }
        }
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
    // The version's commit time is its release date on pkg.go.dev.
    artifact.published_at = date_prefix(row.get("commitTime"));
    artifact.repository = repository_url(row.get("repoUrl"));
    artifact.module_path = if is_package {
        string(row.get("modulePath"))
    } else {
        Some(returned.clone())
    };
    artifact.package_path = is_package.then_some(returned);
    artifact.source_ref = go_source_ref(&artifact);
    if artifact.repository_directory.is_none() {
        artifact.repository_directory =
            go_module_directory(&artifact).filter(|directory| !directory.is_empty());
    }
    // A vanity module path (`go.opentelemetry.io/otel`) does not name its
    // repository directory or tag; the module proxy records the commit
    // the version was built from, and the directory inside the repo.
    if artifact.source_ref.is_none()
        && let (Some(module), Some(version)) =
            (artifact.module_path.clone(), artifact.version.clone())
    {
        let info = parse_url(&format!(
            "https://proxy.golang.org/{}/@v/{}.info",
            go_proxy_escape(&module),
            super::util::encode_component(&version)
        ))?;
        // The proxy is a lead source only: its failure leaves the lookup.
        if let Some(origin) = client
            .json(ArtifactType::Go, info, true, None, None, JSON)
            .await
            .ok()
            .flatten()
            .as_ref()
            .and_then(|info| info.get("Origin"))
        {
            artifact.source_ref = commit_sha(origin.get("Hash"));
            if artifact.repository_directory.is_none() {
                artifact.repository_directory =
                    string(origin.get("Subdir")).filter(|dir| !dir.is_empty());
            }
        }
    }
    artifact.dependencies = go_dependencies(&artifact, client).await;
    drop_restated_go_paths(&mut artifact);
    Ok(ArtifactProviderPage::single(artifact))
}

/// The module version's direct requirements from its `go.mod` on the
/// module proxy. Best-effort: `None` (unknown) when the proxy has none.
async fn go_dependencies(
    artifact: &ArtifactItem,
    client: &RegistryClient<'_>,
) -> Option<Vec<String>> {
    let (module, version) = (
        artifact.module_path.as_deref()?,
        artifact.version.as_deref()?,
    );
    let url = parse_url(&format!(
        "https://proxy.golang.org/{}/@v/{}.mod",
        go_proxy_escape(module),
        super::util::encode_component(version)
    ))
    .ok()?;
    let go_mod = client.text(ArtifactType::Go, url, true).await.ok()??;
    Some(go_requirements(&go_mod))
}

/// `require` directives of a `go.mod` as `module version`, in file order:
/// the module's own imports, so `// indirect` requirements are left out.
fn go_requirements(go_mod: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut in_block = false;
    for line in go_mod.lines() {
        let (code, comment) = line.split_once("//").unwrap_or((line, ""));
        let code = code.trim();
        let requirement = if in_block {
            if code == ")" {
                in_block = false;
                continue;
            }
            code
        } else {
            let Some(rest) = code
                .strip_prefix("require")
                .filter(|rest| rest.starts_with([' ', '\t', '(']))
            else {
                continue;
            };
            let rest = rest.trim();
            if rest == "(" {
                in_block = true;
                continue;
            }
            rest
        };
        let mut fields = requirement.split_whitespace();
        if let (Some(module), Some(version), None) = (fields.next(), fields.next(), fields.next())
            && !comment.trim_start().starts_with("indirect")
        {
            found.push(format!("{module} {version}"));
        }
    }
    found
}

/// A Go module or package path equal to the row's name restates it; a
/// package inside a module keeps its module path. Dropped once the row is
/// built (the source-ref lookups read the module path).
fn drop_restated_go_paths(artifact: &mut ArtifactItem) {
    if artifact.package_path.as_deref() == Some(artifact.package_name.as_str()) {
        artifact.package_path = None;
    }
    if artifact.module_path.as_deref() == Some(artifact.package_name.as_str()) {
        artifact.module_path = None;
    }
}

pub(crate) async fn go(
    query: &ArtifactSearchQuery,
    state: &ArtifactProviderState,
    client: &RegistryClient<'_>,
) -> Result<ArtifactProviderPage, ArtifactError> {
    if let Some(name) = artifact_search_query::package_name(query) {
        return go_exact(name, query, client).await;
    }
    // pkg.go.dev pages by opaque token only: page N follows the tokens of
    // the N-1 pages before it, each the same size, so pages tile the list.
    // Each next page's `cursor` carries the token that starts it, so
    // following `nextPage` reads one page with no server-side state. A page
    // without a usable cursor walks from the deepest page whose start the
    // registry cache remembers.
    let page = state.page.unwrap_or(1);
    let supplied = state
        .token
        .as_deref()
        .and_then(|cursor| go_token(&go_page_key(query, page), cursor))
        .filter(|_| page > 1);
    let served = match supplied {
        // A token the registry no longer honors (a 4xx that is no access,
        // rate-limit or timeout answer) falls back to the walk, which
        // reaches the same rows.
        Some(token) => match go_search(query, Some(token.to_owned()), client).await {
            Err(error)
                if error.status.is_some_and(|status| {
                    matches!(status, 400..=499)
                        && status != 401
                        && status != 403
                        && !classify_status(status).is_retryable()
                }) =>
            {
                None
            }
            other => Some(other?),
        },
        None => None,
    };
    let data = match served {
        Some(data) => data,
        None => match go_walk(query, page, client).await? {
            (data, true) => data,
            (end, false) => return Ok(ArtifactProviderPage::empty(total(end.get("total")))),
        },
    };
    if let Some(next) = string(data.get("nextPageToken")) {
        client.remember_cursor(&go_page_key(query, page + 1), &next);
    }
    let data = &data;
    // pkg.go.dev answers a search with no match as `items: null`.
    let Some(items) = data.get("items").filter(|items| !items.is_null()) else {
        return Ok(ArtifactProviderPage::empty(total(data.get("total"))));
    };
    let artifacts = rows(items, ArtifactType::Go)?
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
            // Search names no repository; a module on GitHub is its own.
            artifact.repository = artifact
                .module_path
                .as_deref()
                .and_then(super::util::github_repository);
            artifact.package_path = Some(name);
            artifact.version = string(row.get("version"));
            artifact.description = string(row.get("synopsis"));
            drop_restated_go_paths(&mut artifact);
            Ok(artifact)
        })
        .collect::<Result<Vec<_>, ArtifactError>>()?;
    Ok(ArtifactProviderPage {
        artifacts,
        next_state: string(data.get("nextPageToken")).map(|token| ArtifactProviderState {
            page: Some(page + 1),
            token: Some(go_cursor(&go_page_key(query, page + 1), &token)),
            ..Default::default()
        }),
        // A total at the cap is a lower bound, not a count: unknown.
        total: total(data.get("total")).filter(|&total| total < GO_SEARCH_CAP),
        terminal_limit: None,
        registry: None,
    })
}

/// A module path in the module proxy's case encoding: each upper-case letter
/// becomes `!` plus its lower-case form; segments are URL-encoded.
fn go_proxy_escape(module: &str) -> String {
    let folded = module
        .chars()
        .flat_map(|c| {
            if c.is_ascii_uppercase() {
                vec!['!', c.to_ascii_lowercase()]
            } else {
                vec![c]
            }
        })
        .collect::<String>();
    coordinate_path(&folded)
}

/// The pkg.go.dev `version` of a Go lookup: `None` for the latest release
/// (no version, or `latest`), the `v`-prefixed exact version otherwise.
/// Ranges and other tags need a module version list: not supported.
fn go_version(requested: Option<&str>) -> Result<Option<String>, ArtifactError> {
    match requested.map(VersionSpec::parse) {
        None => Ok(None),
        Some(VersionSpec::Tag(tag)) if tag == "latest" => Ok(None),
        Some(VersionSpec::Exact(version)) => Ok(Some(format!("v{version}"))),
        Some(VersionSpec::Tag(spec) | VersionSpec::Range(spec)) => Err(ArtifactError::new(
            ErrorCode::CapabilityUnavailable,
            format!("Go lookups take an exact version or latest, not \"{spec}\"."),
        )),
    }
}

/// The VCS ref a Go module version names: a pseudo-version's commit, or the
/// release tag (prefixed by the module's directory inside its repository;
/// a major-version suffix is never part of that prefix).
fn go_source_ref(artifact: &ArtifactItem) -> Option<String> {
    let version = artifact.version.as_deref()?;
    let version = version.strip_suffix("+incompatible").unwrap_or(version);
    // vX.Y.Z-yyyymmddhhmmss-abcdefabcdef (also with a .0./-0. base).
    if let Some(commit) = version
        .rsplit_once('-')
        .map(|(_, tail)| tail)
        .filter(|tail| tail.len() == 12 && version.matches('-').count() >= 2)
        .and_then(|tail| commit_sha(Some(&Value::String(tail.to_owned()))))
    {
        return Some(commit);
    }
    let directory = go_module_directory(artifact)?;
    Some(if directory.is_empty() {
        version.to_owned()
    } else {
        format!("{directory}/{version}")
    })
}

/// The module's directory inside its repository, from its path under the
/// repository root: `service/s3` for `github.com/aws/aws-sdk-go-v2/service/s3`
/// (empty at the root; a major-version suffix is never part of it). `None`
/// for a vanity path that does not start with the repository.
fn go_module_directory(artifact: &ArtifactItem) -> Option<String> {
    let module = artifact.module_path.as_deref()?;
    let repository = artifact.repository.as_deref()?;
    let root = repository
        .split_once("://")
        .map_or(repository, |(_, rest)| rest)
        .trim_end_matches('/')
        .trim_end_matches(".git");
    let dir = module.strip_prefix(root)?;
    if !(dir.is_empty() || dir.starts_with('/')) {
        return None;
    }
    let mut segments: Vec<&str> = dir.split('/').filter(|part| !part.is_empty()).collect();
    if segments.last().is_some_and(|last| {
        last.strip_prefix('v')
            .is_some_and(|major| !major.is_empty() && major.bytes().all(|b| b.is_ascii_digit()))
    }) {
        segments.pop();
    }
    Some(segments.join("/"))
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod discovery_tests;
