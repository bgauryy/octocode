//! `block:true`: widen a line window to its enclosing declaration, so a read
//! that starts at (or inside) a function, class, or heading section returns
//! through its end instead of stopping mid-body and forcing a re-read. Spans
//! come from the same declaration outline behind `minify:"symbols"` and
//! clasify locate; a file type without one keeps the requested window.

use super::types::LineRange;
use crate::tools::declarations_cache::{Declared, all_declarations, file_document};

/// Largest declaration a block read returns in one window.
pub const BLOCK_MAX_LINES: usize = 400;

/// The multi-line declarations the engine outlines, in outline order.
pub(crate) fn declarations(content: &str, path: &str) -> Option<Vec<Declared>> {
    let declared: Vec<Declared> = all_declarations(content, path)?
        .into_iter()
        // A one-line declaration is not a block to widen to.
        .filter(|declared| declared.end > declared.start)
        .collect();
    (!declared.is_empty()).then_some(declared)
}

/// A file's outline as a block read widens with it: the multi-line
/// declarations, and the lines of the one-line ones (nothing to widen to,
/// which is not the same as no declaration at all).
struct Blocks {
    declared: Vec<Declared>,
    one_line: Vec<usize>,
}

impl Blocks {
    /// `None` when the file type has no outline or the outline is empty.
    fn of(content: &str, path: &str) -> Option<Self> {
        let (declared, one_line): (Vec<Declared>, Vec<Declared>) = all_declarations(content, path)?
            .into_iter()
            .partition(|declared| declared.end > declared.start);
        if declared.is_empty() && one_line.is_empty() {
            return None;
        }
        let mut one_line: Vec<usize> = one_line.iter().map(|declared| declared.start).collect();
        one_line.sort_unstable();
        one_line.dedup();
        Some(Self { declared, one_line })
    }

    fn spans(&self) -> Vec<(usize, usize)> {
        self.declared.iter().map(|d| (d.start, d.end)).collect()
    }

    fn is_one_line(&self, line: usize) -> bool {
        self.one_line.binary_search(&line).is_ok()
    }
}

/// The 1-based line spans the parser could not parse and recovered from;
/// empty after a clean parse.
fn unparsed_spans(content: &str, path: &str) -> Vec<[usize; 2]> {
    file_document(content, path)
        .map(|document| {
            document
                .error_lines
                .iter()
                .map(|span| span.map(crate::num::usize_of))
                .collect()
        })
        .unwrap_or_default()
}

/// `spans` as a note names them: `"3, 7-9"`.
fn span_list(spans: &[[usize; 2]]) -> String {
    spans
        .iter()
        .map(|&[start, end]| {
            if start == end {
                start.to_string()
            } else {
                format!("{start}-{end}")
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Why the parser could not parse lines of `content`, and what `misses`:
/// a span holding a `redacted` line or the line after it (where the parser
/// meets the placeholder's missing syntax) is redacted text the grammar
/// cannot read; the rest are syntax the grammar lacks (Flow types, in
/// JavaScript). `None` after a clean parse.
fn unparsed_note(content: &str, path: &str, redacted: &[usize], misses: &str) -> Option<String> {
    let (by_redaction, by_syntax): (Vec<_>, Vec<_>) = unparsed_spans(content, path)
        .into_iter()
        .partition(|&[start, end]| {
            redacted
                .iter()
                .any(|&line| line <= end && start <= line.saturating_add(1))
        });
    let example = std::path::Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .filter(|ext| octocode_engine::text::JS_EXTENSIONS.contains(ext))
        .map_or("", |_| ", e.g. Flow types");
    let notes = [
        (!by_syntax.is_empty()).then(|| {
            format!(
                "The parser could not parse lines {} of this file (syntax its grammar lacks{example}) and recovered, so {misses}.",
                span_list(&by_syntax)
            )
        }),
        (!by_redaction.is_empty()).then(|| {
            format!(
                "The parser could not parse lines {} of this file, where secret-shaped text was redacted, so {misses}.",
                span_list(&by_redaction)
            )
        }),
    ];
    let notes = notes.into_iter().flatten().collect::<Vec<_>>();
    (!notes.is_empty()).then(|| notes.join(" "))
}

/// Why a block may miss its enclosing declaration: the parser recovered
/// from syntax errors, named with the skipped line spans. A read states it
/// once, however many of its windows it explains.
fn recovered_parse(content: &str, path: &str) -> Option<String> {
    unparsed_note(
        content,
        path,
        &[],
        "the outline may miss the enclosing declaration",
    )
    .map(|note| format!(" {note}"))
}

/// Why a symbols outline may miss declarations: the same recovered parse
/// a block read names, with the `redacted` lines (1-based) of `content`.
pub(super) fn outline_recovery_warning(
    content: &str,
    path: &str,
    redacted: &[usize],
) -> Option<String> {
    unparsed_note(
        content,
        path,
        redacted,
        "this outline may miss declarations there",
    )
}

/// 1-based inclusive spans of the multi-line declarations the engine outlines.
fn declaration_spans(content: &str, path: &str) -> Option<Vec<(usize, usize)>> {
    declarations(content, path).map(|declared| declared.iter().map(|d| (d.start, d.end)).collect())
}

/// `lines` as a warning names them: `line 4` or `lines 4, 9`.
fn named_lines(lines: &[usize]) -> String {
    let list = lines
        .iter()
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    match lines {
        [_] => format!("line {list}"),
        _ => format!("lines {list}"),
    }
}

/// Whether a source line is part of the doc comment or attribute run that
/// leads a declaration: a comment in the comment syntax of the file's
/// grammar, an attribute (`#[…]`) or a decorator (`@…`).
fn leads_declaration(path: &str, line: &str) -> bool {
    let line = line.trim_start();
    line.starts_with("#[")
        || line.starts_with('@')
        || octocode_engine::signatures::is_comment_line(path, line).unwrap_or(false)
}

/// Declaration spans for a block read: each widened up over the doc
/// comments and attributes directly above it, so the window holds the
/// declaration as written. Document headings keep their own spans.
fn block_spans(content: &str, path: &str, blocks: &Blocks) -> Vec<(usize, usize)> {
    let lines: Vec<&str> = content.lines().collect();
    blocks
        .spans()
        .into_iter()
        .map(|span| with_docs(&lines, path, span))
        .collect()
}

/// `span` widened up over the doc comment and attribute run directly above
/// it; a document heading keeps its span.
fn with_docs(lines: &[&str], path: &str, (start, end): (usize, usize)) -> (usize, usize) {
    if matches!(
        crate::content::classify_file_type(path),
        Some(crate::content::FileType::Doc)
    ) {
        return (start, end);
    }
    let mut first = start;
    while first > 1
        && lines
            .get(first - 2)
            .is_some_and(|line| leads_declaration(path, line))
    {
        first -= 1;
    }
    (first, end)
}

/// The smallest declaration containing `line`.
fn innermost(spans: &[(usize, usize)], line: usize) -> Option<(usize, usize)> {
    spans
        .iter()
        .filter(|(start, end)| *start <= line && line <= *end)
        .min_by_key(|(start, end)| end - start)
        .copied()
}

fn no_outline(content: &str, path: &str) -> String {
    format!(
        "block: no declaration outline for {path}; returned the requested lines.{}",
        recovered_parse(content, path).unwrap_or_default()
    )
}

/// The innermost declaration (up to [`BLOCK_MAX_LINES`]) enclosing each hit,
/// or `None` when the file type has no declaration outline. The doc comments
/// above a declaration are not part of it here: a window that shows its body
/// whole is not cut.
pub fn enclosing(content: &str, path: &str, hits: &[usize]) -> Option<Vec<LineRange>> {
    let spans = declaration_spans(content, path)?;
    Some(
        hits.iter()
            .filter_map(|&hit| innermost(&spans, hit))
            .filter(|(start, end)| end + 1 - start <= BLOCK_MAX_LINES)
            .map(|(start, end)| LineRange { start, end })
            .collect(),
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
    let Some(blocks) = Blocks::of(content, path) else {
        warnings.push(no_outline(content, path));
        return ranges;
    };
    let spans = block_spans(content, path, &blocks);
    let (mut unenclosed, mut one_line) = (vec![], vec![]);
    let widened = ranges
        .into_iter()
        .map(|range| {
            let Some((first, last)) = innermost(&spans, range.start) else {
                if blocks.is_one_line(range.start) {
                    one_line.push(range.start);
                } else {
                    unenclosed.push(range.start);
                }
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
                    "block: declaration {first}-{last} exceeds {BLOCK_MAX_LINES} lines; returned {}-{end}.",
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
        .collect();
    if !one_line.is_empty() {
        warnings.push(format!(
            "block: {} {} a one-line declaration, nothing to widen; returned the requested lines.",
            named_lines(&one_line),
            if one_line.len() == 1 {
                "holds"
            } else {
                "each hold"
            },
        ));
    }
    if !unenclosed.is_empty() {
        warnings.push(format!(
            "block: no declaration encloses {}; returned the requested lines.{}",
            named_lines(&unenclosed),
            recovered_parse(content, path).unwrap_or_default()
        ));
    }
    widened
}

/// The innermost declaration containing `line`.
fn innermost_declared(declared: &[Declared], line: usize) -> Option<&Declared> {
    declared
        .iter()
        .filter(|d| d.start <= line && line <= d.end)
        .min_by_key(|d| d.end - d.start)
}

/// Whether `hit` is a declaration's own head: its first line or its name
/// line.
fn declares(declared: &[Declared], hit: usize) -> bool {
    innermost_declared(declared, hit).is_some_and(|d| d.start == hit || d.line == hit)
}

/// Replace each match window with the innermost declaration containing the
/// matched line, when that declaration fits [`BLOCK_MAX_LINES`]; a larger
/// one keeps the window and goes to `oversized_spans`. When some hit is a
/// declaration's own head, only those hits widen: a call site of the
/// declared name keeps its window instead of reading its caller whole.
/// Each widened declaration is named in `blocks`.
pub fn widen_matches(
    content: &str,
    path: &str,
    hits: &[usize],
    windows: Vec<LineRange>,
    warnings: &mut Vec<String>,
    oversized_spans: &mut Vec<LineRange>,
    blocks: &mut Vec<super::types::DeclaredBlock>,
) -> Vec<LineRange> {
    let Some(outline) = Blocks::of(content, path) else {
        warnings.push(no_outline(content, path));
        return windows;
    };
    let spans = outline.spans();
    let declared = &outline.declared;
    let declaration_first = hits.iter().any(|&hit| declares(declared, hit));
    let lines: Vec<&str> = content.lines().collect();
    let mut oversized = 0;
    let mut unenclosed = 0;
    let mut one_line = 0;
    let mut call_sites = 0;
    let widened = hits
        .iter()
        .zip(windows)
        .map(|(&hit, window)| match innermost(&spans, hit) {
            Some(_) if declaration_first && !declares(declared, hit) => {
                call_sites += 1;
                window
            }
            // A hit on the declaration's head reads it as written, docs
            // included; a hit inside reads the declaration it sits in.
            Some((start, end)) if end + 1 - start <= BLOCK_MAX_LINES => {
                if let Some(owner) = innermost_declared(declared, hit)
                    && !owner.name.is_empty()
                    && !blocks.iter().any(|block| block.line == owner.line)
                {
                    blocks.push(super::types::DeclaredBlock {
                        symbol_name: owner.name.clone(),
                        line: owner.line,
                        end_line: owner.end,
                    });
                }
                let (start, end) = if hit == start {
                    with_docs(&lines, path, (start, end))
                } else {
                    (start, end)
                };
                LineRange { start, end }
            }
            Some((start, end)) => {
                oversized += 1;
                oversized_spans.push(LineRange { start, end });
                window
            }
            None if outline.is_one_line(hit) => {
                one_line += 1;
                window
            }
            None => {
                unenclosed += 1;
                window
            }
        })
        .collect();
    // `matchedLines` names every hit; the warning carries the count.
    if call_sites > 0 {
        warnings.push(format!(
            "block: {call_sites} match(es) are not declaration heads; they keep their context window, and only the declarations matched by name widen."
        ));
    }
    if one_line > 0 {
        warnings.push(format!(
            "block: {one_line} match(es) are one-line declarations, nothing to widen; those keep their match context window."
        ));
    }
    if unenclosed > 0 {
        warnings.push(format!(
            "block: {unenclosed} match(es) sit outside any declaration; those keep their match context window.{}",
            recovered_parse(content, path).unwrap_or_default()
        ));
    }
    if oversized > 0 {
        warnings.push(format!(
            "block: {oversized} match(es) sit in declarations over {BLOCK_MAX_LINES} lines; those keep their context window."
        ));
    }
    widened
}

#[cfg(test)]
mod tests {
    use super::*;

    const PY: &str = "import os\n\ndef first(a):\n    x = 1\n    return a\n\n\ndef second(b):\n    if b:\n        return 1\n    return 2\n";

    /// A recovered parse is named in block warnings with the lines the
    /// parser skipped; a clean parse adds nothing.
    #[test]
    fn a_recovered_parse_is_named_with_its_skipped_lines() {
        let flow = "// @flow\ntype Dispatch<A> = A => void;\nexport function useState<S>(init: S): [S, Dispatch<S>] {\n  return [init, () => {}];\n}\n";
        let note = recovered_parse(flow, "Hooks.js").expect("Flow types force a recovered parse");
        assert!(note.contains("could not parse lines"), "{note}");
        assert!(recovered_parse(PY, "m.py").is_none());
        let warning = outline_recovery_warning(flow, "Hooks.js", &[]).expect("outline warning");
        assert!(
            warning.starts_with("The parser could not parse lines"),
            "{warning}"
        );
        assert!(warning.contains("Flow types"), "{warning}");
        assert!(outline_recovery_warning(PY, "m.py", &[]).is_none());
    }

    /// Lines the parser skipped because redaction replaced them are named as
    /// redacted, never blamed on syntax the grammar lacks; Flow is named only
    /// for JavaScript.
    #[test]
    fn a_parse_broken_by_redaction_names_the_redacted_lines() {
        let rust =
            "fn a() {\n    let key = 1;\n[REDACTED-PRIVATEKEYFRAGMENT]\n    key\n}\n\nfn b() {}\n";
        let warning = outline_recovery_warning(rust, "k.rs", &[3]).expect("recovered parse");
        assert!(warning.contains("redacted"), "{warning}");
        assert!(!warning.contains("Flow"), "{warning}");
        let unredacted = outline_recovery_warning(rust, "k.rs", &[]).expect("recovered parse");
        assert!(!unredacted.contains("Flow"), "{unredacted}");
        assert!(!unredacted.contains("redacted"), "{unredacted}");
    }

    /// The lines a block widens up over are comments in the file's own
    /// grammar, attributes and decorators: a Python `#` comment leads its
    /// def, a Rust line that merely starts with `--` does not lead its fn.
    #[test]
    fn widening_follows_the_files_comment_syntax() {
        let py = "x = 1\n# Adds.\n@cache\ndef add(a, b):\n    s = a + b\n    return s\n";
        let widened = widen_ranges(
            py,
            "m.py",
            vec![LineRange { start: 5, end: 5 }],
            &mut vec![],
            &mut vec![],
        );
        assert_eq!(widened, vec![LineRange { start: 2, end: 6 }]);
        let rs = "const S: &str = \"a\n-- b\";\n#[inline]\nfn f() -> u8 {\n    1\n}\n";
        let widened = widen_ranges(
            rs,
            "lib.rs",
            vec![LineRange { start: 5, end: 5 }],
            &mut vec![],
            &mut vec![],
        );
        assert_eq!(widened, vec![LineRange { start: 3, end: 6 }]);
    }

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
            &mut vec![],
        );
        assert_eq!(windows, vec![LineRange { start: 18, end: 22 }]);
    }

    /// A window that holds a declaration's whole body is not cut short by
    /// the doc comments above it: those join only an explicit block read.
    #[test]
    fn a_window_cut_ignores_the_doc_comments_above_a_declaration() {
        let source = "/// Doc one.\n/// Doc two.\nfn target() {\n    body();\n}\n";
        assert_eq!(
            enclosing(source, "m.rs", &[4]),
            Some(vec![LineRange { start: 3, end: 5 }])
        );
    }

    /// A declaration's leading doc comments and attributes belong to it:
    /// a block read starts at the first of them, not at the `fn` line.
    #[test]
    fn blocks_include_leading_doc_comments_and_attributes() {
        let source = "use std::fmt;\n\n/// Doc one.\n/// Doc two.\n#[inline]\nfn target() {\n    body();\n}\n";
        let mut warnings = vec![];
        let widened = widen_ranges(
            source,
            "m.rs",
            vec![LineRange { start: 7, end: 7 }],
            &mut warnings,
            &mut vec![],
        );
        assert_eq!(
            widened,
            vec![LineRange { start: 3, end: 8 }],
            "{warnings:?}"
        );
        // A match on the declaration's own head reads it as written; with
        // a head hit present, a match inside a body keeps its window.
        let windows = widen_matches(
            source,
            "m.rs",
            &[6, 7],
            vec![
                LineRange { start: 6, end: 6 },
                LineRange { start: 7, end: 7 },
            ],
            &mut warnings,
            &mut vec![],
            &mut vec![],
        );
        assert_eq!(
            windows,
            vec![
                LineRange { start: 3, end: 8 },
                LineRange { start: 7, end: 7 }
            ]
        );
        // A blank line ends the doc run; a preprocessor line is not a doc.
        let c = "#define LIMIT 4\nint f(int a) {\n    return a;\n}\n";
        let widened = widen_ranges(
            c,
            "m.c",
            vec![LineRange { start: 3, end: 3 }],
            &mut warnings,
            &mut vec![],
        );
        assert_eq!(widened, vec![LineRange { start: 2, end: 4 }]);
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

    /// A hit on a one-line declaration is not outside every declaration:
    /// the warning says there is nothing to widen.
    #[test]
    fn one_line_declarations_are_named_as_such_not_as_outside() {
        let source =
            "pub struct Id(u8);\npub struct Name(String);\n\nfn body() {\n    1;\n}\n// trailing\n";
        let mut warnings = vec![];
        let windows = widen_matches(
            source,
            "m.rs",
            &[1, 2, 7],
            vec![
                LineRange { start: 1, end: 1 },
                LineRange { start: 2, end: 2 },
                LineRange { start: 7, end: 7 },
            ],
            &mut warnings,
            &mut vec![],
            &mut vec![],
        );
        assert_eq!(windows[0], LineRange { start: 1, end: 1 });
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert!(
            warnings[0].starts_with("block: 2 match(es) are one-line declarations"),
            "{warnings:?}"
        );
        assert!(
            warnings[1].starts_with("block: 1 match(es) sit outside any declaration"),
            "{warnings:?}"
        );
        let mut warnings = vec![];
        widen_ranges(
            source,
            "m.rs",
            vec![LineRange { start: 2, end: 2 }],
            &mut warnings,
            &mut vec![],
        );
        assert_eq!(
            warnings,
            [
                "block: line 2 holds a one-line declaration, nothing to widen; returned the requested lines."
            ]
        );
    }

    /// Several unenclosed ranges of one recovered parse share one warning,
    /// which names the parse recovery once.
    #[test]
    fn unenclosed_ranges_state_the_recovered_parse_once() {
        let flow = "// @flow\ntype Dispatch<A> = A => void;\nconst a = 1;\nconst b = 2;\nexport function useState<S>(init: S): [S, Dispatch<S>] {\n  return [init, () => {}];\n}\n";
        let mut warnings = vec![];
        widen_ranges(
            flow,
            "Hooks.js",
            vec![
                LineRange { start: 1, end: 1 },
                LineRange { start: 3, end: 3 },
                LineRange { start: 4, end: 4 },
            ],
            &mut warnings,
            &mut vec![],
        );
        let notes = warnings
            .iter()
            .filter(|warning| warning.contains("could not parse lines"))
            .count();
        assert_eq!(notes, 1, "{warnings:?}");
        assert!(
            warnings
                .iter()
                .any(|w| w.starts_with("block: no declaration encloses lines 1, ")),
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
            &mut vec![],
        );
        assert_eq!(windows, vec![LineRange { start: 3, end: 9 }]);
        assert!(warnings.is_empty(), "{warnings:?}");
    }

    /// A call-site hit keeps its window when a declaration hit exists; the
    /// widened declaration is named.
    #[test]
    fn call_site_hits_keep_window_when_a_declaration_hit_exists() {
        let source = PY.replace("return 1", "return first(1)");
        let mut warnings = vec![];
        let mut blocks = vec![];
        let windows = widen_matches(
            &source,
            "m.py",
            &[3, 10],
            vec![
                LineRange { start: 3, end: 3 },
                LineRange { start: 10, end: 10 },
            ],
            &mut warnings,
            &mut vec![],
            &mut blocks,
        );
        assert_eq!(
            windows,
            vec![
                LineRange { start: 3, end: 5 },
                LineRange { start: 10, end: 10 }
            ]
        );
        assert_eq!(blocks.len(), 1);
        assert_eq!(
            (
                blocks[0].symbol_name.as_str(),
                blocks[0].line,
                blocks[0].end_line
            ),
            ("first", 3, 5)
        );
        // No declaration hit: a call site still widens to its caller.
        let windows = widen_matches(
            PY,
            "m.py",
            &[10],
            vec![LineRange { start: 10, end: 10 }],
            &mut warnings,
            &mut vec![],
            &mut vec![],
        );
        assert_eq!(windows, vec![LineRange { start: 8, end: 11 }]);
    }
}
