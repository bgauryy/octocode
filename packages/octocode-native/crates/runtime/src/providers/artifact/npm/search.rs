//! npm keyword search: one window per page, checked against the window
//! ending just before it, since npm answers a `from` past its reach with
//! earlier rows.
use super::registry::trim_registry;
use crate::providers::artifact::http::{DnsPin, JSON, RegistryClient};
use crate::providers::artifact::query::artifact_search_query;
use crate::providers::artifact::util::{
    canonical_repository, encode_component, endpoint, invalid, object_for, required, safe_url,
    string, total,
};
use crate::providers::artifact::{
    ArtifactError, ArtifactItem, ArtifactProviderPage, ArtifactProviderState, ResolvedNpmRegistry,
    TerminalLimit,
};
use octocode_contract_types::{ArtifactSearchQuery, ArtifactType, ErrorCode};
use serde_json::Value;

pub(super) async fn search(
    query: &ArtifactSearchQuery,
    state: &ArtifactProviderState,
    registry: &ResolvedNpmRegistry,
    client: &RegistryClient<'_>,
    dns_pin: Option<DnsPin>,
) -> Result<ArtifactProviderPage, ArtifactError> {
    let offset = state.offset.unwrap_or(0);
    let size = artifact_search_query::page_size(query);
    let window = |from: u64, size: usize| {
        search_window(query, registry, client, dns_pin.clone(), from, size)
    };
    let (total_found, objects) = window(offset, size).await?;
    // npm serves a deep window from an undocumented reach on and answers a
    // `from` past it with earlier rows instead of an error. The page is its
    // own rows only when the two rows ending just before it are the row
    // before its first and its first: a restarted or clamped window fails
    // that either way.
    if offset > 0
        && let Some(first) = objects.first()
    {
        let (_, probe) = window(offset - 1, 2).await?;
        let name = |entry: Option<&Value>| {
            entry
                .and_then(|entry| entry.pointer("/package/name"))
                .cloned()
        };
        let first = name(Some(first));
        if name(probe.first()) == first || name(probe.get(1)) != first {
            return Ok(ArtifactProviderPage {
                artifacts: Vec::new(),
                next_state: None,
                total: Some(total_found),
                terminal_limit: Some(unreachable_rows(offset, total_found)),
                registry: Some(trim_registry(&registry.base)),
            });
        }
    }
    let mut artifacts = Vec::with_capacity(objects.len().min(size));
    let terms = artifact_search_query::keyword_list(query);
    let mut matched = Vec::with_capacity(objects.len().min(size));
    for entry in objects.iter().take(size) {
        matched.push(terms_matched(&terms, entry.get("package")));
        let package = object_for(
            object_for(entry, ArtifactType::Npm)?
                .get("package")
                .ok_or_else(|| invalid(ArtifactType::Npm))?,
            ArtifactType::Npm,
        )?;
        let name = required(package.get("name"), ArtifactType::Npm).map_err(|_| {
            ArtifactError::new(
                ErrorCode::ProviderError,
                "npm registry search returned an unnamed package; refusing to skip a result.",
            )
        })?;
        let mut artifact = ArtifactItem::new(
            ArtifactType::Npm,
            name.clone(),
            format!(
                "{}/{}",
                trim_registry(&registry.base),
                encode_component(&name)
            ),
        );
        artifact.version = string(package.get("version")).filter(|value| value != "unknown");
        artifact.downloads_monthly = entry.pointer("/downloads/monthly").and_then(Value::as_u64);
        artifact.description = string(package.get("description"));
        artifact.license = string(package.get("license"));
        if let Some(Value::Object(links)) = package.get("links") {
            artifact.homepage = safe_url(links.get("homepage"));
            artifact.repository = links
                .get("repository")
                .and_then(Value::as_str)
                .and_then(canonical_repository);
        }
        artifacts.push(artifact);
    }
    rank_by_terms(&mut artifacts, &matched);
    let has_more = offset.saturating_add(artifacts.len() as u64) < total_found;
    let terminal_limit =
        (has_more && artifacts.is_empty()).then(|| unreachable_rows(offset, total_found));
    Ok(ArtifactProviderPage {
        next_state: (has_more && !artifacts.is_empty()).then(|| ArtifactProviderState {
            offset: Some(offset + artifacts.len() as u64),
            ..Default::default()
        }),
        artifacts,
        total: Some(total_found),
        terminal_limit,
        registry: Some(trim_registry(&registry.base)),
    })
}

/// How many of `terms` a package's name, description or keywords spell
/// (case and `-`/`_`/`.`/`/`/`@` separators aside): npm ranks by its own
/// score, which puts a package matching one term of several above one
/// matching them all.
fn terms_matched(terms: &[String], package: Option<&Value>) -> usize {
    let fold = |text: &str| {
        text.to_lowercase()
            .split(|c: char| c.is_whitespace() || matches!(c, '-' | '_' | '.' | '/' | '@'))
            .filter(|word| !word.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    };
    let Some(package) = package else {
        return 0;
    };
    let mut text = vec![
        string(package.get("name")).unwrap_or_default(),
        string(package.get("description")).unwrap_or_default(),
    ];
    text.extend(
        package
            .get("keywords")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned),
    );
    let haystack = format!(" {} ", fold(&text.join(" ")));
    terms
        .iter()
        .map(|term| fold(term))
        .filter(|term| !term.is_empty() && haystack.contains(&format!(" {term} ")))
        .count()
}

/// A page's rows, most terms matched first; npm's order breaks ties. Only
/// this page's rows move: pages keep the rows npm put on them.
fn rank_by_terms(artifacts: &mut Vec<ArtifactItem>, matched: &[usize]) {
    let mut ranked = std::mem::take(artifacts)
        .into_iter()
        .zip(matched.iter().copied())
        .collect::<Vec<_>>();
    ranked.sort_by_key(|(_, matched)| std::cmp::Reverse(*matched));
    artifacts.extend(ranked.into_iter().map(|(artifact, _)| artifact));
}

/// One npm search window: the registry's total and its result objects.
async fn search_window(
    query: &ArtifactSearchQuery,
    registry: &ResolvedNpmRegistry,
    client: &RegistryClient<'_>,
    dns_pin: Option<DnsPin>,
    from: u64,
    size: usize,
) -> Result<(u64, Vec<Value>), ArtifactError> {
    let url = endpoint(
        &format!("{}/-/v1/search", trim_registry(&registry.base)),
        &[
            ("text", Some(artifact_search_query::terms(query))),
            ("size", Some(size.to_string())),
            ("from", (from > 0).then(|| from.to_string())),
        ],
    )?;
    let authorization = registry.authorization_for(&url);
    let mut response = client
        .json(ArtifactType::Npm, url, false, authorization, dns_pin, JSON)
        .await?
        .ok_or_else(|| {
            ArtifactError::new(ErrorCode::ProviderError, "npm registry search failed.")
        })?;
    let data = object_for(&response, ArtifactType::Npm)?;
    let total_found = total(data.get("total")).ok_or_else(|| {
        ArtifactError::new(
            ErrorCode::ProviderError,
            "npm registry search omitted a valid total; pagination cannot be determined.",
        )
    })?;
    match response.get_mut("objects").map(Value::take) {
        Some(Value::Array(objects)) => Ok((total_found, objects)),
        _ => Err(ArtifactError::new(
            ErrorCode::ProviderError,
            "Invalid npm registry search response; expected an object with results and a total.",
        )),
    }
}

/// The terminal limit of a page npm does not serve: the rows before it are
/// the most this search reaches.
fn unreachable_rows(offset: u64, total: u64) -> TerminalLimit {
    TerminalLimit::result_cap(format!(
        "npm serves no rows past the first {offset} for this search: at most {offset} of {total} are reachable."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn item(name: &str) -> ArtifactItem {
        ArtifactItem::new(ArtifactType::Npm, name.to_owned(), String::new())
    }

    /// The live `["mcp","code search"]` page: npm put a Notion server that
    /// matches only `mcp` above packages that spell both terms.
    #[test]
    fn a_page_lists_packages_matching_more_terms_first() {
        let terms = ["mcp".to_owned(), "code search".to_owned()];
        let packages = [
            json!({"name":"@notionhq/notion-mcp-server","description":"Official MCP server for Notion API","keywords":["notion","mcp"]}),
            json!({"name":"smarter-faster-better-mcp","description":"Blazing fast AST-based MCP code search"}),
            json!({"name":"codeindex-sync","description":"Index sync","keywords":["mcp","code-search"]}),
            json!({"name":"unrelated","description":"nothing here"}),
        ];
        let matched = packages
            .iter()
            .map(|package| terms_matched(&terms, Some(package)))
            .collect::<Vec<_>>();
        assert_eq!(matched, [1, 2, 2, 0]);
        let mut page = packages
            .iter()
            .map(|package| item(package["name"].as_str().expect("name")))
            .collect::<Vec<_>>();
        rank_by_terms(&mut page, &matched);
        let names = page
            .iter()
            .map(|row| row.package_name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            [
                "smarter-faster-better-mcp",
                "codeindex-sync",
                "@notionhq/notion-mcp-server",
                "unrelated"
            ]
        );
        // A term matches whole words: `mcp` is not in `mcpx`.
        assert_eq!(terms_matched(&terms, Some(&json!({"name":"mcpx"}))), 0);
    }
}
