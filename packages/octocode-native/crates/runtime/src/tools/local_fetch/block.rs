//! `block:true`: widen a line window to its enclosing declaration, so a read
//! that starts at (or inside) a function, class, or heading section returns
//! through its end instead of stopping mid-body and forcing a re-read. Spans
//! come from the same declaration outline behind `minify:"symbols"` and
//! clasify locate; a file type without one keeps the requested window.

use super::types::LineRange;

/// Largest declaration a block read returns in one window.
pub const BLOCK_MAX_LINES: usize = 400;

/// 1-based inclusive spans of the multi-line declarations the engine outlines.
pub(crate) fn declaration_spans(content: &str, path: &str) -> Option<Vec<(usize, usize)>> {
    let raw = octocode_engine::portable::extract_declarations(content, path)?;
    let facts: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let line = |declaration: &serde_json::Value, pointer: &str| {
        declaration
            .pointer(pointer)
            .and_then(serde_json::Value::as_u64)
            .and_then(|line| usize::try_from(line).ok())
            .map(|line| line + 1)
    };
    let spans: Vec<(usize, usize)> = facts["declarations"]
        .as_array()?
        .iter()
        .filter_map(|declaration| {
            let start = line(declaration, "/range/start/line")?;
            let end = line(declaration, "/range/end/line")?;
            // A one-line declaration is not a block to widen to.
            (end > start).then_some((start, end))
        })
        .collect();
    (!spans.is_empty()).then_some(spans)
}

/// The smallest declaration containing `line`.
fn innermost(spans: &[(usize, usize)], line: usize) -> Option<(usize, usize)> {
    spans
        .iter()
        .filter(|(start, end)| *start <= line && line <= *end)
        .min_by_key(|(start, end)| end - start)
        .copied()
}

fn no_outline(path: &str) -> String {
    format!(
        "block: no declaration outline for {path}; returned the requested lines. Read on with startLine/endLine."
    )
}

/// Widen each range to the innermost declaration containing its first line.
/// A declaration over [`BLOCK_MAX_LINES`] keeps the range start and reads at
/// most that many lines of it; the lines it stops before go to `rest`.
pub fn widen_ranges(
    content: &str,
    path: &str,
    ranges: Vec<LineRange>,
    warnings: &mut Vec<String>,
    rest: &mut Vec<LineRange>,
) -> Vec<LineRange> {
    let Some(spans) = declaration_spans(content, path) else {
        warnings.push(no_outline(path));
        return ranges;
    };
    ranges
        .into_iter()
        .map(|range| {
            let Some((first, last)) = innermost(&spans, range.start) else {
                warnings.push(format!(
                    "block: no declaration encloses line {}; returned the requested lines.",
                    range.start
                ));
                return range;
            };
            let start = first.min(range.start);
            let end = last.max(range.end);
            if end + 1 - start <= BLOCK_MAX_LINES {
                return LineRange { start, end };
            }
            let end = range.end.max(last.min(range.start + BLOCK_MAX_LINES - 1));
            if end < last {
                warnings.push(format!(
                    "block: declaration {first}-{last} exceeds {BLOCK_MAX_LINES} lines; returned {}-{end}. next.continueBlock reads through line {last}.",
                    range.start,
                ));
                rest.push(LineRange {
                    start: end + 1,
                    end: last,
                });
            }
            LineRange {
                start: range.start,
                end,
            }
        })
        .collect()
}

/// Replace each match window with the innermost declaration containing the
/// matched line, when that declaration fits [`BLOCK_MAX_LINES`]; a larger
/// one keeps the window and goes to `oversized_spans`.
pub fn widen_matches(
    content: &str,
    path: &str,
    hits: &[usize],
    windows: Vec<LineRange>,
    warnings: &mut Vec<String>,
    oversized_spans: &mut Vec<LineRange>,
) -> Vec<LineRange> {
    let Some(spans) = declaration_spans(content, path) else {
        warnings.push(no_outline(path));
        return windows;
    };
    let mut oversized = 0;
    let mut unenclosed = 0;
    let widened = hits
        .iter()
        .zip(windows)
        .map(|(&hit, window)| match innermost(&spans, hit) {
            Some((start, end)) if end + 1 - start <= BLOCK_MAX_LINES => LineRange { start, end },
            Some((start, end)) => {
                oversized += 1;
                oversized_spans.push(LineRange { start, end });
                window
            }
            None => {
                unenclosed += 1;
                window
            }
        })
        .collect();
    // `matchedLines` names every hit; the warning carries the count.
    if unenclosed > 0 {
        warnings.push(format!(
            "block: {unenclosed} match(es) sit outside any declaration; those keep their match context window. Read on with startLine/endLine."
        ));
    }
    if oversized > 0 {
        warnings.push(format!(
            "block: {oversized} match(es) sit in declarations over {BLOCK_MAX_LINES} lines; those keep their context window, and hints.readBlock reads the rest of each declaration."
        ));
    }
    widened
}

#[cfg(test)]
mod tests {
    use super::*;

    const PY: &str = "import os\n\ndef first(a):\n    x = 1\n    return a\n\n\ndef second(b):\n    if b:\n        return 1\n    return 2\n";

    #[test]
    fn ranges_widen_to_the_enclosing_declaration() {
        let mut warnings = vec![];
        let widened = widen_ranges(
            PY,
            "m.py",
            vec![
                LineRange { start: 9, end: 9 },
                LineRange { start: 4, end: 4 },
            ],
            &mut warnings,
            &mut vec![],
        );
        assert_eq!(
            widened,
            vec![
                LineRange { start: 8, end: 11 },
                LineRange { start: 3, end: 5 }
            ]
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        // Outside every declaration the range stays, with a note.
        let kept = widen_ranges(
            PY,
            "m.py",
            vec![LineRange { start: 1, end: 1 }],
            &mut warnings,
            &mut vec![],
        );
        assert_eq!(kept, vec![LineRange { start: 1, end: 1 }]);
        assert!(warnings[0].contains("line 1"), "{warnings:?}");
    }

    #[test]
    fn unsupported_files_keep_the_window_with_a_note() {
        let mut warnings = vec![];
        let kept = widen_ranges(
            "a\nb\n",
            "notes.unknownext",
            vec![LineRange { start: 1, end: 1 }],
            &mut warnings,
            &mut vec![],
        );
        assert_eq!(kept, vec![LineRange { start: 1, end: 1 }]);
        assert!(
            warnings[0].starts_with("block: no declaration outline"),
            "{warnings:?}"
        );
    }

    #[test]
    fn oversized_declarations_are_capped() {
        let body: String = (0..450).map(|i| format!("    x{i} = {i}\n")).collect();
        let source = format!("def big():\n{body}    return 0\n");
        let mut warnings = vec![];
        let widened = widen_ranges(
            &source,
            "m.py",
            vec![LineRange { start: 10, end: 12 }],
            &mut warnings,
            &mut vec![],
        );
        assert_eq!(
            widened,
            vec![LineRange {
                start: 10,
                end: 409
            }]
        );
        assert!(warnings[0].contains("exceeds 400 lines"), "{warnings:?}");
        let windows = widen_matches(
            &source,
            "m.py",
            &[20],
            vec![LineRange { start: 18, end: 22 }],
            &mut warnings,
            &mut vec![],
        );
        assert_eq!(windows, vec![LineRange { start: 18, end: 22 }]);
    }

    #[test]
    fn matches_widen_to_their_declaration() {
        let mut warnings = vec![];
        let windows = widen_matches(
            PY,
            "m.py",
            &[10],
            vec![LineRange { start: 9, end: 11 }],
            &mut warnings,
            &mut vec![],
        );
        assert_eq!(windows, vec![LineRange { start: 8, end: 11 }]);
        assert!(warnings.is_empty(), "{warnings:?}");
    }

    #[test]
    fn a_match_outside_every_declaration_says_block_did_not_apply() {
        let mut warnings = vec![];
        let windows = widen_matches(
            PY,
            "m.py",
            &[1, 10],
            vec![
                LineRange { start: 1, end: 2 },
                LineRange { start: 9, end: 11 },
            ],
            &mut warnings,
            &mut vec![],
        );
        assert_eq!(
            windows,
            vec![
                LineRange { start: 1, end: 2 },
                LineRange { start: 8, end: 11 }
            ]
        );
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(
            warnings[0].starts_with("block: 1 match(es) sit outside any declaration;"),
            "{warnings:?}"
        );
    }

    #[test]
    fn javascript_member_assigned_functions_are_blocks() {
        let source = "var res = module.exports = {};\n\nres.redirect = function redirect(url) {\n  var status = 302;\n  if (url) {\n    status = 301;\n  }\n  return status;\n};\n";
        let mut warnings = vec![];
        let windows = widen_matches(
            source,
            "lib/response.js",
            &[3],
            vec![LineRange { start: 1, end: 5 }],
            &mut warnings,
            &mut vec![],
        );
        assert_eq!(windows, vec![LineRange { start: 3, end: 9 }]);
        assert!(warnings.is_empty(), "{warnings:?}");
    }
}
