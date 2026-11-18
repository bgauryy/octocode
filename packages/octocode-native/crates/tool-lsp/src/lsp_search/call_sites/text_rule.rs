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
        && is_call_site(
            content,
            lines,
            (range.start.line, range.start.character),
            (range.end.line, range.end.character),
        )
}

/// The text rule of a site by its range: `(` follows the range, or the name
/// that starts it, since a server may range an invocation over its arguments
/// (jdtls). A range of the name alone reads the same either way.
pub(in crate::lsp_search) fn is_call_site(
    content: &str,
    lines: &LineIndex,
    start: (u32, u32),
    end: (u32, u32),
) -> bool {
    if is_call_at(content, lines, end.0, end.1) {
        return true;
    }
    let at =
        usize::try_from(lines.position_to_byte(content, start.0, start.1)).unwrap_or(content.len());
    let name = content.get(at..).map_or(0, |rest| {
        rest.find(|c: char| !octocode_engine::text::is_ident_char(c))
            .unwrap_or(rest.len())
    });
    name > 0 && opens_call(content, lines, start.0, at + name)
}

/// The text rule of a file oxc cannot parse: `(` (after optional spaces)
/// follows the UTF-16 position on its line. `lines` indexes `content`.
pub(in crate::lsp_search) fn is_call_at(
    content: &str,
    lines: &LineIndex,
    line: u32,
    character: u32,
) -> bool {
    let at =
        usize::try_from(lines.position_to_byte(content, line, character)).unwrap_or(content.len());
    opens_call(content, lines, line, at)
}

/// Whether `(` (after optional spaces) follows byte `at` on `line`.
fn opens_call(content: &str, lines: &LineIndex, line: u32, at: usize) -> bool {
    let Some(start) = usize::try_from(line)
        .ok()
        .and_then(|line| lines.line_start(line))
    else {
        return false;
    };
    let end = usize::try_from(line)
        .ok()
        .and_then(|line| lines.line_start(line + 1))
        .unwrap_or(content.len());
    content
        .get(at.max(start)..end)
        .is_some_and(|rest| rest.trim_start().starts_with('('))
}

#[cfg(test)]
mod tests;
