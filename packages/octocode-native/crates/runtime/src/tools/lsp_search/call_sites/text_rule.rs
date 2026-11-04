//! The text rule for call sites: used where oxc does not decide a site.

use super::symbols::{FlatSymbol, is_declaration_name};
use octocode_engine::lsp::types::LspRange;
use octocode_engine::text::LineIndex;

/// The text rule for a site oxc does not decide: `(` follows the reference
/// and the reference is not a declaration's own name in `outline` (a
/// method signature `read(at: number) {` is followed by `(` too). Language
/// neutral: the outline is the server's.
pub(super) fn is_text_call(
    content: &str,
    lines: &LineIndex,
    outline: &[FlatSymbol<'_>],
    range: &LspRange,
) -> bool {
    !is_declaration_name(outline, range)
        && is_call_at(content, lines, range.end.line, range.end.character)
}

/// The text rule of a file oxc cannot parse: `(` (after optional spaces)
/// follows the UTF-16 position on its line. `lines` indexes `content`.
pub(in crate::tools::lsp_search) fn is_call_at(
    content: &str,
    lines: &LineIndex,
    line: u32,
    character: u32,
) -> bool {
    let Some(start) = usize::try_from(line)
        .ok()
        .and_then(|line| lines.line_start(line))
    else {
        return false;
    };
    let at =
        usize::try_from(lines.position_to_byte(content, line, character)).unwrap_or(content.len());
    let end = usize::try_from(line)
        .ok()
        .and_then(|line| lines.line_start(line + 1))
        .unwrap_or(content.len());
    content
        .get(at.max(start)..end)
        .is_some_and(|rest| rest.trim_start().starts_with('('))
}

#[cfg(test)]
mod tests {
    use super::super::symbols::flatten_symbols;
    use super::*;
    use serde_json::json;

    #[test]
    fn the_text_rule_reads_the_sites_own_line() {
        let text_call = |content: &str, line, character| {
            is_call_at(content, &LineIndex::new(content), line, character)
        };
        assert!(text_call("  return stage (1);", 0, 14));
        // UTF-16 column after a non-BMP character before the site.
        assert!(text_call("const 😀 = 1; needle(1)", 0, 20));
        assert!(!text_call("const 😀 = 1; needle(1)", 0, 19));
        assert!(!text_call("const { stage } = x;", 0, 13));
        // The rule reads the site's own line, never the next one.
        assert!(!text_call("stage\n(1);", 0, 5));
        assert!(text_call("a;\nstage(1);", 1, 5));
    }

    /// Issue #480: an implementing method's own name is followed by `(`, but
    /// the outline names it a declaration; the call inside it stays.
    #[test]
    fn a_declaration_name_is_no_call_by_the_text_rule() {
        let content = "class Ledger {\n  read(at: number): string {\n    return this.port.read(at);\n  }\n}\n";
        let range = |line: u32, start: u32, end: u32| -> LspRange {
            serde_json::from_value(json!({
                "start": {"line": line, "character": start},
                "end": {"line": line, "character": end}
            }))
            .expect("range")
        };
        let outline = json!([{
            "name": "Ledger", "kind": 5,
            "range": {"start": {"line": 0, "character": 0}, "end": {"line": 4, "character": 1}},
            "selectionRange": {"start": {"line": 0, "character": 6}, "end": {"line": 0, "character": 12}},
            "children": [{
                "name": "read", "kind": 6,
                "range": {"start": {"line": 1, "character": 2}, "end": {"line": 3, "character": 3}},
                "selectionRange": {"start": {"line": 1, "character": 2}, "end": {"line": 1, "character": 6}}
            }]
        }]);
        let mut flat = Vec::new();
        flatten_symbols(&outline, &mut flat);
        let lines = LineIndex::new(content);
        let declaration = range(1, 2, 6);
        let call = range(2, 21, 25);
        assert!(
            is_call_at(content, &lines, 1, 6),
            "the bare text rule is fooled"
        );
        assert!(!is_text_call(content, &lines, &flat, &declaration));
        assert!(is_text_call(content, &lines, &flat, &call));
        // Without an outline (it failed) the text rule alone decides.
        assert!(is_text_call(content, &lines, &[], &declaration));
    }
}
