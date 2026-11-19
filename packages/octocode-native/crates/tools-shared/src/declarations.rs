//! Declaration facts of a source text, kept in one process-wide store.
//!
//! A document depends only on the text, the path that reads it (its grammar,
//! and the file and ids the document names) and the analysis, so it is kept
//! by the text's digest with both, in the one process budget
//! ([`PageMemo`]): a later page, a repeat request or another tool asking the
//! same of the same text reads it instead of parsing again. The binary fixes
//! the grammars and extractors, so no version outlives the process. Eviction
//! or expiry only costs a parse; a document whose parse stopped early is not
//! kept.
use crate::page_memo::{Computed, PageMemo};
use octocode_engine::graph::{GraphFactCode, GraphFactsDocument};
use std::sync::Arc;

/// Where a declaration starts before its name line, 1-based: the one rule
/// every producer that lists a declaration prints it by (astSearch
/// `symbols`, localSearch `declarations[]`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StartLines {
    /// `startLine`: the first line of its attributes or decorators, when
    /// its range starts before its name line.
    pub start_line: Option<u32>,
    /// `docStartLine`: the first line of the comment block directly above.
    pub doc_start_line: Option<u32>,
}

impl StartLines {
    /// The start lines of `declaration`.
    pub fn of(declaration: &octocode_engine::graph::GraphFactDeclaration) -> Self {
        let line = declaration.selection_range.start.line.saturating_add(1);
        let start = declaration.range.start.line.saturating_add(1);
        Self {
            start_line: (start < line).then_some(start),
            doc_start_line: declaration.doc_line.map(|doc| doc.saturating_add(1)),
        }
    }
}

/// Every analysis's documents, by [`Analysis`], digest and path; a text no
/// analysis can read is kept as `None`, so it is not tried again either.
static DOCUMENTS: PageMemo<Option<Arc<GraphFactsDocument>>> = PageMemo::new();

/// What a kept document was extracted for.
enum Analysis<'a> {
    /// Declarations only, with the path's grammar or the named one.
    Declarations(Option<&'a str>),
    /// The whole graph facts.
    GraphFacts,
    /// The declarations whose `extends` / `implements` clause names this
    /// type, read from the parts of the file that spell it.
    NamedHeritage(&'a str),
}

impl Analysis<'_> {
    fn extract(&self, source: &str, path: &str) -> Option<GraphFactsDocument> {
        use octocode_engine::signatures as engine;
        match self {
            Self::Declarations(Some(extension)) => {
                engine::extract_declarations_document_with_extension(source, path, extension)
            }
            Self::Declarations(None) => engine::extract_declarations_document(source, path),
            Self::GraphFacts => engine::extract_graph_facts(source, path),
            Self::NamedHeritage(name) => engine::extract_named_heritage(source, path, name),
        }
    }

    /// The analysis's name and its argument (the parser, the type name).
    fn name(&self) -> (&'static str, &str) {
        match self {
            Self::Declarations(parser) => ("declarations", parser.unwrap_or_default()),
            Self::GraphFacts => ("graphFacts", ""),
            Self::NamedHeritage(name) => ("namedHeritage", *name),
        }
    }

    /// The store key: the analysis, the text's digest and the path (each
    /// part's length first, so no two keys share a spelling).
    fn key(&self, source: &str, path: &str) -> String {
        let (analysis, argument) = self.name();
        let digest = octocode_engine::digest::sha256(source.as_bytes());
        format!(
            "{analysis}:{}:{argument}:{digest}:{}:{path}",
            argument.len(),
            path.len()
        )
    }

    /// The kept document of `source` read from `path`, extracted first when
    /// none is kept. Concurrent requests for one document (the rows of one
    /// batch) extract it once ([`PageMemo::get_or_compute`]).
    fn of(&self, source: &str, path: &str) -> Option<Arc<GraphFactsDocument>> {
        let stopped = || {
            octocode_engine::deep_stack::current_stop()
                .is_some_and(|stop| stop.load(std::sync::atomic::Ordering::Relaxed))
        };
        DOCUMENTS.get_or_compute(self.key(source, path), "", &stopped, || {
            #[cfg(any(test, feature = "test-support"))]
            parses::note(self.name().0, path);
            let document = self.extract(source, path).map(Arc::new);
            let deadline = document.as_ref().is_some_and(|document| {
                document
                    .diagnostics
                    .iter()
                    .any(|note| note.code == GraphFactCode::ParseDeadline)
            });
            // Cancellation is represented by `None`, just like an unsupported
            // grammar. Only the latter is a fact about this source that another
            // live request can reuse. A document that cannot be measured is
            // not kept: it may not be small.
            let keep = (!deadline && !stopped())
                .then(|| {
                    document.as_deref().map_or(0, |document| {
                        octocode_engine::json_size::bytes_or(document, usize::MAX)
                    })
                })
                .filter(|bytes| *bytes != usize::MAX);
            Computed {
                value: document,
                keep,
            }
        })
    }
}

pub fn document(source: &str, path: &str, parser: Option<&str>) -> Option<Arc<GraphFactsDocument>> {
    Analysis::Declarations(parser).of(source, path)
}

pub fn file_document(source: &str, path: &str) -> Option<Arc<GraphFactsDocument>> {
    document(source, path, None)
}

pub fn graph_facts(source: &str, path: &str) -> Option<Arc<GraphFactsDocument>> {
    Analysis::GraphFacts.of(source, path)
}

/// The declarations of `source` whose `extends` / `implements` clause names
/// `name`, with those edges ([`octocode_engine::signatures::extract_named_heritage`]);
/// `None` when only the whole facts can tell.
pub fn named_heritage(source: &str, path: &str, name: &str) -> Option<Arc<GraphFactsDocument>> {
    Analysis::NamedHeritage(name).of(source, path)
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

/// The innermost of `declared` containing `line`: the shortest span.
pub(crate) fn innermost<'a>(
    declared: impl IntoIterator<Item = &'a Declared>,
    line: usize,
) -> Option<&'a Declared> {
    innermost_by(declared, line, |d| (d.start, d.end))
}

/// The innermost of `items` whose inclusive `span` contains `line`: the
/// shortest one.
pub(crate) fn innermost_by<'a, T>(
    items: impl IntoIterator<Item = &'a T>,
    line: usize,
    span: impl Fn(&T) -> (usize, usize),
) -> Option<&'a T> {
    items
        .into_iter()
        .filter(|item| {
            let (start, end) = span(item);
            start <= line && line <= end
        })
        .min_by_key(|item| {
            let (start, end) = span(item);
            end - start
        })
}

#[cfg(test)]
mod tests;

/// Test-only count of the extractions ([`Analysis`]) of each path: parses,
/// not reads of a kept document.
#[cfg(any(test, feature = "test-support"))]
pub mod parses;
