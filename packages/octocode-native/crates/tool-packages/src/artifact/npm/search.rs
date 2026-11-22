//! npm keyword search: one window per page, checked against the window
//! ending just before it, since npm answers a `from` past its reach with
//! earlier rows. Several keywords are ANDed here (npm matches any one), so
//! such a page reads npm's widest window and keeps its matching rows, and
//! reads on while a window holds none: a page is empty only where the
//! search ends.
use super::registry::trim_registry;
use crate::artifact::http::{DnsPin, JSON, RegistryClient};
use crate::artifact::query::artifact_search_query;
use crate::artifact::util::{
    encode_component, endpoint, invalid, object_for, required, safe_url, string, total,
};
use crate::artifact::{
    ArtifactError, ArtifactItem, ArtifactProviderPage, ArtifactProviderState, ResolvedNpmRegistry,
    TerminalLimit,
};
use octocode_contract_types::{ArtifactSearchQuery, ArtifactType, ErrorCode};
use serde_json::Value;

/// The widest window npm's search serves in one request (its documented
/// `size` maximum: <https://github.com/npm/registry/blob/main/docs/REGISTRY-API.md#get-v1search>).
const NPM_SEARCH_WINDOW: usize = 250;

pub(super) async fn search(
    query: &ArtifactSearchQuery,
    state: &ArtifactProviderState,
    registry: &ResolvedNpmRegistry,
    client: &RegistryClient<'_>,
    dns_pin: Option<DnsPin>,
) -> Result<ArtifactProviderPage, ArtifactError> {
    // An ANDed walk resumes at the npm row after the last one it showed,
    // which its continuation carries (`registryCursor`); a page asked by
    // number starts where pages of `limit` rows tile.
    let mut offset = state
        .token
        .as_deref()
        .and_then(|token| token.parse::<u64>().ok())
        .or(state.offset)
        .unwrap_or(0);
    let size = artifact_search_query::page_size(query);
    let terms = artifact_search_query::keyword_list(query);
    let anded = terms.len() > 1;
    let window = |from: u64, size: usize| {
        search_window(query, registry, client, dns_pin.clone(), from, size)
    };
    loop {
        // npm's own rows match any one keyword, so an ANDed page reads the
        // widest window and fills from its matching rows: a walk to the end
        // costs one request per npm window, not one per `limit` rows.
        let (total_found, objects) =
            window(offset, if anded { NPM_SEARCH_WINDOW } else { size }).await?;
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
                    // npm's total counts one-term rows: not this search's
                    // count when its keywords are ANDed.
                    total: (!anded).then_some(total_found),
                    terminal_limit: Some(unreachable_rows(offset, total_found)),
                    registry: Some(trim_registry(&registry.base)),
                });
            }
        }
        let mut artifacts = Vec::with_capacity(objects.len().min(size));
        let mut matched = Vec::with_capacity(objects.len().min(size));
        // The npm rows this page consumed: an ANDed page stops right after the
        // row that fills it, so the next page starts at the row after it and no
        // matching row is skipped.
        let mut read = 0_u64;
        for entry in &objects {
            if artifacts.len() == size {
                break;
            }
            read += 1;
            let count = terms_matched(&terms, entry.get("package"));
            if anded && count < terms.len() {
                continue;
            }
            matched.push(count);
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
            artifact.downloads_monthly =
                entry.pointer("/downloads/monthly").and_then(Value::as_u64);
            artifact.description = string(package.get("description"));
            artifact.license = string(package.get("license"));
            if let Some(Value::Object(links)) = package.get("links") {
                artifact.homepage = safe_url(links.get("homepage"));
                if let Some(link) = links.get("repository").and_then(Value::as_str) {
                    artifact.set_source_link(link);
                }
            }
            artifacts.push(artifact);
        }
        rank_by_terms(&mut artifacts, &matched);
        // Keywords are ANDed, as on every other search tool, and npm alone returns
        // rows matching one term of several: only rows holding every term are
        // kept. Paging stays on npm's raw rows, and npm's total, which counts
        // one-term rows, is not the count of these.
        let has_more = offset.saturating_add(read) < total_found;
        // A window with no row holding every term is no page of this search:
        // the page reads on, so it is empty only where npm's rows end (their
        // total, or the reach the check above finds).
        if artifacts.is_empty() && has_more && read > 0 {
            offset += read;
            continue;
        }
        let terminal_limit = (has_more && read == 0).then(|| unreachable_rows(offset, total_found));
        let next = offset + read;
        return Ok(ArtifactProviderPage {
            next_state: (has_more && read > 0).then(|| ArtifactProviderState {
                offset: Some(next),
                token: anded.then(|| next.to_string()),
                ..Default::default()
            }),
            artifacts,
            total: (!anded).then_some(total_found),
            terminal_limit,
            registry: Some(trim_registry(&registry.base)),
        });
    }
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
    let fields = text
        .iter()
        .map(|field| format!(" {} ", fold(field)))
        .collect::<Vec<_>>();
    terms
        .iter()
        .map(|term| fold(term))
        .filter(|term| !term.is_empty())
        .filter(|term| {
            let phrase = format!(" {term} ");
            fields.iter().any(|field| field.contains(&phrase))
        })
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
mod tests;
