//! GitHub search paging: the 1,000-result window, next pages, completeness and repair leads.
use super::{
    GhFailure, RepoPath, repository_not_found, repository_recovery, repository_search_hint,
};
use crate::id::ToolId;
use crate::response::result::{Continuation, query_row};
use octocode_github::{ProviderError, ProviderErrorKind, ProviderErrorReason};
use serde_json::{Value, json};

/// Keywords are ANDed literal terms: a bare GitHub boolean operator (`OR`,
/// `NOT`, `AND`, as GitHub spells them) standing as a word of a keyword
/// would be searched as a word (quoted into a phrase) and silently empty
/// the search, so it is refused, naming the operator and the one way to ask
/// for alternatives.
pub(crate) fn reject_boolean_keywords<S: AsRef<str>>(keywords: &[S]) -> Result<(), ProviderError> {
    let operator = keywords
        .iter()
        .flat_map(|keyword| keyword.as_ref().split_whitespace())
        .find_map(|word| ["OR", "NOT", "AND"].into_iter().find(|op| word == *op));
    match operator {
        Some(operator) => Err(ProviderError::new(
            ProviderErrorKind::Validation,
            format!(
                "keywords are ANDed literal terms; {operator} is no operator here: run one search per alternative"
            ),
        )),
        None => Ok(()),
    }
}

/// Results GitHub search reaches for one query, however many match.
pub(crate) const SEARCH_RESULT_CAP: usize = 1000;

/// Where a GitHub search endpoint draws its 1,000-result line through the
/// pages of one query.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SearchWindow {
    /// A page is served while its first result lies inside the cap, so the
    /// last page may run past it: repository, issue and commit search.
    PageStart,
    /// A page is served only while its last result lies inside the cap
    /// (`page * per <= 1000`): code search. When `per` does not divide
    /// 1,000, the results past the last whole page are unreachable.
    PageEnd,
}

impl SearchWindow {
    /// The last page GitHub serves at `per` rows a page.
    pub(crate) fn last_page(self, per: usize) -> usize {
        let per = per.max(1);
        match self {
            Self::PageStart => SEARCH_RESULT_CAP.div_ceil(per),
            Self::PageEnd => (SEARCH_RESULT_CAP / per).max(1),
        }
    }

    /// What `matched` results reach through this window at `per` a page.
    pub(crate) fn reach(self, matched: usize, per: usize) -> SearchReach {
        let per = per.max(1);
        let total = matched.min(self.last_page(per).saturating_mul(per));
        SearchReach {
            total,
            pages: total.div_ceil(per),
            capped: matched > total,
        }
    }

    /// A page past the last one GitHub serves cannot be read.
    pub(crate) fn reject(self, page: usize, per: usize) -> Result<(), ProviderError> {
        let last = self.last_page(per);
        if page > last {
            Err(ProviderError::new(
                ProviderErrorKind::Validation,
                format!(
                    "Page {page} is past GitHub's search window: at {per} per page, page {last} is the last, reaching the first {} results.",
                    last.saturating_mul(per.max(1))
                ),
            )
            .with_reason(ProviderErrorReason::SearchWindowExceeded))
        } else {
            Ok(())
        }
    }
}

/// What one search's match count reaches through GitHub's 1,000-result
/// window at `per` rows a page.
#[derive(Clone, Copy)]
pub(crate) struct SearchReach {
    /// Reachable results.
    pub(crate) total: usize,
    pub(crate) pages: usize,
    /// More results matched than the window reaches.
    pub(crate) capped: bool,
}

impl SearchReach {
    /// Whether page `current` lies past the last page of a non-empty search.
    pub(crate) fn past_last(&self, current: usize) -> bool {
        self.total > 0 && current > self.pages
    }
}

/// A search failure's one repair lead, replacing its static hint: a page
/// past the search window leads to the last page a
/// [`SearchWindow::PageStart`] search serves (`repair`; a code search
/// re-points it at its own last page); a repository that did not resolve
/// leads to its [`repository_recovery`] (a search for its name and the
/// `owner`'s listing), labelled by its [`repository_search_hint`]. Any other
/// failure is returned unchanged.
pub(crate) fn with_search_repair(
    mut failure: GhFailure,
    tool: ToolId,
    query: &impl serde::Serialize,
    per: usize,
    owner: Option<&str>,
    repo: Option<&str>,
) -> GhFailure {
    let (leads, hints) = if failure.error.reason == Some(ProviderErrorReason::SearchWindowExceeded)
    {
        let last = page_replay(tool, query, SearchWindow::PageStart.last_page(per));
        (json!({ "repair": last }), Vec::new())
    } else if let Some(repo) = repo.filter(|_| repository_not_found(&failure.error)) {
        (
            repository_recovery(owner, repo),
            vec![repository_search_hint(owner, repo)],
        )
    } else {
        return failure;
    };
    failure.hints = hints;
    let mut next = failure.calls.take().unwrap_or_else(|| Box::new(json!({})));
    if let (Some(next), Value::Object(leads)) = (next.as_object_mut(), leads) {
        next.extend(leads);
    }
    failure.calls = Some(next);
    failure
}

/// `next.nextPage`: the same query at the page after `page`. GitHub's own
/// search window ([`SearchWindow`]) decides `has_more`.
pub(crate) fn add_next(
    value: &mut Value,
    tool: ToolId,
    query: &impl serde::Serialize,
    page: usize,
    has_more: bool,
) {
    if !has_more {
        return;
    }
    value["calls"] = json!({"nextPage": page_replay(tool, query, page + 1)});
}

/// The same query at `page`: an exact replay.
fn page_replay(tool: ToolId, query: &impl serde::Serialize, page: usize) -> Value {
    let mut row = query_row(query, &[]);
    row["page"] = json!(page);
    Continuation::continued(tool, row).build()
}

/// The listing that checks an empty search's `path` scope (a path that is
/// not there lists nothing): its parent directory, at the searched `ref`
/// when one is set. `None` without `owner`, `repo` and a `path`.
pub(crate) fn path_check(row: &Value) -> Option<Value> {
    let (owner, repo) = (row["owner"].as_str()?, row["repo"].as_str()?);
    let path = row["pathPrefix"].as_str()?.trim_matches('/');
    if path.is_empty() {
        return None;
    }
    let at = RepoPath {
        owner,
        repo,
        path: path.rsplit_once('/').map_or("", |(parent, _)| parent),
        reference: row["ref"].as_str(),
    };
    Some(at.lead(ToolId::GhStructure).build())
}

/// What GitHub said about one search page's completeness.
#[derive(Clone, Copy)]
pub(crate) struct SearchCompleteness<'a> {
    pub(crate) tool: ToolId,
    /// GitHub's own match count (past the cap, more than `reachable`).
    pub(crate) matched: usize,
    /// The results the search window reaches (`totalItems`).
    pub(crate) reachable: usize,
    /// GitHub's incomplete-results flag.
    pub(crate) incomplete: bool,
    pub(crate) page: usize,
    pub(crate) has_more: bool,
    /// What one result is (`code`, `repository`), for the count warning.
    pub(crate) subject: &'a str,
}

/// Provider-side incompleteness of a search page: the 1,000-result cap and
/// GitHub's own incomplete-results flag (with a `retry` of the same page).
/// Past the cap the first page states GitHub's match count, since the
/// reachable `totalItems` is not the count. Only the last reachable page of
/// a capped search ends coverage (`terminalLimit`: no executable
/// continuation remains).
pub(crate) fn apply_partial(
    value: &mut Value,
    query: &impl serde::Serialize,
    facts: &SearchCompleteness<'_>,
) {
    use crate::response::paging::{mark_partial, terminal_limit};
    let SearchCompleteness {
        tool,
        matched,
        reachable,
        incomplete,
        page,
        has_more,
        subject,
    } = *facts;
    if matched > reachable {
        // The match count is this call's fact; that search stops at its
        // window is the cap reason (`providerResultCap`), not a per-call
        // note.
        let count = (page == 1).then(|| {
            format!(
                "GitHub matched {matched} {subject} results; the first {reachable} are reachable."
            )
        });
        let warnings = Vec::from_iter(count);
        if let Some(pagination) = value.get_mut("pagination").and_then(Value::as_object_mut) {
            pagination.insert("totalItemsCapped".into(), Value::Bool(true));
        }
        if has_more {
            mark_partial(
                value,
                crate::response::paging::PartialReason::ProviderResultCap,
                &warnings,
            );
        } else {
            terminal_limit(
                value,
                crate::response::paging::PartialReason::ProviderResultCap,
                &warnings,
            );
        }
    }
    if incomplete {
        mark_partial(
            value,
            crate::response::paging::PartialReason::ProviderIncompleteResults,
            &[],
        );
        value["calls"]["retry"] = page_replay(tool, query, page);
    }
}
