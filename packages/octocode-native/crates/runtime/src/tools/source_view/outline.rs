//! The `view:"symbols"` view: a file's declaration outline.

use super::prepare::*;
use crate::security::scan::ContentScan;
use octocode_engine::signatures::OutlineUnavailable;

/// A request's rendered outline and the source lines its redaction changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SymbolsView {
    pub(crate) outline: Result<String, OutlineUnavailable>,
    pub(crate) warning: Option<String>,
    pub(crate) redacted_lines: Vec<(usize, usize)>,
    /// The outline was rendered from the redacted source.
    pub(crate) sanitized: bool,
}

/// The outline of `source` with its declaration spans, or why it has none,
/// and the recovered-parse note, produced from this request's source.
/// The outline's lines are the source's own: no whitespace pass, which
/// would join tokens (`boolean{`, `string,...args`).
///
/// Redact the source before rendering. A source the scanner refuses renders
/// unredacted for this page only; the output stage scans what it returns.
pub(crate) fn symbols_view(
    source: &str,
    path: &str,
    source_path: &std::path::Path,
    security: &impl ContentScan,
) -> SymbolsView {
    // Past the parse limit only a document format has an outline, and the
    // compact fallback never reads this redaction: skip both on every page.
    if source.len() > octocode_engine::signatures::MAX_PARSE_SIZE
        && crate::content::DocumentFormat::of(path.split(['?', '#']).next().unwrap_or(path))
            .is_none()
    {
        return SymbolsView {
            outline: Err(OutlineUnavailable::TooLarge),
            warning: None,
            redacted_lines: Vec::new(),
            sanitized: false,
        };
    }
    match crate::security::scan::redact_lines(security, source, source_path) {
        Ok((redacted, changed)) => {
            let redacted_lines = if changed {
                redacted_lines(source, &redacted)
            } else {
                Vec::new()
            };
            SymbolsView {
                sanitized: true,
                ..render_symbols_view(&redacted, path, redacted_lines)
            }
        }
        Err(_) => render_symbols_view(source, path, Vec::new()),
    }
}

/// The symbols view of `source`, which the caller has redacted.
fn render_symbols_view(
    source: &str,
    path: &str,
    redacted_lines: Vec<(usize, usize)>,
) -> SymbolsView {
    // The lines holding a placeholder, whichever pass redacted them (the
    // whole-file key blocks before this view, or the line pass above).
    let redacted = source
        .lines()
        .enumerate()
        .filter(|(_, line)| octocode_engine::security::placeholder_regex().is_match(line))
        .map(|(index, _)| index + 1)
        .collect::<Vec<_>>();
    // Both projections borrow the declaration document from this extraction;
    // no lookup, second parse, or retained global result joins them.
    let (signatures, declarations) =
        octocode_engine::signatures::extract_signatures_with_declarations(source, path);
    let code = |outline: String| {
        let outline = declaration_ranges(
            crate::response::numbered::tab_gutter(&outline),
            declarations.as_ref(),
        );
        (
            Ok(outline),
            super::block::outline_recovery_warning(declarations.as_ref(), path, &redacted),
        )
    };
    let (outline, warning) = match signatures {
        Ok(outline) => code(outline),
        Err(reason) => match crate::content::markdown_heading_outline(source, path) {
            Some(headings) => code(headings),
            None
                if reason == OutlineUnavailable::UnsupportedLanguage
                    && crate::content::DocumentFormat::of(
                        path.split(['?', '#']).next().unwrap_or(path),
                    ) == Some(crate::content::DocumentFormat::Markdown) =>
            {
                (Err(OutlineUnavailable::NoDeclarations), None)
            }
            // A data document lists its keys, gutters and spans included.
            None => match super::key_outline::key_outline(source, path) {
                Some(keys) if keys.text.len() < source.len() => (
                    Ok(keys.text),
                    keys.unparsed.map(|lines| {
                        let noun = if lines.contains([',', '-']) { "lines" } else { "line" };
                        format!(
                            "The parser could not parse {noun} {lines} of this file and resumed after them, so this outline may miss keys or tables there."
                        )
                    }),
                ),
                Some(_) => (Err(OutlineUnavailable::NotSmaller), None),
                None => (Err(reason), None),
            },
        },
    };
    SymbolsView {
        outline,
        warning,
        redacted_lines,
        sanitized: false,
    }
}

/// An outline whose declaration heads carry their line span: the gutter
/// of a multi-line declaration's head row reads `head-end`, so a body is one
/// `ranges` read away. The head row is the one holding the declaration's
/// name (a class, method or field line, not the annotation or decorator
/// rows a declaration may start with: those stay plain rows above it, in
/// every grammar), else its first shown row. The gutter is metadata, not
/// source text; the TAB still separates the source.
fn declaration_ranges(
    outline: String,
    document: Option<&octocode_engine::graph::GraphFactsDocument>,
) -> String {
    let Some(document) = document else {
        return outline;
    };
    let declared = document
        .declarations
        .iter()
        .map(crate::tools::declarations::Declared::of)
        .filter(|declaration| declaration.end > declaration.start);
    let row_line = |record: &str| {
        record
            .split_once(crate::response::numbered::SEPARATOR)
            .and_then(|(number, _)| number.parse::<usize>().ok())
    };
    let mut rows: Vec<usize> = outline.lines().filter_map(row_line).collect();
    rows.sort_unstable();
    let mut ends = std::collections::HashMap::<usize, usize>::new();
    for declaration in declared {
        let head = if rows.binary_search(&declaration.line).is_ok() {
            Some(declaration.line)
        } else {
            rows.get(rows.partition_point(|row| *row < declaration.start))
                .copied()
                .filter(|row| *row <= declaration.end)
        };
        if let Some(head) = head.filter(|head| *head < declaration.end) {
            let entry = ends.entry(head).or_insert(declaration.end);
            *entry = (*entry).max(declaration.end);
        }
    }
    let mut out = String::with_capacity(outline.len() + ends.len() * 6);
    for record in outline.split_inclusive('\n') {
        match row_line(record).and_then(|line| Some((line, ends.remove(&line)?))) {
            Some((line, end)) => {
                out.push_str(&format!("{line}-{end}"));
                out.push_str(&record[line.to_string().len()..]);
            }
            None => out.push_str(record),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_without_atx_headings_reports_no_declarations() {
        for path in ["README.md", "guide.MARKDOWN", "page.mdx"] {
            for source in [
                "",
                "Ordinary prose.\n",
                "Title\n=====\n",
                "```\n# Code\n```\n",
            ] {
                let view = render_symbols_view(source, path, Vec::new());
                assert_eq!(
                    view.outline,
                    Err(OutlineUnavailable::NoDeclarations),
                    "{path}: {source:?}"
                );
            }
        }
        assert_eq!(
            render_symbols_view("Ordinary prose.\n", "notes.txt", Vec::new()).outline,
            Err(OutlineUnavailable::UnsupportedLanguage)
        );
    }
}
