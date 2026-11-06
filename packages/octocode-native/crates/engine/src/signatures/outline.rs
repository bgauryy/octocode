//! The symbols outline of one file: the declarations document every outline
//! reads (astSearch symbols, localFetch symbols) rendered as source rows, or
//! the typed reason a reader falls back to another view.
use super::languages;
use crate::graph::{GraphFactDeclaration, GraphFactsDocument};
use crate::text::extension_of;

/// Why a file has no symbols outline: the typed reason a reader falls back
/// to another view.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum OutlineUnavailable {
    /// No grammar with an outline for this file type.
    UnsupportedLanguage,
    /// The parser gave no tree: it was stopped, or its work passed the
    /// linear bound.
    ParseFailed,
    /// The parse kept no declaration line.
    NoDeclarations,
    /// The outline would be no smaller than the source.
    NotSmaller,
}

impl OutlineUnavailable {
    /// The reason as the readers report it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UnsupportedLanguage => "unsupported-language",
            Self::ParseFailed => "parse-failed",
            Self::NoDeclarations => "no-declarations",
            Self::NotSmaller => "not-smaller",
        }
    }
}

/// The outline of `content` from its declarations document ([`None`]: the
/// extractor gave none), as `N\ttext` rows: each declaration's name row, and
/// its first row when annotations or decorators open it above the name
/// (marked cut when more rows lie between); the rows of a member-listing body (a class, struct, impl or module) that no
/// nested declaration spans, such as fields; and the file's import runs,
/// one summary row each. A row longer than [`OUTLINE_MAX_LINE_CHARS`] (a
/// minified or deeply nested line) lists the declaration heads it holds,
/// each to the end of its name, so every declaration keeps a row and the
/// row's line reads the rest.
pub fn declarations_outline(
    content: &str,
    file_path: &str,
    document: Option<&GraphFactsDocument>,
) -> Result<String, OutlineUnavailable> {
    let ext = extension_of(file_path, true, "txt");
    let entry = languages::find_entry(crate::text::file_extension::grammar_extension(
        content, &ext,
    ))
    .filter(|entry| entry.admits(crate::text::analysis::AnalysisOperation::Declarations));
    let (Some(entry), Some(document)) = (entry, document) else {
        return Err(if entry.is_none() && document.is_none() {
            OutlineUnavailable::UnsupportedLanguage
        } else {
            OutlineUnavailable::ParseFailed
        });
    };
    let lines: Vec<&str> = content.lines().collect();
    let rows = outline_rows(&lines, document, entry.comment_style);
    if rows.is_empty() {
        // A parse that gave no tree says so; its empty document proves no
        // absence of declarations.
        let unparsed = document
            .diagnostics
            .iter()
            .any(|note| note.code == crate::graph::GraphFactCode::ParseDeadline);
        return Err(if unparsed {
            OutlineUnavailable::ParseFailed
        } else {
            OutlineUnavailable::NoDeclarations
        });
    }
    let mut out = String::new();
    for (row, text) in rows {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&(row + 1).to_string());
        out.push('\t');
        out.push_str(&text);
    }
    // The outline the agent receives (gutter included) never inflates the
    // source: a file with few body rows reads better as itself.
    if out.len() >= content.len() {
        return Err(OutlineUnavailable::NotSmaller);
    }
    Ok(out)
}

/// The outline's `(0-based row, text)` pairs in source order.
fn outline_rows(
    lines: &[&str],
    document: &GraphFactsDocument,
    comments: languages::CommentStyle,
) -> Vec<(usize, String)> {
    let n = lines.len();
    if n == 0 {
        return Vec::new();
    }
    let row_of = |line: u32| (line as usize).min(n - 1);
    // Depth counts per row from difference arrays: linear in rows plus
    // declarations however deep they nest.
    let mut heads: Vec<Vec<&GraphFactDeclaration>> = vec![Vec::new(); n];
    // A declaration's first row above its name row, and whether rows lie
    // between them.
    let mut openers: Vec<Option<bool>> = vec![None; n];
    let mut members = vec![0i64; n + 1];
    let mut covered = vec![0i64; n + 1];
    for declaration in &document.declarations {
        let name = row_of(declaration.selection_range.start.line);
        let end = row_of(declaration.range.end.line).max(name);
        heads[name].push(declaration);
        let start = row_of(declaration.range.start.line).min(name);
        if start < name && member_row(lines[start], comments) {
            let elided = openers[start].unwrap_or(false) || name - start > 1;
            openers[start] = Some(elided);
        }
        if lists_members(&declaration.kind) {
            if name < end {
                members[name + 1] += 1;
                members[end + 1] -= 1;
            }
        } else if !holds_statements(&declaration.kind) {
            covered[start] += 1;
            covered[end + 1] -= 1;
        }
    }
    let (mut in_members, mut in_covered) = (0i64, 0i64);
    let mut shown = vec![false; n];
    for row in 0..n {
        in_members += members[row];
        in_covered += covered[row];
        shown[row] = !heads[row].is_empty()
            || openers[row].is_some()
            || (in_members > 0 && in_covered == 0 && member_row(lines[row], comments));
    }
    let imports = import_rows(document, n);
    let mut rows = Vec::new();
    let mut at = 0;
    while at < n {
        if imports[at] && !shown[at] {
            // A run of imports with no other outline row inside reads as
            // one row naming its line span.
            let mut last = at;
            let mut count = 1;
            let mut next = at + 1;
            while next < n && !shown[next] {
                if imports[next] {
                    last = next;
                    count += 1;
                }
                next += 1;
            }
            let text = if count > 1 {
                let word = lines[at].split_whitespace().next().unwrap_or("import");
                format!("{word}{ELIDED} (lines {}-{})", at + 1, last + 1)
            } else {
                row_text(lines[at], &[])
            };
            rows.push((at, text));
            at = last + 1;
            continue;
        }
        if shown[at] {
            let mut text = row_text(lines[at], &heads[at]);
            if heads[at].is_empty() && openers[at] == Some(true) && !text.ends_with(ELIDED) {
                text.push_str(ELIDED);
            }
            rows.push((at, text));
        }
        at += 1;
    }
    rows
}

/// Rows holding a file-level import, by 0-based row.
fn import_rows(document: &GraphFactsDocument, n: usize) -> Vec<bool> {
    let mut rows = vec![false; n];
    for import in document
        .imports
        .iter()
        .filter(|import| import.scope_id.is_none())
    {
        let spans = [Some(import.line.saturating_sub(1))]
            .into_iter()
            .chain(
                [import.imported_range.as_ref(), import.local_range.as_ref()]
                    .into_iter()
                    .flatten()
                    .flat_map(|range| [Some(range.start.line), Some(range.end.line)]),
            )
            .flatten();
        for line in spans {
            if let Some(row) = rows.get_mut(line as usize) {
                *row = true;
            }
        }
    }
    rows
}

/// Whether `kind` declares a body of members rather than statements: such a
/// body's own rows (fields, variants, attributes) are part of an outline.
fn lists_members(kind: &str) -> bool {
    super::TYPE_DECLARATION_KINDS.contains(&kind) || kind == "impl"
}

/// Whether `kind` declares a body of statements, as a file is: a module or
/// namespace. Its outline is the declarations inside it, which a function's
/// body (covered whole) never shows.
fn holds_statements(kind: &str) -> bool {
    matches!(kind, "module" | "namespace")
}

/// Whether a row of a member-listing body says something an outline lists:
/// not blank, not a comment, not only closing delimiters.
fn member_row(line: &str, comments: languages::CommentStyle) -> bool {
    let text = line.trim();
    // Closing delimiters, alone or before a comment (`}  // namespace x`).
    let after = text.trim_start_matches(|c: char| {
        matches!(c, '}' | ')' | ']' | ';' | ',') || c.is_whitespace()
    });
    let closing = after.len() < text.len()
        && (after.is_empty() || super::renderer::is_pure_comment(after, comments));
    !text.is_empty() && !super::renderer::is_pure_comment(text, comments) && !closing
}

/// A row as the outline shows it: the source's own text, or, past
/// [`OUTLINE_MAX_LINE_CHARS`], the heads of the declarations named on it.
fn row_text(line: &str, heads: &[&GraphFactDeclaration]) -> String {
    let line = line.trim_end();
    if line.chars().count() <= OUTLINE_MAX_LINE_CHARS {
        return line.to_owned();
    }
    let mut heads = heads.to_vec();
    heads.sort_by_key(|declaration| declaration.selection_range.start.character);
    // One index of the row serves every declaration on it.
    let index = crate::text::LineIndex::tree_sitter(line);
    let utf16_byte = |column: u32| index.position_to_byte(line, 0, column) as usize;
    let mut text = String::new();
    for declaration in heads {
        let name_start = utf16_byte(declaration.selection_range.start.character);
        let name_end =
            if declaration.selection_range.end.line == declaration.selection_range.start.line {
                utf16_byte(declaration.selection_range.end.character)
            } else {
                line.len()
            }
            .max(name_start);
        let start = if declaration.range.start.line == declaration.selection_range.start.line {
            utf16_byte(declaration.range.start.character).min(name_start)
        } else {
            0
        };
        let head = &line[start..name_end];
        let head = if head.chars().count() > OUTLINE_MAX_LINE_CHARS {
            &line[name_start..name_end]
        } else {
            head
        };
        if !text.is_empty() {
            text.push_str(ELIDED);
            text.push(' ');
        }
        text.push_str(head.trim());
    }
    if text.is_empty() {
        text.extend(line.chars().take(OUTLINE_MAX_LINE_CHARS));
    }
    text.push_str(ELIDED);
    text
}

/// Marks a row the outline cut: the row's line range reads the rest.
const ELIDED: &str = " …";

/// Whether `line` is a whole-line comment in the comment syntax of the
/// grammar that reads `file_path`; `None` when no grammar reads it.
#[must_use]
pub fn is_comment_line(file_path: &str, line: &str) -> Option<bool> {
    let ext = extension_of(file_path, true, "txt");
    let entry = languages::find_entry(&ext)?;
    Some(super::renderer::is_pure_comment(line, entry.comment_style))
}

/// Longest outline row shown as its source text. Hand-written declaration
/// lines effectively never reach this; a longer row (minified or bundled
/// source, a deeply nested expression) lists its declaration heads instead.
pub(super) const OUTLINE_MAX_LINE_CHARS: usize = 1000;

/// The outline of `content` read from its own declarations document.
#[cfg(test)]
pub(crate) fn extract_signatures_inner(
    content: &str,
    file_path: &str,
) -> Result<String, OutlineUnavailable> {
    declarations_outline(
        content,
        file_path,
        super::extract_declarations_document(content, file_path).as_ref(),
    )
}
