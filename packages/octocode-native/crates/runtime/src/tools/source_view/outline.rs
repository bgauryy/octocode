//! The `minify:"symbols"` view: a file's declaration outline.

use super::prepare::*;
use crate::security::scan::ContentScan;
use octocode_engine::signatures::OutlineUnavailable;

/// The outline of `source` with its declaration spans, or why it has none,
/// and the recovered-parse note: one render per source version, shared by
/// every page of the outline (across processes under persistent storage).
/// The outline's lines are the source's own: no whitespace pass, which
/// would join tokens (`boolean{`, `string,...args`).
///
/// The view is kept (in memory and on disk), so it renders from the source
/// redacted line by line first: no kept outline ever holds a secret, and a
/// kept one is served without scanning the source again. A source the
/// scanner refuses renders unredacted for this page only (each page scans
/// what it returns) and is never kept.
pub(crate) fn symbols_view(
    store: &crate::cache::Store<crate::tools::declarations_cache::SymbolsView>,
    source: &str,
    path: &str,
    source_path: &std::path::Path,
    security: &impl ContentScan,
) -> std::sync::Arc<crate::tools::declarations_cache::SymbolsView> {
    crate::tools::declarations_cache::symbols_view(store, source, path, || {
        match crate::security::scan::redact_lines(security, source, source_path) {
            Ok((redacted, changed)) => {
                let redacted_lines = if changed {
                    redacted_lines(source, &redacted)
                } else {
                    Vec::new()
                };
                Ok(render_symbols_view(&redacted, path, redacted_lines))
            }
            Err(_) => Err(render_symbols_view(source, path, Vec::new())),
        }
    })
}

/// The symbols view of `source`, which the caller has redacted.
fn render_symbols_view(
    source: &str,
    path: &str,
    redacted_lines: Vec<(usize, usize)>,
) -> crate::tools::declarations_cache::SymbolsView {
    // The lines holding a placeholder, whichever pass redacted them (the
    // whole-file key blocks before this view, or the line pass above).
    let redacted = source
        .lines()
        .enumerate()
        .filter(|(_, line)| octocode_engine::security::placeholder_regex().is_match(line))
        .map(|(index, _)| index + 1)
        .collect::<Vec<_>>();
    let code = |outline: String| {
        let outline = declaration_ranges(
            crate::response::numbered::tab_gutter(&outline),
            source,
            path,
        );
        (
            Ok(outline),
            super::block::outline_recovery_warning(source, path, &redacted),
        )
    };
    // One parse serves the outline and the declarations its spans and
    // recovery note read: the declarations seed the shared cache those
    // readers take them from.
    let (signatures, declarations) =
        octocode_engine::signatures::extract_signatures_with_declarations(source, path);
    if let Some(declarations) = declarations {
        let _ =
            crate::tools::declarations_cache::document(source, path, None, || Some(declarations));
    }
    let (outline, warning) = match signatures {
        Ok(outline) => code(outline),
        Err(reason) => match crate::content::markdown_heading_outline(source, path) {
            Some(headings) => code(headings),
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
    crate::tools::declarations_cache::SymbolsView {
        outline,
        warning,
        redacted_lines,
    }
}

/// An outline whose declaration heads carry their line span: the gutter
/// of a multi-line declaration's head row reads `head-end`, so a body is one
/// `ranges` read away. The head row is the one holding the declaration's
/// name (a class, method or field line, not the annotation or decorator
/// rows a declaration may start with: those stay plain rows above it, in
/// every grammar), else its first shown row. The gutter is metadata, not
/// source text; the TAB still separates the source.
fn declaration_ranges(outline: String, source: &str, path: &str) -> String {
    let Some(declared) = super::block::declarations(source, path) else {
        return outline;
    };
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
