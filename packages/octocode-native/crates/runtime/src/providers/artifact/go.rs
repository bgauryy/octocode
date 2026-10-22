//! Go: pkg.go.dev module and package lookups, the module proxy's `go.mod`
//! and release commit, and token-paged discovery.
use super::http::{JSON, RegistryClient};
use super::util::{
    commit_sha, coordinate_path, date_prefix, endpoint, object_for, parse_url, repository_url,
    required, rows, string, total,
};
use super::versions::VersionSpec;
use super::{
    ArtifactError, ArtifactItem, ArtifactProviderPage, ArtifactProviderState, ArtifactSearchQuery,
    ArtifactType,
};
use crate::providers::artifact::types::artifact_search_query;
use octocode_contract_types::ErrorCode;
use serde_json::Value;

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
    if artifact.package_path.as_deref() == Some(artifact.name.as_str()) {
        artifact.package_path = None;
    }
    if artifact.module_path.as_deref() == Some(artifact.name.as_str()) {
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
        // A token the registry no longer honors (`invalidInput`, a 4xx)
        // falls back to the walk, which reaches the same rows.
        Some(token) => match go_search(query, Some(token.to_owned()), client).await {
            Err(error) if error.code == ErrorCode::InvalidInput => None,
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
        total: total(data.get("total")),
        terminal_limit: None,
        registry: None,
    })
}

/// The identity of one Go discovery page's start: its terms, page size and
/// page. The cache remembers each start's token under it, and a cursor is
/// bound to it.
fn go_page_key(query: &ArtifactSearchQuery, page: u64) -> String {
    format!(
        "go:search:{}:{}:{page}",
        artifact_search_query::page_size(query),
        artifact_search_query::terms(query)
    )
}

/// A pkg.go.dev token as the wire cursor of the page `key` names: the token
/// behind a digest of that page's identity, so a cursor copied onto another
/// query or page is ignored (that page walks) instead of serving rows that
/// are not its own.
fn go_cursor(key: &str, token: &str) -> String {
    format!("{}.{token}", go_cursor_binding(key))
}

/// The token of a cursor bound to the page `key` names; `None` for a cursor
/// bound to any other page.
fn go_token<'a>(key: &str, cursor: &'a str) -> Option<&'a str> {
    cursor
        .strip_prefix(go_cursor_binding(key).as_str())?
        .strip_prefix('.')
        .filter(|token| !token.is_empty())
}

fn go_cursor_binding(key: &str) -> String {
    // The first 8 bytes of the digest, as hex.
    octocode_engine::digest::sha256(key.as_bytes())[..16].to_owned()
}

/// One pkg.go.dev search page: the first, or the one `token` starts.
async fn go_search(
    query: &ArtifactSearchQuery,
    token: Option<String>,
    client: &RegistryClient<'_>,
) -> Result<serde_json::Map<String, Value>, ArtifactError> {
    let url = endpoint(
        "https://pkg.go.dev/v1/search",
        &[
            ("q", Some(artifact_search_query::terms(query))),
            (
                "limit",
                Some(artifact_search_query::page_size(query).to_string()),
            ),
            ("token", token),
        ],
    )?;
    let response = client
        .json(ArtifactType::Go, url, false, None, None, JSON)
        .await?
        .ok_or_else(|| super::util::invalid(ArtifactType::Go))?;
    Ok(object_for(&response, ArtifactType::Go)?.clone())
}

/// Search page `page` reached by following tokens from the deepest page
/// whose start the cache remembers (else page 1), remembering each start
/// on the way, and whether it was reached: the results can end before it
/// (then the last page read, for its total).
async fn go_walk(
    query: &ArtifactSearchQuery,
    page: u64,
    client: &RegistryClient<'_>,
) -> Result<(serde_json::Map<String, Value>, bool), ArtifactError> {
    let (mut hop, mut token) = (2..=page)
        .rev()
        .find_map(|hop| Some((hop, Some(client.cursor(&go_page_key(query, hop))?))))
        .unwrap_or((1, None));
    loop {
        let data = go_search(query, token, client).await?;
        if hop >= page {
            return Ok((data, true));
        }
        let Some(next) = string(data.get("nextPageToken")) else {
            return Ok((data, false));
        };
        client.remember_cursor(&go_page_key(query, hop + 1), &next);
        token = Some(next);
        hop += 1;
    }
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
mod tests {
    use super::super::test_http::RouteHttp;
    use super::super::types::{artifact_query, discovery, exact_query, test_budget};
    use super::*;

    use serde_json::json;

    #[test]
    fn go_source_ref_names_the_tag_or_pseudo_version_commit() {
        let item = |module: &str, version: &str| {
            let mut item = ArtifactItem::new(ArtifactType::Go, module.into(), String::new());
            item.module_path = Some(module.into());
            item.version = Some(version.into());
            item.repository = Some("https://github.com/o/r".into());
            go_source_ref(&item)
        };
        assert_eq!(
            item("github.com/o/r", "v1.10.2").as_deref(),
            Some("v1.10.2")
        );
        assert_eq!(
            item("github.com/o/r/v2", "v2.3.0").as_deref(),
            Some("v2.3.0")
        );
        assert_eq!(
            item("github.com/o/r/sub/mod", "v0.4.1").as_deref(),
            Some("sub/mod/v0.4.1")
        );
        assert_eq!(
            item("github.com/o/r", "v2.0.0+incompatible").as_deref(),
            Some("v2.0.0")
        );
        assert_eq!(
            item("github.com/o/r", "v0.0.0-20240101120000-abcdef123456").as_deref(),
            Some("abcdef123456")
        );
        assert_eq!(item("github.com/o/rx", "v1.0.0"), None);
    }

    /// A module inside a repository keeps its directory, so the release
    /// lead lists the module and not the repository root.
    #[test]
    fn go_module_directory_is_the_path_under_the_repository_root() {
        let directory = |module: &str| {
            let mut item = ArtifactItem::new(ArtifactType::Go, module.into(), String::new());
            item.module_path = Some(module.into());
            item.repository = Some("https://github.com/aws/aws-sdk-go-v2".into());
            go_module_directory(&item)
        };
        assert_eq!(
            directory("github.com/aws/aws-sdk-go-v2/service/s3").as_deref(),
            Some("service/s3")
        );
        assert_eq!(
            directory("github.com/aws/aws-sdk-go-v2/v2").as_deref(),
            Some("")
        );
        assert_eq!(directory("go.example.com/x"), None);
    }

    /// An exact Go module row: its release day from the module record, and
    /// its direct requirements from the version's `go.mod` on the module
    /// proxy (`// indirect` ones left out); without that file the
    /// dependencies are unknown, not none.
    #[tokio::test]
    async fn go_parses_exact_module_lookup() {
        let module = json!({
            "path": "github.com/gin-gonic/gin",
            "version": "v1.9.1",
            "synopsis": "HTTP web framework for Go",
            "commitTime": "2023-06-12T08:50:28Z"
        });
        let go_mod = "module github.com/gin-gonic/gin\n\ngo 1.20\n\nrequire github.com/bytedance/sonic v1.9.1\n\
            require (\n\tgithub.com/gin-contrib/sse v0.1.0 // pinned\n\tgolang.org/x/net v0.10.0 // indirect\n)\n\
            replace example.com/x => ../x\n";
        let b = test_budget();
        let q = exact_query(ArtifactType::Go, "github.com/gin-gonic/gin");
        let http = RouteHttp::routes(vec![(
            "/v1/module/github.com/gin-gonic/gin",
            module.clone(),
        )])
        .bytes("/github.com/gin-gonic/gin/@v/v1.9.1.mod", go_mod);
        let client = RegistryClient::uncached(&http, &b);
        let page = go(&q, &ArtifactProviderState::default(), &client)
            .await
            .expect("go");
        assert_eq!(page.artifacts.len(), 1);
        let item = &page.artifacts[0];
        assert_eq!(item.name, "github.com/gin-gonic/gin");
        assert_eq!(item.version.as_deref(), Some("v1.9.1"));
        assert_eq!(item.published_at.as_deref(), Some("2023-06-12"));
        assert_eq!(
            item.registry_url,
            "https://pkg.go.dev/github.com/gin-gonic/gin"
        );
        assert_eq!(
            item.dependencies,
            Some(vec![
                "github.com/bytedance/sonic v1.9.1".to_owned(),
                "github.com/gin-contrib/sse v0.1.0".to_owned(),
            ])
        );
        let http = RouteHttp::routes(vec![("/v1/module/github.com/gin-gonic/gin", module)]);
        let client = RegistryClient::uncached(&http, &b);
        let page = go(&q, &ArtifactProviderState::default(), &client)
            .await
            .expect("go without go.mod");
        assert_eq!(page.artifacts[0].dependencies, None);
    }

    /// pkg.go.dev rejects a package path on the module endpoint (HTTP 400,
    /// not 404): the lookup still reads the package endpoint and returns the
    /// package row with its containing module; a path neither endpoint knows
    /// keeps the module rejection.
    #[tokio::test]
    async fn go_package_paths_rejected_as_modules_read_the_package_endpoint() {
        let rejection = json!({"code": 400,
            "message": "golang.org/x/net/http2 is a package, not a module"});
        let http = RouteHttp::new()
            .status("/v1/module/golang.org/x/net/http2", 400, rejection.clone())
            .json(
                "/v1/package/golang.org/x/net/http2",
                json!({"path": "golang.org/x/net/http2", "modulePath": "golang.org/x/net",
                    "version": "v0.61.0", "synopsis": "Package http2 implements the HTTP/2 protocol."}),
            );
        let b = test_budget();
        let client = RegistryClient::uncached(&http, &b);
        let q = exact_query(ArtifactType::Go, "golang.org/x/net/http2");
        let page = go(&q, &ArtifactProviderState::default(), &client)
            .await
            .expect("package fallback");
        let item = &page.artifacts[0];
        assert_eq!(item.name, "golang.org/x/net/http2");
        assert_eq!(item.module_path.as_deref(), Some("golang.org/x/net"));
        assert_eq!(item.version.as_deref(), Some("v0.61.0"));
        let paths = |http: &RouteHttp| {
            http.seen()
                .iter()
                .map(|seen| seen.url.path().to_owned())
                .collect::<Vec<_>>()
        };
        let lookups = [
            "/v1/module/golang.org/x/net/http2",
            "/v1/package/golang.org/x/net/http2",
        ];
        assert_eq!(paths(&http)[..2], lookups, "{:?}", paths(&http));
        let unknown = RouteHttp::new().status("/v1/module/golang.org/x/net/http2", 400, rejection);
        let client = RegistryClient::uncached(&unknown, &b);
        let error = go(&q, &ArtifactProviderState::default(), &client)
            .await
            .expect_err("rejected by both endpoints");
        assert_eq!(error.status, Some(400));
        assert!(
            error.message.contains("is a package, not a module"),
            "{error:?}"
        );
        assert_eq!(paths(&unknown), lookups);
    }

    /// A pinned Go module version is looked up as that version, so its
    /// release ref names the pinned tag, not the latest one; an unpublished
    /// version names the nearest published ones, and ranges are refused.
    #[tokio::test]
    async fn go_pinned_versions_resolve_the_pinned_release() {
        let module = "/v1/module/github.com/open-telemetry/opentelemetry-go";
        let http = RouteHttp::routes(vec![
            (
                "/v1/module/github.com/open-telemetry/opentelemetry-go?version=v0.71.0",
                json!({"path":"github.com/open-telemetry/opentelemetry-go","version":"v0.71.0",
                    "repoUrl":"https://github.com/open-telemetry/opentelemetry-go"}),
            ),
            (
                module,
                json!({"path":"github.com/open-telemetry/opentelemetry-go","version":"v0.72.0",
                    "repoUrl":"https://github.com/open-telemetry/opentelemetry-go"}),
            ),
            (
                "/v1/versions/github.com/open-telemetry/opentelemetry-go",
                json!({"items":[{"version":"v0.72.0"},{"version":"v0.71.0"},{"version":"v0.70.0"}]}),
            ),
        ]);
        let b = test_budget();
        let client = RegistryClient::uncached(&http, &b);
        let lookup = |version: Option<&str>| {
            let mut fields = json!({"ecosystem":"go","packageName":"github.com/open-telemetry/opentelemetry-go"});
            if let Some(version) = version {
                fields["version"] = json!(version);
            }
            artifact_query(fields, None)
        };
        let state = ArtifactProviderState::default();
        for pinned in ["v0.71.0", "0.71.0"] {
            let page = go(&lookup(Some(pinned)), &state, &client)
                .await
                .expect("pinned");
            let item = &page.artifacts[0];
            assert_eq!(item.version.as_deref(), Some("v0.71.0"));
            assert_eq!(item.source_ref.as_deref(), Some("v0.71.0"));
        }
        let latest = go(&lookup(Some("latest")), &state, &client)
            .await
            .expect("latest");
        assert_eq!(latest.artifacts[0].version.as_deref(), Some("v0.72.0"));
        let missing = go(&lookup(Some("v0.69.9")), &state, &client)
            .await
            .expect_err("unpublished");
        assert_eq!(missing.code, ErrorCode::VersionNotFound);
        assert!(
            missing.nearest.iter().any(|version| version == "0.70.0"),
            "{missing:?}"
        );
        let range = go(&lookup(Some("^0.71")), &state, &client)
            .await
            .expect_err("range");
        assert_eq!(range.code, ErrorCode::CapabilityUnavailable);
    }

    /// A vanity module path names neither its repository directory nor its
    /// tag: the release commit and directory come from the module proxy.
    #[tokio::test]
    async fn go_vanity_modules_take_the_release_commit_from_the_proxy() {
        let http = RouteHttp::routes(vec![
            (
                "/v1/module/go.opentelemetry.io/otel/sdk?version=v1.30.0",
                json!({"path":"go.opentelemetry.io/otel/sdk","version":"v1.30.0",
                    "repoUrl":"https://github.com/open-telemetry/opentelemetry-go"}),
            ),
            (
                "/go.opentelemetry.io/otel/sdk/@v/v1.30.0.info",
                json!({"Version":"v1.30.0","Origin":{"VCS":"git",
                    "URL":"https://github.com/open-telemetry/opentelemetry-go","Subdir":"sdk",
                    "Hash":"ed4fc757583a88b4da51b1fe1c3f0703ac27a487","Ref":"refs/tags/sdk/v1.30.0"}}),
            ),
        ]);
        let b = test_budget();
        let client = RegistryClient::uncached(&http, &b);
        let query = artifact_query(
            json!({"ecosystem":"go","packageName":"go.opentelemetry.io/otel/sdk","version":"v1.30.0"}),
            None,
        );
        let page = go(&query, &ArtifactProviderState::default(), &client)
            .await
            .expect("vanity");
        let item = &page.artifacts[0];
        assert_eq!(
            item.source_ref.as_deref(),
            Some("ed4fc757583a88b4da51b1fe1c3f0703ac27a487")
        );
        assert_eq!(item.repository_directory.as_deref(), Some("sdk"));
        assert_eq!(
            go_proxy_escape("github.com/BurntSushi/toml"),
            "github.com/!burnt!sushi/toml"
        );
    }

    /// Go pages by opaque token: each served page remembers the token that
    /// starts the next, so following `nextPage` reads one page (not every
    /// page before it), and a deeper jump walks from the deepest page known.
    #[tokio::test]
    async fn go_next_pages_start_from_the_remembered_token() {
        let item = |n: usize| json!({"packagePath": format!("example.com/p{n}")});
        let search = |token: Option<&str>| match token {
            None => "/v1/search?q=json&limit=2".to_owned(),
            Some(token) => format!("/v1/search?q=json&limit=2&token={token}"),
        };
        let http = RouteHttp::new()
            .json(
                &search(None),
                json!({"items": [item(1), item(2)], "nextPageToken": "t2", "total": 9}),
            )
            .json(
                &search(Some("t2")),
                json!({"items": [item(3), item(4)], "nextPageToken": "t3", "total": 9}),
            )
            .json(
                &search(Some("t3")),
                json!({"items": [item(5), item(6)], "nextPageToken": "t4", "total": 9}),
            )
            .json(
                &search(Some("t4")),
                json!({"items": [item(7), item(8)], "total": 9}),
            );
        let cache = super::super::ArtifactCache::new(None);
        let b = test_budget();
        let client = RegistryClient {
            cache: Some(&cache),
            ..RegistryClient::uncached(&http, &b)
        };
        let page_of = |page: u64| {
            (
                discovery(ArtifactType::Go, json!({"page": page, "pageSize": 2})),
                ArtifactProviderState {
                    page: Some(page),
                    ..Default::default()
                },
            )
        };
        let (query, state) = page_of(1);
        let first = go(&query, &state, &client).await.expect("page 1");
        let cursor = first.next_state.and_then(|next| next.token);
        assert_eq!(
            cursor
                .as_deref()
                .and_then(|cursor| go_token(&go_page_key(&query, 2), cursor)),
            Some("t2")
        );
        let seen = http.targets().len();
        let (query, state) = page_of(2);
        let second = go(&query, &state, &client).await.expect("page 2");
        assert_eq!(second.artifacts[0].name, "example.com/p3");
        assert_eq!(http.targets()[seen..], [search(Some("t2"))], "one request");
        // Page 4 walks from page 3, the deepest page whose start is known.
        let seen = http.targets().len();
        let (query, state) = page_of(4);
        let fourth = go(&query, &state, &client).await.expect("page 4");
        assert_eq!(fourth.artifacts[0].name, "example.com/p7");
        assert_eq!(fourth.next_state, None);
        assert_eq!(
            http.targets()[seen..],
            [search(Some("t3")), search(Some("t4"))]
        );
    }

    /// pkg.go.dev search pages of two rows (`p1`…`p8`), tokens `t2`…`t4`.
    fn go_pages() -> RouteHttp {
        let item = |n: usize| json!({"packagePath": format!("example.com/p{n}")});
        let search = |token: &str| format!("/v1/search?q=json&limit=2&token={token}");
        RouteHttp::new()
            .json(
                "/v1/search?q=json&limit=2",
                json!({"items": [item(1), item(2)], "nextPageToken": "t2", "total": 8}),
            )
            .json(
                &search("t2"),
                json!({"items": [item(3), item(4)], "nextPageToken": "t3", "total": 8}),
            )
            .json(
                &search("t3"),
                json!({"items": [item(5), item(6)], "nextPageToken": "t4", "total": 8}),
            )
            .json(
                &search("t4"),
                json!({"items": [item(7), item(8)], "total": 8}),
            )
    }

    fn go_page(page: u64, cursor: Option<String>) -> (ArtifactSearchQuery, ArtifactProviderState) {
        (
            discovery(ArtifactType::Go, json!({"page": page, "pageSize": 2})),
            ArtifactProviderState {
                page: Some(page),
                token: cursor,
                ..Default::default()
            },
        )
    }

    /// The cursor a page hands its next page is all that page needs: with no
    /// registry cache (memory mode, a new daemon, a gap past the cache's
    /// lifetime), page 4 is one request, not a walk from page 1.
    #[tokio::test]
    async fn a_go_cursor_serves_its_page_in_one_request_without_a_cache() {
        let http = go_pages();
        let b = test_budget();
        let client = RegistryClient::uncached(&http, &b);
        let (query, state) = go_page(3, None);
        let third = go(&query, &state, &client).await.expect("page 3 walks");
        assert_eq!(third.artifacts[0].name, "example.com/p5");
        let cursor = third.next_state.and_then(|next| next.token);
        assert!(
            cursor.as_deref().is_some_and(|cursor| cursor != "t4"),
            "{cursor:?}"
        );
        let seen = http.targets().len();
        let (query, state) = go_page(4, cursor);
        let fourth = go(&query, &state, &client).await.expect("page 4");
        assert_eq!(fourth.artifacts[0].name, "example.com/p7");
        assert_eq!(
            http.targets()[seen..],
            ["/v1/search?q=json&limit=2&token=t4"],
            "one request"
        );
        assert_eq!(fourth.next_state, None);
    }

    /// A cursor that is not its page's, or one the registry rejects, is no
    /// shortcut: the page walks and serves exactly the rows it owns.
    #[tokio::test]
    async fn a_foreign_or_rejected_go_cursor_walks_to_the_same_rows() {
        let other = discovery(
            ArtifactType::Go,
            json!({"keywords": ["yaml"], "pageSize": 2}),
        );
        for cursor in [
            // Bound to another query's page 3.
            go_cursor(&go_page_key(&other, 3), "t2"),
            // Bound to page 2 of this query, copied onto page 3.
            go_cursor(&go_page_key(&go_page(2, None).0, 2), "t2"),
            // This page's, but the registry no longer honors it (404).
            go_cursor(&go_page_key(&go_page(3, None).0, 3), "expired"),
            "not-a-cursor".to_owned(),
        ] {
            let http = go_pages();
            let b = test_budget();
            let client = RegistryClient::uncached(&http, &b);
            let (query, state) = go_page(3, Some(cursor.clone()));
            let third = go(&query, &state, &client).await.expect("page 3");
            let names = third
                .artifacts
                .iter()
                .map(|row| row.name.as_str())
                .collect::<Vec<_>>();
            assert_eq!(names, ["example.com/p5", "example.com/p6"], "{cursor}");
            assert!(
                http.targets()
                    .iter()
                    .any(|target| target == "/v1/search?q=json&limit=2&token=t2"),
                "walked: {cursor}"
            );
        }
    }
}
