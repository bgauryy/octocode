//! What a streamed regex scan says about the scan edges it cut the source at.
//! The scan follows a match [`Scan::overlap`] bytes past an edge, so a match
//! is only cut when it is longer than that and crosses an edge: a pattern that
//! can span lines, at an edge between lines, or any pattern, at an edge inside
//! a line too long to cut elsewhere. The pattern's syntax tree decides.

use super::scan::Scan;
use crate::tools::source_view::*;
use octocode_contract_types::LocalFetchQuery;
use octocode_engine::search::pattern_reach::pattern_reach;

/// The warning for a regex read whose scan may have cut a match at an edge;
/// `None` for a literal read, or when no match of the patterns could be cut.
pub(super) fn cut_warning(q: &LocalFetchQuery, found: &Scan) -> Option<String> {
    if !local_fetch_query::is_regex(q) {
        return None;
    }
    let (mut between_lines, mut inside_line) = (false, false);
    for pattern in local_fetch_query::match_strings(q) {
        let reach = pattern_reach(pattern, !local_fetch_query::case_sensitive_for(q, pattern));
        if reach.may_exceed(found.overlap) {
            between_lines |= found.line_edges > 0 && reach.crosses_newline;
            inside_line |= found.mid_edges > 0;
        }
    }
    let causes: Vec<&str> = [
        (between_lines, "the pattern can span lines"),
        (inside_line, "a line longer than the scan window was cut"),
    ]
    .into_iter()
    .filter_map(|(cause, text)| cause.then_some(text))
    .collect();
    (!causes.is_empty()).then(|| {
        format!(
            "Regex matches are found within {} bytes of each scan edge; a longer match crossing an edge is not selected ({}).",
            found.overlap,
            causes.join(" and ")
        )
    })
}
