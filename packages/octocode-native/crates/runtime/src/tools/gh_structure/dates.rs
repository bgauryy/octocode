//! Freshness of a tree page: each listed entry's last-commit date at the
//! listed commit, and that commit's own date, as `YYYY-MM-DD`.
use super::*;
use std::collections::HashMap;

/// Dates of one listing page.
#[derive(Default, serde::Serialize, serde::Deserialize)]
pub(super) struct PageDates {
    /// The listed commit's date.
    pub(super) commit: Option<String>,
    /// Entry path (repository-relative) → its last-commit date.
    pub(super) entries: HashMap<String, String>,
    /// Entries left undated, stated as one row warning.
    #[serde(skip)]
    pub(super) warning: Option<String>,
    /// Page entries past the first [`DATED_PER_PAGE`], left to
    /// `next.expandDates`.
    #[serde(skip)]
    pub(super) undated: usize,
}

/// Entries one page dates inline: one GraphQL request's aliases. Spike
/// 2026-10-06: 1×100 aliases ≈ 3.4 s, 1×300 ≈ 7.6 s (near the 10 s
/// GraphQL timeout), so a larger page leads the rest via `expandDates`.
pub(super) const DATED_PER_PAGE: usize = octocode_github::MAX_PATHS_PER_REQUEST;

/// A day is enough: entries of one page are compared by day, and the
/// listed commit's day bounds them all. Full timestamps would add 10 bytes
/// per entry for an ordering no listing reader asked for.
fn day(timestamp: &str) -> Option<String> {
    timestamp.get(..10).map(str::to_owned)
}

/// The page's dates: its first [`DATED_PER_PAGE`] entries, one cached
/// record per (owner, repo, commit, those paths), else one GraphQL request.
/// A failure dates what was answered and names the rest in
/// [`PageDates::warning`]; entries past the first chunk are counted in
/// [`PageDates::undated`].
pub(super) async fn page_dates<C: crate::providers::github::ConditionalCache>(
    provider: &GitHubProvider<C>,
    owner: &str,
    repo: &str,
    commit_sha: &str,
    entries: &[TreeEntry],
    context: &RequestContext,
) -> PageDates {
    if entries.is_empty() {
        return PageDates::default();
    }
    let undated = entries.len().saturating_sub(DATED_PER_PAGE);
    let paths = entries[..entries.len() - undated]
        .iter()
        .map(|entry| entry.path.as_str())
        .collect::<Vec<_>>();
    let key = cache_key(owner, repo, commit_sha, &paths);
    let partition = provider.transport.cache_partition(context, None).ok();
    if let Some(partition) = &partition
        && let Some(cached) = provider.cache.get(partition, &key).await
        && let Ok(mut dates) = serde_json::from_slice::<PageDates>(&cached.bytes)
    {
        dates.undated = undated;
        return dates;
    }
    let answer = provider
        .transport
        .path_commit_dates(owner, repo, commit_sha, &paths, context)
        .await;
    let mut dates = PageDates {
        commit: answer.commit_date.as_deref().and_then(day),
        entries: paths
            .iter()
            .zip(&answer.dates)
            .filter_map(|(path, date)| Some(((*path).to_owned(), day(date.as_deref()?)?)))
            .collect(),
        warning: None,
        undated,
    };
    match answer.error {
        Some(error) => {
            let missing = paths.len() - dates.entries.len();
            dates.warning = Some(format!(
                "Last-commit dates (updated) are missing for {missing} {}: {}. Rerun the same query to retry.",
                if missing == 1 { "entry" } else { "entries" },
                error.message
            ));
        }
        None => {
            // A complete answer at a commit SHA never changes.
            if let Some(partition) = &partition
                && let Ok(bytes) = serde_json::to_vec(&dates)
            {
                provider
                    .cache
                    .put(
                        partition,
                        key,
                        crate::providers::github::CachedContent {
                            etag: None,
                            bytes: bytes.into(),
                            resolved_ref: commit_sha.to_owned(),
                        },
                    )
                    .await;
            }
        }
    }
    dates
}

/// `next.expandDates` for a page whose entries past the first
/// [`DATED_PER_PAGE`] are undated: the same listing pinned to its SHA at
/// `pageSize:` [`DATED_PER_PAGE`], one row per page that holds an undated
/// entry (overlap with dated entries is allowed, a gap never), plus one
/// warning naming the count. Each row dates its whole page inline.
pub(super) fn expand(
    value: &mut Value,
    pinned: &GhStructureQuery,
    page: &super::listing::ListingPage<'_>,
    undated: usize,
) -> Result<(), ProviderError> {
    if undated == 0 {
        return Ok(());
    }
    let start = page.current.saturating_sub(1) * page.per_page;
    let first_undated = start + page.entries.len() - undated;
    let end = start + page.entries.len();
    let pages = (first_undated / DATED_PER_PAGE + 1)..=end.div_ceil(DATED_PER_PAGE);
    let mut rows = Vec::new();
    for number in pages {
        let mut row = super::listing::public_query(pinned)?;
        row["page"] = json!(number);
        row["pageSize"] = json!(DATED_PER_PAGE);
        rows.push(row);
    }
    let warning = json!(format!(
        "Dates cover the first {DATED_PER_PAGE} entries; {undated} more: next.expandDates."
    ));
    match value.get_mut("warnings").and_then(Value::as_array_mut) {
        Some(warnings) => warnings.push(warning),
        None => value["warnings"] = json!([warning]),
    }
    value["next"]["expandDates"] =
        crate::tools::result::Continuation::input(ToolId::GhStructure, json!({ "queries": rows }))
            .why("Date the rest of this page's entries.")
            .confidence("exact")
            .build();
    Ok(())
}

fn cache_key(owner: &str, repo: &str, commit_sha: &str, paths: &[&str]) -> String {
    use sha2::{Digest, Sha256};
    let mut digest = Sha256::new();
    for value in [
        owner.to_ascii_lowercase().as_str(),
        repo.to_ascii_lowercase().as_str(),
        commit_sha,
    ]
    .into_iter()
    .chain(paths.iter().copied())
    {
        digest.update(value.as_bytes());
        digest.update([0]);
    }
    format!("github-tree-dates:{}", hex::encode(digest.finalize()))
}

/// State the page's dates on its rows: each row's `updated` maps a listed
/// name to its date; the listed commit's date is `commitDate`. Row `dir`s
/// are repo-relative, like the dated paths.
pub(super) fn attach(value: &mut Value, dates: PageDates) {
    if let Some(commit) = dates.commit {
        value["commitDate"] = json!(commit);
    }
    if let Some(warning) = dates.warning {
        match value.get_mut("warnings").and_then(Value::as_array_mut) {
            Some(warnings) => warnings.push(json!(warning)),
            None => value["warnings"] = json!([warning]),
        }
    }
    if dates.entries.is_empty() {
        return;
    }
    let Some(rows) = value.get_mut("entries").and_then(Value::as_array_mut) else {
        return;
    };
    for row in rows {
        let dir = row["dir"].as_str().unwrap_or(".").to_owned();
        let mut updated = Map::new();
        for key in ["files", "folders"] {
            for name in row[key].as_array().into_iter().flatten() {
                let Some(name) = name.as_str() else { continue };
                let full = if dir == "." {
                    name.to_owned()
                } else {
                    format!("{dir}/{name}")
                };
                if let Some(date) = dates.entries.get(&full) {
                    updated.insert(name.to_owned(), json!(date));
                }
            }
        }
        if !updated.is_empty() {
            row["updated"] = Value::Object(updated);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_map_each_listed_name_to_its_repo_relative_date() {
        let mut value = json!({"entries": [
            {"dir": "src", "files": ["a.rs"], "folders": ["sub"]},
            {"dir": "src/sub", "files": ["b.rs"]}
        ]});
        let dates = PageDates {
            commit: Some("2026-02-03".into()),
            entries: [
                ("src/a.rs", "2025-01-01"),
                ("src/sub", "2025-02-02"),
                ("src/sub/b.rs", "2025-03-03"),
            ]
            .into_iter()
            .map(|(path, date)| (path.to_owned(), date.to_owned()))
            .collect(),
            warning: None,
            undated: 0,
        };
        attach(&mut value, dates);
        assert_eq!(value["commitDate"], "2026-02-03");
        assert_eq!(
            value["entries"][0]["updated"],
            json!({"a.rs": "2025-01-01", "sub": "2025-02-02"})
        );
        assert_eq!(
            value["entries"][1]["updated"],
            json!({"b.rs": "2025-03-03"})
        );
        assert!(value.get("warnings").is_none());
    }

    /// B7: each name is written once: the day joins the size inside the
    /// entry's fields, and the row keeps no `updated` map.
    #[test]
    fn dates_fold_into_the_entries_after_their_size() {
        let mut value = json!({"entries": [
            {"dir": "src", "files": ["a.rs", "b (1).rs"], "folders": ["sub", "old"]}
        ]});
        let dates = PageDates {
            entries: [("src/a.rs", "2025-01-01"), ("src/sub", "2025-02-02")]
                .into_iter()
                .map(|(path, date)| (path.to_owned(), date.to_owned()))
                .collect(),
            ..PageDates::default()
        };
        attach(&mut value, dates);
        let entries = [
            ("src/a.rs", super::super::EntryKind::File, Some(12)),
            ("src/b (1).rs", super::super::EntryKind::File, Some(3)),
        ]
        .map(|(path, kind, size)| super::super::TreeEntry {
            path: path.to_owned(),
            kind,
            size,
        });
        super::super::listing::attach_fields(&mut value, &entries);
        assert_eq!(
            value["entries"][0],
            json!({"dir": "src", "files": ["a.rs (12, 2025-01-01)", "b (1).rs (3)"],
                "folders": ["sub (2025-02-02)", "old"]})
        );
    }

    #[test]
    fn a_warning_joins_existing_row_warnings() {
        let mut value = json!({"entries": [], "warnings": ["first"]});
        let dates = PageDates {
            warning: Some("dates missing".into()),
            ..PageDates::default()
        };
        attach(&mut value, dates);
        assert_eq!(value["warnings"], json!(["first", "dates missing"]));
    }

    #[test]
    fn a_timestamp_shortens_to_its_day() {
        assert_eq!(day("2026-01-31T23:59:59Z").as_deref(), Some("2026-01-31"));
        assert_eq!(day("bad"), None);
    }
}
