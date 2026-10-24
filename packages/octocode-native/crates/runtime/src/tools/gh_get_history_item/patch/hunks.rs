//! Patch views: the `matchString` hit runs (widened from the head text) and
//! the numbered diff of one changed file.
use super::super::HistoryItemRequest;
use super::super::text::needle;
use crate::tools::gh_get_history_item::gh_get_history_item_query;
use std::collections::HashMap;

mod matching;
#[cfg(test)]
pub(super) use matching::HunkMatch;
mod stream;

pub(super) use matching::matching_hunks;

/// File text at the pull request's head (`headSha`): a `matchString` view
/// widens its hit runs past GitHub's diff context from it. A path maps to
/// `None` when its read failed; its clipped runs are then flagged.
#[derive(Debug, Default)]
pub(in crate::tools::gh_get_history_item) struct HeadSources(HashMap<String, Option<Vec<String>>>);

impl HeadSources {
    pub(in crate::tools::gh_get_history_item) fn insert(
        &mut self,
        path: String,
        lines: Option<Vec<String>>,
    ) {
        self.0.insert(path, lines);
    }
    pub(super) fn lines(&self, path: &str) -> Option<&[String]> {
        self.0.get(path)?.as_deref()
    }
}

/// One changed file's patch view.
pub(super) struct PatchView {
    pub(super) text: String,
    /// A `matchString` view narrowed to its hit runs.
    pub(super) narrowed: bool,
    /// Inclusive new-side line windows the requested context reaches past
    /// the hunks without head text to fill them.
    pub(super) clipped: Vec<(usize, usize)>,
    /// Char offsets in `text` of the lines a `matchString` view shows as
    /// hits (diff lines holding the needle); empty for any other view.
    pub(super) hits: Vec<usize>,
}

/// Diff lines kept around each `matchString` hit: `contextLines`, else the
/// contract's `limits.defaultReadContextLines`. Related reads offer
/// unfiltered patches beyond that selection.
pub(in crate::tools::gh_get_history_item) fn match_context(
    query: &HistoryItemRequest,
) -> (usize, usize) {
    gh_get_history_item_query::match_context(query)
}

/// Whether a `matchString` view of `patch` needs file text beyond its hunks
/// to hold `context` lines around every hit.
pub(in crate::tools::gh_get_history_item) fn head_text_needed(
    patch: &str,
    needle: &super::super::matcher::Matcher,
    context: (usize, usize),
) -> bool {
    matching_hunks(patch, needle, context, None).is_some_and(|found| !found.clipped.is_empty())
}

/// The patch view of one changed file: a `matchString` view keeps only the
/// hit runs (widened from the head text when the hunks hold too little
/// context); every other read is the whole patch. Code hunks are never
/// minified. Either view is numbered (see [`number_patch`]).
pub(super) fn history_patch_view(value: &str, path: &str, query: &HistoryItemRequest) -> PatchView {
    if let Some(needle) = needle(query) {
        let source = query
            .head_sources
            .as_deref()
            .and_then(|sources| sources.lines(path));
        if let Some(found) = matching_hunks(value, &needle, match_context(query), source) {
            let text = number_patch(&found.text);
            let hits = line_starts(&text, &found.hit_lines);
            return PatchView {
                text,
                narrowed: true,
                clipped: found.clipped,
                hits,
            };
        }
    }
    PatchView {
        text: number_patch(value),
        narrowed: false,
        clipped: Vec::new(),
        hits: Vec::new(),
    }
}

/// Char offsets of the given (sorted) line indices in `text`.
fn line_starts(text: &str, lines: &[usize]) -> Vec<usize> {
    let mut wanted = lines.iter().peekable();
    let mut starts = Vec::with_capacity(lines.len());
    let mut at = 0usize;
    for (index, line) in text.split_inclusive('\n').enumerate() {
        while wanted.next_if(|&&want| want < index).is_some() {}
        if wanted.next_if(|&&want| want == index).is_some() {
            starts.push(at);
        }
        at += line.chars().count();
    }
    starts
}

/// The `(start, count)` of a hunk header's `mark` side (`-` or `+`): the
/// one hunk-header parser every patch view numbers its lines by.
pub(in crate::tools::gh_get_history_item) fn hunk_side(
    header: &str,
    mark: char,
) -> Option<(usize, usize)> {
    let side = header.split(' ').find_map(|part| part.strip_prefix(mark))?;
    let mut numbers = side.split(',').map(str::parse::<usize>);
    let start = numbers.next()?.ok()?;
    let count = match numbers.next() {
        Some(count) => count.ok()?,
        None => 1,
    };
    Some((start, count))
}

/// Number a unified diff on the side of each line's sign, `cat -n`-like:
/// each kept or added line opens with its new-file line number and a tab
/// (`86\t ctx`, `87\t+added`), each removed line with its old-file line
/// number (`85\t-removed`), and a `\ No newline` marker with a bare tab.
/// So `+`/` ` numbers are ranges at the change's commit and `-` numbers are
/// ranges at its parent. `@@ -a,b +c,d @@ heading` lines stay as GitHub sent
/// them (the heading is the enclosing symbol). Text that does not open with
/// a hunk header (empty, or already a line-filtered `+N:` view) is returned
/// unchanged. Removing the gutter (up to the first tab of each non-header
/// line) restores the raw patch.
pub(super) fn number_patch(patch: &str) -> String {
    if !patch.starts_with("@@") {
        return patch.to_owned();
    }
    let sep = crate::response::numbered::SEPARATOR;
    let mut out = String::with_capacity(patch.len() + patch.len() / 8);
    let (mut old, mut new) = (0usize, 0usize);
    for line in patch.split_inclusive('\n') {
        if line.starts_with("@@") {
            old = hunk_side(line, '-').map_or(0, |(start, _)| start);
            new = hunk_side(line, '+').map_or(0, |(start, _)| start);
            out.push_str(line);
            continue;
        }
        match line.as_bytes().first() {
            Some(b'\\') => {}
            Some(b'-') => {
                out.push_str(&old.to_string());
                old += 1;
            }
            Some(b'+') => {
                out.push_str(&new.to_string());
                new += 1;
            }
            _ => {
                out.push_str(&new.to_string());
                old += 1;
                new += 1;
            }
        }
        out.push(sep);
        out.push_str(line);
    }
    out
}

/// Context lines GitHub puts around each change of a diff.
pub(in crate::tools::gh_get_history_item) const DIFF_CONTEXT_LINES: usize = 3;
