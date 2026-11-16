//! `matchString` over discussion bodies: an issue's comments and a pull
//! request's comments, reviews and review comments keep the entries whose
//! `body` holds a needle (each section on its own), the way changed files
//! keep the ones whose patch does.
use super::matcher::Matcher;
use super::window::Loaded;
use serde_json::Value;

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
            let warning = super::filter::match_dropped_warning(
                dropped,
                self.listed,
                &format!("{noun}{scanned}"),
                "body",
            );
            // The read always builds its recovery (the same read without the
            // literal); the note promises it, so the served row keeps it.
            format!(
                "{}; related reads them without matchString.",
                warning.trim_end_matches('.')
            )
        })
    }
}

/// A loaded pull-request section kept to the entries whose body holds the
/// needle; a dropped entry's disclosure joins `warnings`.
pub(super) fn keep_loaded(
    loaded: Option<Loaded>,
    needle: Option<&Matcher>,
    noun: &str,
    warnings: &mut Vec<String>,
) -> Option<Loaded> {
    let mut loaded = loaded?;
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
