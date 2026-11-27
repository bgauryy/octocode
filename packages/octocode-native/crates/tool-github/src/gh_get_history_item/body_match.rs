//! `matchString` over discussion bodies: an issue's comments and a pull
//! request's comments, reviews and review comments keep the entries whose
//! `body` holds a needle (each section on its own), the way changed files
//! keep the ones whose patch does.
use super::matcher::Matcher;
use super::window::Loaded;
use super::{HistoryItemRequest, gh_get_history_item_query};
use serde_json::Value;
use std::borrow::Cow;

/// Whether a provider entry's `body` holds the needle; every entry when
/// the read has no `matchString`.
pub(super) fn holds(needle: Option<&Matcher>, entry: &Value) -> bool {
    needle.is_none_or(|needle| {
        entry
            .get("body")
            .and_then(Value::as_str)
            .is_some_and(|body| needle.matches(body))
    })
}

/// Whether a read keeps a provider entry: its body holds the needle, and,
/// when the read leaves bots out (`bots:false`), a person wrote it.
pub(super) fn keeps(needle: Option<&Matcher>, humans: bool, entry: &Value) -> bool {
    holds(needle, entry) && !(humans && super::util::is_bot(entry))
}

/// The thread a provider entry belongs to: the comment it replies to
/// (`in_reply_to_id`, which GitHub sets to a review thread's first comment),
/// else itself. An entry of a list without replies is its own thread.
fn thread(entry: &Value) -> Option<String> {
    entry
        .get("in_reply_to_id")
        .filter(|id| !id.is_null())
        .or_else(|| entry.get("id"))
        .filter(|id| !id.is_null())
        .map(|id| id.to_string().trim_matches('"').to_owned())
}

/// The entries of one load `bots:false` keeps (`humans`): those a person
/// wrote, and a bot's or app's in a thread a person wrote in, so an answered
/// thread stays whole. The disclosure of those it left out joins `warnings`.
/// A scan that stopped early counts only the entries it listed; a list with
/// replies is read whole first (`pull_request::load::load_inline`), since a
/// reply may follow its thread in any batch.
pub(super) fn humans_only(
    entries: Vec<Value>,
    humans: bool,
    exhausted: bool,
    noun: &str,
    warnings: &mut Vec<String>,
) -> Vec<Value> {
    if !humans {
        return entries;
    }
    let listed = entries.len();
    let answered = entries
        .iter()
        .filter(|entry| !super::util::is_bot(entry))
        .filter_map(thread)
        .collect::<std::collections::HashSet<_>>();
    let bots = entries
        .iter()
        .filter(|entry| super::util::is_bot(entry))
        .count();
    let kept = entries
        .into_iter()
        .filter(|entry| {
            !super::util::is_bot(entry)
                || thread(entry).is_some_and(|thread| answered.contains(&thread))
        })
        .collect::<Vec<_>>();
    let dropped = listed - kept.len();
    if dropped > 0 {
        let scanned = if exhausted { "" } else { " scanned so far" };
        let stayed = match bots - dropped {
            0 => String::new(),
            stayed => format!(" {stayed} more stay: a person wrote in their thread."),
        };
        warnings.push(format!(
            "bots:false left out {dropped} of {listed} {noun}{scanned} that a bot or app wrote.{stayed}"
        ));
    }
    kept
}

/// What a `matchString` left of the entries one load listed.
pub(super) struct Kept {
    pub(super) entries: Vec<Value>,
    /// Entries the load listed (the scan stops once the page is covered).
    pub(super) listed: usize,
    /// Whether the scan reached the end of the provider's list.
    pub(super) exhausted: bool,
}

impl Kept {
    /// The entries of `listed` that hold the needle.
    pub(super) fn of(listed: Vec<Value>, needle: Option<&Matcher>, exhausted: bool) -> Self {
        let count = listed.len();
        Self {
            entries: listed
                .into_iter()
                .filter(|entry| holds(needle, entry))
                .collect(),
            listed: count,
            exhausted,
        }
    }

    /// The disclosure of the dropped entries, `None` when none was dropped:
    /// a scan that stopped early counts only the entries it listed.
    pub(super) fn warning(&self, noun: &str) -> Option<String> {
        let dropped = self.listed - self.entries.len();
        (dropped > 0).then(|| {
            let scanned = if self.exhausted {
                ""
            } else {
                " scanned so far"
            };
            super::filter::match_dropped_warning(
                dropped,
                self.listed,
                &format!("{noun}{scanned}"),
                "body",
            )
        })
    }
}

/// A loaded pull-request section kept to the entries whose body holds the
/// needle; a dropped entry's disclosure joins `warnings`.
pub(super) fn keep_loaded(
    loaded: Option<Loaded>,
    needle: Option<&Matcher>,
    humans: bool,
    noun: &str,
    warnings: &mut Vec<String>,
) -> Option<Loaded> {
    let mut loaded = loaded?;
    loaded.items = humans_only(
        std::mem::take(&mut loaded.items),
        humans,
        loaded.state.exhausted,
        noun,
        warnings,
    );
    if needle.is_none() {
        return Some(loaded);
    }
    let kept = Kept::of(
        std::mem::take(&mut loaded.items),
        needle,
        loaded.state.exhausted,
    );
    warnings.extend(kept.warning(noun));
    loaded.items = kept.entries;
    Some(loaded)
}

/// A discussion body as a `matchString` read shows it ([`hit_view`]), with
/// the whole body's size in chars; the whole body, and no size, without a
/// `matchString` or when no single line holds a hit (a match across lines).
pub(super) fn matched_view<'a>(
    body: &'a str,
    query: &HistoryItemRequest,
) -> (Cow<'a, str>, Option<usize>) {
    let Some(needle) = query.matcher.as_deref() else {
        return (Cow::Borrowed(body), None);
    };
    match hit_view(
        body,
        needle,
        gh_get_history_item_query::match_context(query),
    ) {
        Some(view) => (Cow::Owned(view), Some(body.chars().count())),
        None => (Cow::Borrowed(body), None),
    }
}

/// The lines of `body` holding a hit, with `context` lines before and after
/// each, every line opening `N\t` (its 1-based line in the body) and a `--`
/// line between runs that do not touch: the view a patch's `matchString`
/// read gives its hunks. `None` when no line holds a hit, or when the view
/// would hold every line: nothing is left out, so the body stays as it is.
pub(super) fn hit_view(body: &str, needle: &Matcher, context: (usize, usize)) -> Option<String> {
    let lines = body.lines().collect::<Vec<_>>();
    let last = lines.len().saturating_sub(1);
    let mut runs: Vec<(usize, usize)> = Vec::new();
    for (hit, _) in lines
        .iter()
        .enumerate()
        .filter(|(_, line)| needle.matches(line))
    {
        let (from, to) = (
            hit.saturating_sub(context.0),
            hit.saturating_add(context.1).min(last),
        );
        match runs.last_mut() {
            Some(run) if from <= run.1.saturating_add(1) => run.1 = run.1.max(to),
            _ => runs.push((from, to)),
        }
    }
    if runs.is_empty() || runs == [(0, last)] {
        return None;
    }
    let mut view = String::new();
    for (index, &(from, to)) in runs.iter().enumerate() {
        if index > 0 {
            view.push_str("--\n");
        }
        for (at, line) in lines
            .iter()
            .enumerate()
            .take(to.saturating_add(1))
            .skip(from)
        {
            view.push_str(&at.saturating_add(1).to_string());
            view.push('\t');
            view.push_str(line);
            view.push('\n');
        }
    }
    view.pop();
    Some(view)
}

#[cfg(test)]
mod tests;
