//! The `view:"symbols"` view: a file's declaration outline.

use octocode_engine::signatures::OutlineUnavailable;

/// A request's rendered outline, or why it has none, and the
/// recovered-parse note.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SymbolsView {
    pub(crate) outline: Result<String, OutlineUnavailable>,
    pub(crate) warning: Option<String>,
}

/// The symbols view of `source`, which the caller has redacted (a read's
/// selection is cut from its source redacted whole), with its declaration
/// spans. The outline's lines are the source's own: no whitespace pass,
/// which would join tokens (`boolean{`, `string,...args`).
pub(super) fn render_symbols_view(source: &str, path: &str) -> SymbolsView {
    // The one outline every reader shows: astSearch symbols lists this
    // declarations document's rows, this view renders them as source rows.
    // Both projections borrow the document of this request's own parse.
    // A parse's memory follows the source's tokens, not its bytes: a source
    // whose estimate passes the memory budget (a one-line minified file) is
    // not parsed, like one the parser gave no tree for.
    let parsable = octocode_engine::structural::parse_cost(source.as_bytes())
        <= octocode_engine::structural::parse_budget();
    let declarations = parsable
        .then(|| crate::declarations::file_document(source, path))
        .flatten();
    let declarations = declarations.as_deref();
    // The lines holding a placeholder (the source's redaction put them in)
    // say which skipped span is redacted text: only a recovered parse has
    // such a span, so a clean parse never scans the source's lines for them.
    let redacted = if declarations.is_some_and(|document| !document.error_lines.is_empty()) {
        source
            .lines()
            .enumerate()
            .filter(|(_, line)| octocode_engine::security::placeholder_regex().is_match(line))
            .map(|(index, _)| index + 1)
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let signatures = if parsable {
        octocode_engine::signatures::declarations_outline(source, path, declarations)
    } else {
        Err(OutlineUnavailable::ParseFailed)
    };
    let code = |outline: String| {
        let outline = declaration_ranges(
            octocode_runtime_base::response::numbered::tab_gutter(&outline),
            declarations,
        );
        (
            Ok(outline),
            super::block::outline_recovery_warning(declarations, path, &redacted),
        )
    };
    let (outline, warning) = match signatures {
        Ok(outline) => code(outline),
        Err(reason) => match octocode_runtime_base::content::markdown_heading_outline(source, path) {
            Some(headings) => code(headings),
            None
                if reason == OutlineUnavailable::UnsupportedLanguage
                    && octocode_runtime_base::content::DocumentFormat::of(
                        path.split(['?', '#']).next().unwrap_or(path),
                    ) == Some(octocode_runtime_base::content::DocumentFormat::Markdown) =>
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
    SymbolsView { outline, warning }
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
        .map(crate::declarations::Declared::of)
        .filter(|declaration| declaration.end > declaration.start);
    let row_line = |record: &str| {
        record
            .split_once(octocode_runtime_base::response::numbered::SEPARATOR)
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
mod tests;
