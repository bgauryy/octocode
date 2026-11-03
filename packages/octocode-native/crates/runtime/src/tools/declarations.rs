//! Request-owned declaration facts. Callers pass one document to every
//! projection that needs it; no parse or rendered answer survives a request.
use octocode_engine::graph::GraphFactsDocument;
use std::sync::Arc;

pub(crate) fn document(
    source: &str,
    path: &str,
    parser: Option<&str>,
) -> Option<Arc<GraphFactsDocument>> {
    match parser {
        Some(extension) => {
            octocode_engine::signatures::extract_declarations_document_with_extension(
                source, path, extension,
            )
        }
        None => octocode_engine::signatures::extract_declarations_document(source, path),
    }
    .map(Arc::new)
}

pub(crate) fn file_document(source: &str, path: &str) -> Option<Arc<GraphFactsDocument>> {
    document(source, path, None)
}

pub(crate) fn graph_facts(source: &str, path: &str) -> Option<Arc<GraphFactsDocument>> {
    octocode_engine::signatures::extract_graph_facts(source, path).map(Arc::new)
}

/// One declaration of the engine outline, 1-based lines.
pub(crate) struct Declared {
    pub(crate) name: String,
    pub(crate) kind: String,
    pub(crate) doc_start: Option<usize>,
    /// Its name line.
    pub(crate) line: usize,
    pub(crate) start: usize,
    pub(crate) end: usize,
}

impl Declared {
    pub(crate) fn of(declaration: &octocode_engine::graph::GraphFactDeclaration) -> Self {
        let line = |zero_based: u32| zero_based as usize + 1;
        Self {
            name: declaration.name.clone(),
            kind: declaration.kind.clone(),
            doc_start: declaration.doc_line.map(line),
            line: line(declaration.selection_range.start.line),
            start: line(declaration.range.start.line),
            end: line(declaration.range.end.line),
        }
    }
}

/// Every declaration the engine outlines, one-line ones included, in
/// outline order; `None` when the file type has no outline.
pub(crate) fn all_declarations(content: &str, path: &str) -> Option<Vec<Declared>> {
    let document = file_document(content, path)?;
    Some(document.declarations.iter().map(Declared::of).collect())
}

#[cfg(test)]
mod tests;
