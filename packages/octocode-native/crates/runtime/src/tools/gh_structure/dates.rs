//! Freshness of a tree page: each listed entry's last-commit date at the
//! listed commit, and that commit's own date, as `YYYY-MM-DD`.
// A day is enough: entries of one page are compared by day, and the
// listed commit's day bounds them all. Full timestamps would add 10 bytes
// per entry for an ordering no listing reader asked for.
use super::*;
use crate::response::paging::push_warnings;
use crate::tools::gh_shared::dates::utc_day;
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
}

/// Entries one page lists and dates: one GraphQL request's aliases. Spike
/// 2026-10-06: 1×100 aliases ≈ 3.4 s, 1×300 ≈ 7.6 s (near the 10 s
/// GraphQL timeout), so a dated page lists at most that many entries and the
/// walk's next page leads the rest. The executor spaces GraphQL starts by a
/// second, so splitting a request in two or more saves little wall time.
pub(super) const DATED_PER_PAGE: usize = octocode_github::MAX_PATHS_PER_REQUEST;

/// The dates of `listing[shown]`, the entries one page shows.
///
/// The listing is dated in chunks aligned on [`DATED_PER_PAGE`] boundaries
/// of the whole listing, one GraphQL request each, whatever the page size:
/// every page asks for a chunk in the same words, so a chunk an earlier
/// page of the walk read is answered by the evidence its cursor owns (the
/// request journal) and a full walk sends one request per chunk, never
/// dating a path twice. The chunks one page needs (at most two) are sent
/// together. A failed chunk leaves its entries undated, named in
/// [`PageDates::warning`].
pub(super) async fn page_dates(
    provider: &GitHubProvider,
    owner: &str,
    repo: &str,
    commit_sha: &str,
    listing: &[TreeEntry],
    shown: std::ops::Range<usize>,
    context: &RequestContext,
) -> PageDates {
    let shown = shown.start.min(listing.len())..shown.end.min(listing.len());
    if shown.is_empty() {
        return PageDates::default();
    }
    let chunks = (shown.start / DATED_PER_PAGE..=(shown.end - 1) / DATED_PER_PAGE)
        .map(|chunk| {
            let start = chunk * DATED_PER_PAGE;
            &listing[start..start.saturating_add(DATED_PER_PAGE).min(listing.len())]
        })
        .collect::<Vec<_>>();
    let answers = futures_util::future::join_all(chunks.iter().map(|chunk| {
        let paths = chunk
            .iter()
            .map(|entry| entry.path.as_str())
            .collect::<Vec<_>>();
        async move {
            provider
                .transport
                .path_commit_dates(owner, repo, commit_sha, &paths, context)
                .await
        }
    }))
    .await;
    let wanted = listing[shown]
        .iter()
        .map(|entry| entry.path.as_str())
        .collect::<std::collections::HashSet<_>>();
    let mut dates = PageDates::default();
    let mut missing = 0;
    let mut failure = None;
    for (chunk, answer) in chunks.iter().zip(answers) {
        dates.commit = dates
            .commit
            .or_else(|| answer.commit_date.as_deref().and_then(utc_day));
        for (entry, date) in chunk.iter().zip(&answer.dates) {
            if !wanted.contains(entry.path.as_str()) {
                continue;
            }
            match date.as_deref().and_then(utc_day) {
                Some(day) => {
                    dates.entries.insert(entry.path.clone(), day);
                }
                None if answer.error.is_some() => missing += 1,
                None => {}
            }
        }
        failure = failure.or(answer.error);
    }
    if let Some(error) = failure.filter(|_| missing > 0) {
        dates.warning = Some(format!(
            "Last-commit dates are missing for {missing} {}: {}.",
            if missing == 1 { "entry" } else { "files" },
            error.message
        ));
    }
    dates
}

/// The page's size cap, stated when `limit` asked for more than one date
/// request covers and the listing needs more than one page: the page lists
/// [`DATED_PER_PAGE`] entries and the cursor leads to the rest.
pub(super) fn cap_warning(page: &super::listing::ListingPage<'_>) -> Option<String> {
    let limit = page
        .capped_from
        .filter(|_| page.total_entries > page.per_page)?;
    Some(format!(
        "limit {limit} is lowered to {DATED_PER_PAGE}: a page lists the entries one last-commit-date request covers."
    ))
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
