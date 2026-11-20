//! Names one edit away from a package name that its registry has, with the
//! downloads the registry counts for each: the evidence the `lookalikeName`
//! caution, an exact row's lookalike warning and a missing name's repair
//! stand on. One edit is a character dropped, two adjacent characters
//! swapped, a separator added, dropped or changed, or the name's capitals
//! lowercased (npm takes only lowercase for a new name, so `React` most
//! likely means `react`). Only registries that
//! count many names in one request are asked: npm's downloads API for
//! unscoped names on the public registry (last-week downloads) and crates.io
//! (90-day downloads). Any other registry, a scoped npm name and a
//! configured npm registry are not checked, and say so by `None`.
use super::http::{JSON, RegistryClient};
use super::util::{encode_component, endpoint, object_for, parse_url};
use super::{ArtifactError, ArtifactProviderContext, ResolvedNpmRegistry};
use futures_util::future::try_join_all;
use octocode_contract_types::ArtifactType;
use serde_json::Value;
use std::collections::BTreeMap;

/// npm's bulk downloads API answers at most this many names in one request
/// (<https://github.com/npm/registry/blob/main/docs/download-counts.md>).
const NPM_BULK_NAMES: usize = 128;
/// crates.io lists at most this many crates on one page.
const CRATES_PAGE: usize = 100;

/// What the registry counts of a name and of the names one edit away.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Near {
    /// The name's own count; `None`: the registry has no such name.
    pub own: Option<u64>,
    /// Each name one edit away the registry has, most downloaded first.
    pub names: Vec<(String, u64)>,
    /// What the counts count.
    pub measure: &'static str,
}

impl Near {
    /// The most downloaded name one edit away when it has at least `ratio`
    /// times the name's own downloads, counted as one download at least, so
    /// a name with none needs a lookalike of `ratio` downloads.
    pub fn lookalike(&self, ratio: u64) -> Option<&(String, u64)> {
        let floor = ratio.saturating_mul(self.own.unwrap_or(0).max(1));
        self.names
            .first()
            .filter(|(_, downloads)| *downloads >= floor)
    }
}

/// What `kind`'s registry counts of `name` and the names one edit away;
/// `None` when that registry is not checked (module doc).
pub async fn near_names(
    kind: ArtifactType,
    name: &str,
    context: &ArtifactProviderContext<'_>,
) -> Option<Result<Near, ArtifactError>> {
    let client = RegistryClient {
        http: context.http,
        budget: context.budget,
        tags: None,
        answers: context.answers,
        reused: context.reused,
        share: context.share,
    };
    match kind {
        ArtifactType::Npm => {
            let public = context
                .npm_registry
                .is_none_or(|registry| registry.base == ResolvedNpmRegistry::npmjs().base);
            if !public || name.starts_with('@') {
                return None;
            }
            let candidates = edits(name, &['-', '_', '.'], str::to_owned);
            Some(
                npm_counts(name, &candidates, &client)
                    .await
                    .map(|counts| near(name, counts, "last-week downloads")),
            )
        }
        ArtifactType::Crates => {
            // crates.io takes `-` and `_` as one: a name that differs only
            // there is the same crate.
            let candidates = edits(name, &['-', '_'], crate_identity);
            Some(
                crates_counts(name, &candidates, &client)
                    .await
                    .map(|counts| near(name, counts, "90-day downloads")),
            )
        }
        _ => None,
    }
}

/// The name a crate is known by: crates.io matches names case-insensitively
/// and with `-` and `_` alike.
fn crate_identity(name: &str) -> String {
    name.to_ascii_lowercase().replace('_', "-")
}

/// `counts` of the name and its edits as a [`Near`].
fn near(name: &str, mut counts: BTreeMap<String, u64>, measure: &'static str) -> Near {
    let own = counts.remove(name);
    let mut names = counts.into_iter().collect::<Vec<_>>();
    names.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    Near {
        own,
        names,
        measure,
    }
}

/// Every name one edit away from `name` with `separators` as its separators,
/// each once, none the same package as `name` (`identity`: a registry that
/// matches names whatever their case gets no lowercased edit), each starting
/// and ending with a letter or digit.
pub(crate) fn edits(name: &str, separators: &[char], identity: fn(&str) -> String) -> Vec<String> {
    let chars = name.chars().collect::<Vec<_>>();
    let own = identity(name);
    let mut out: Vec<String> = Vec::new();
    let mut push = |candidate: Vec<char>| {
        let text = candidate.iter().collect::<String>();
        let bounded = candidate
            .first()
            .zip(candidate.last())
            .is_some_and(|(first, last)| {
                first.is_ascii_alphanumeric() && last.is_ascii_alphanumeric()
            });
        if bounded && identity(&text) != own && !out.contains(&text) {
            out.push(text);
        }
    };
    push(name.to_lowercase().chars().collect());
    for at in 0..chars.len() {
        let mut dropped = chars.clone();
        dropped.remove(at);
        push(dropped);
    }
    for at in 1..chars.len() {
        if chars[at - 1] != chars[at] {
            let mut swapped = chars.clone();
            swapped.swap(at - 1, at);
            push(swapped);
        }
    }
    for at in 1..chars.len() {
        if separators.contains(&chars[at - 1]) || separators.contains(&chars[at]) {
            continue;
        }
        for separator in separators {
            let mut added = chars.clone();
            added.insert(at, *separator);
            push(added);
        }
    }
    for (at, current) in chars.iter().enumerate() {
        if !separators.contains(current) {
            continue;
        }
        for separator in separators.iter().filter(|separator| *separator != current) {
            let mut changed = chars.clone();
            changed[at] = *separator;
            push(changed);
        }
    }
    out
}

/// npm's last-week downloads of `name` and each of `candidates` it has, in
/// requests of [`NPM_BULK_NAMES`] names read together. A request names two
/// names at least, so its answer is the map of names (one name alone is
/// answered in another shape, and with 404 when npm lacks it).
async fn npm_counts(
    name: &str,
    candidates: &[String],
    client: &RegistryClient<'_>,
) -> Result<BTreeMap<String, u64>, ArtifactError> {
    let names = std::iter::once(name.to_owned())
        .chain(candidates.iter().cloned())
        .collect::<Vec<_>>();
    if names.len() < 2 {
        return Ok(BTreeMap::new());
    }
    let mut chunks = names
        .chunks(NPM_BULK_NAMES)
        .map(<[String]>::to_vec)
        .collect::<Vec<_>>();
    if let [.., before, last] = chunks.as_mut_slice()
        && last.len() == 1
        && let Some(moved) = before.pop()
    {
        last.insert(0, moved);
    }
    let reads = chunks.iter().map(|chunk| async move {
        let joined = chunk
            .iter()
            .map(|name| encode_component(name))
            .collect::<Vec<_>>()
            .join(",");
        let url = parse_url(&format!(
            "https://api.npmjs.org/downloads/point/last-week/{joined}"
        ))?;
        let answer = client
            .json(ArtifactType::Npm, url, true, None, None, JSON)
            .await?;
        let mut counts = BTreeMap::new();
        if let Some(answer) = answer {
            for (listed, entry) in object_for(&answer, ArtifactType::Npm)? {
                if let Some(downloads) = entry.get("downloads").and_then(Value::as_u64) {
                    counts.insert(listed.clone(), downloads);
                }
            }
        }
        Ok::<_, ArtifactError>(counts)
    });
    Ok(try_join_all(reads).await?.into_iter().flatten().collect())
}

/// crates.io's 90-day downloads of `name` and each of `candidates` it has,
/// in pages of [`CRATES_PAGE`] names read together.
async fn crates_counts(
    name: &str,
    candidates: &[String],
    client: &RegistryClient<'_>,
) -> Result<BTreeMap<String, u64>, ArtifactError> {
    let names = std::iter::once(name.to_owned())
        .chain(candidates.iter().cloned())
        .collect::<Vec<_>>();
    let reads = names.chunks(CRATES_PAGE).map(|chunk| async move {
        let mut params = vec![("per_page", Some(CRATES_PAGE.to_string()))];
        params.extend(chunk.iter().map(|name| ("ids[]", Some(name.clone()))));
        let url = endpoint("https://crates.io/api/v1/crates", &params)?;
        let answer = client
            .json(ArtifactType::Crates, url, true, None, None, JSON)
            .await?;
        let mut counts = BTreeMap::new();
        let listed = answer
            .as_ref()
            .and_then(|answer| answer.get("crates"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten();
        for row in listed {
            let found = row
                .get("name")
                .or_else(|| row.get("id"))
                .and_then(Value::as_str);
            // The name the request spelled: crates.io answers by identity.
            let asked = found.and_then(|found| {
                chunk
                    .iter()
                    .find(|asked| crate_identity(asked) == crate_identity(found))
            });
            if let (Some(asked), Some(downloads)) =
                (asked, row.get("recent_downloads").and_then(Value::as_u64))
            {
                counts.insert(asked.clone(), downloads);
            }
        }
        Ok::<_, ArtifactError>(counts)
    });
    Ok(try_join_all(reads).await?.into_iter().flatten().collect())
}

#[cfg(test)]
mod tests;
