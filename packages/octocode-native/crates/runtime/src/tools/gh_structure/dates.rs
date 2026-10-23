//! Freshness of a tree page: each listed entry's last-commit date at the
//! listed commit, and that commit's own date, as `YYYY-MM-DD`.
// A day is enough: entries of one page are compared by day, and the
// listed commit's day bounds them all. Full timestamps would add 10 bytes
// per entry for an ordering no listing reader asked for.
use super::*;
use crate::response::paging::push_warnings;
use crate::tools::gh_shared::dates::utc_day as day;
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

/// The page's dates: its first [`DATED_PER_PAGE`] entries, one cached
/// record per (owner, repo, commit, those paths), else one GraphQL request.
/// A failure dates what was answered and names the rest in
/// [`PageDates::warning`]; entries past the first chunk are counted in
/// [`PageDates::undated`].
pub(super) async fn page_dates(
    provider: &GitHubProvider,
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
        && let Some(cached) = provider.transport.cache.get(partition, &key).await
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
                "Last-commit dates are missing for {missing} {}: {}.",
                if missing == 1 { "entry" } else { "files" },
                error.message
            ));
        }
        None => {
            // A complete answer at a commit SHA never changes.
            if let Some(partition) = &partition
                && let Ok(bytes) = serde_json::to_vec(&dates)
            {
                provider
                    .transport
                    .cache
                    .put(
                        partition,
                        key,
                        octocode_github::CachedContent {
                            etag: None,
                            bytes: bytes.into(),
                            resolved_ref: commit_sha.to_owned(),
                            has_more: false,
                        },
                        octocode_github::Retention::Pinned,
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
    pinned: &GhStructureQueryTree,
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
        let mut row = query_row(pinned, &[]);
        row["page"] = json!(number);
        row["pageSize"] = json!(DATED_PER_PAGE);
        rows.push(row);
    }
    push_warnings(
        value,
        [format!(
            "Dates cover the first {DATED_PER_PAGE} entries; {undated} more are undated."
        )],
    );
    value["next"]["expandDates"] = crate::response::result::Continuation::input(
        ToolId::GhStructure,
        json!({ "queries": rows }),
    )
    .build();
    Ok(())
}

fn cache_key(owner: &str, repo: &str, commit_sha: &str, paths: &[&str]) -> String {
    let (owner, repo) = (owner.to_ascii_lowercase(), repo.to_ascii_lowercase());
    let fields = [owner.as_str(), repo.as_str(), commit_sha]
        .into_iter()
        .chain(paths.iter().copied())
        .collect::<Vec<_>>();
    octocode_github::fields_key("github-tree-dates", &fields)
}

/// State the page's commit date (`commitDate`) and its dating warning; the
/// entries' dates are already in their rows ([`super::listing::build_structure`]).
pub(super) fn attach(value: &mut Value, dates: PageDates) {
    if let Some(commit) = dates.commit {
        value["commitDate"] = json!(commit);
    }
    if let Some(warning) = dates.warning {
        push_warnings(value, [warning]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each name is written once: the day joins the size inside the
    /// entry's fields (keyed by repo-relative path), and the page states its
    /// commit's date once.
    #[test]
    fn dates_fold_into_the_entries_after_their_size() {
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
            ..PageDates::default()
        };
        let entries = [
            ("src/a.rs", super::super::EntryKind::File, Some(12)),
            ("src/b (1).rs", super::super::EntryKind::File, Some(3)),
            ("src/sub", super::super::EntryKind::Dir, None),
            ("src/old", super::super::EntryKind::Dir, None),
            ("src/sub/b.rs", super::super::EntryKind::File, None),
        ]
        .map(|(path, kind, size)| super::super::TreeEntry {
            path: path.to_owned(),
            kind,
            size,
        });
        let rows = super::super::listing::build_structure(&entries, &dates.entries);
        assert_eq!(
            rows,
            vec![
                json!({"dir": "src", "files": ["a.rs (12, 2025-01-01)", "b (1).rs (3)",
                    "old/", "sub/ (2025-02-02)"]}),
                json!({"dir": "src/sub", "files": ["b.rs (2025-03-03)"]}),
            ]
        );
        let mut value = json!({"files": rows});
        attach(&mut value, dates);
        assert_eq!(value["commitDate"], "2026-02-03");
        assert!(value.get("warnings").is_none());
    }

    #[test]
    fn a_warning_joins_existing_row_warnings() {
        let mut value = json!({"files": [], "warnings": ["first"]});
        let dates = PageDates {
            warning: Some("dates missing".into()),
            ..PageDates::default()
        };
        attach(&mut value, dates);
        assert_eq!(value["warnings"], json!(["first", "dates missing"]));
    }
}
