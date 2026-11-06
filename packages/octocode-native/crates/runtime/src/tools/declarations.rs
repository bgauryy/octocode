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
use crate::tools::page_memo::PageMemo;
use octocode_engine::graph::{GraphFactCode, GraphFactsDocument};
use std::sync::Arc;

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
    /// none is kept.
    fn of(&self, source: &str, path: &str) -> Option<Arc<GraphFactsDocument>> {
        let key = self.key(source, path);
        if let Some(kept) = DOCUMENTS.get(&key, "", Clone::clone) {
            return kept;
        }
        #[cfg(test)]
        parses::note(self.name().0, path);
        let document = self.extract(source, path).map(Arc::new);
        let stopped = document.as_ref().is_some_and(|document| {
            document
                .diagnostics
                .iter()
                .any(|note| note.code == GraphFactCode::ParseDeadline)
        });
        // Cancellation is represented by `None`, just like an unsupported
        // grammar. Only the latter is a fact about this source that another
        // live request can reuse.
        let cancelled = octocode_engine::deep_stack::current_stop()
            .is_some_and(|stop| stop.load(std::sync::atomic::Ordering::Relaxed));
        if !stopped && !cancelled {
            // A document that cannot be measured is not kept: it may not be
            // small.
            let bytes = document.as_deref().map_or(0, |document| {
                octocode_engine::json_size::bytes_or(document, usize::MAX)
            });
            if bytes != usize::MAX {
                DOCUMENTS.put(key, String::new(), bytes, document.clone());
            }
        }
        document
    }
}

pub(crate) fn document(
    source: &str,
    path: &str,
    parser: Option<&str>,
) -> Option<Arc<GraphFactsDocument>> {
    Analysis::Declarations(parser).of(source, path)
}

pub(crate) fn file_document(source: &str, path: &str) -> Option<Arc<GraphFactsDocument>> {
    document(source, path, None)
}

pub(crate) fn graph_facts(source: &str, path: &str) -> Option<Arc<GraphFactsDocument>> {
    Analysis::GraphFacts.of(source, path)
}

/// The declarations of `source` whose `extends` / `implements` clause names
/// `name`, with those edges ([`octocode_engine::signatures::extract_named_heritage`]);
/// `None` when only the whole facts can tell.
pub(crate) fn named_heritage(
    source: &str,
    path: &str,
    name: &str,
) -> Option<Arc<GraphFactsDocument>> {
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

#[cfg(test)]
mod tests;

/// Test-only count of the extractions ([`Analysis`]) of each path: parses,
/// not reads of a kept document.
#[cfg(test)]
pub(crate) mod parses {
    use std::collections::HashMap;
    use std::sync::{LazyLock, Mutex, PoisonError};

    static PARSES: LazyLock<Mutex<HashMap<(&'static str, String), usize>>> =
        LazyLock::new(Mutex::default);

    pub(super) fn note(analysis: &'static str, path: &str) {
        *PARSES
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry((analysis, path.to_owned()))
            .or_default() += 1;
    }

    /// Extractions of `path` by the analysis named `analysis`
    /// (`declarations`, `graphFacts`, `namedHeritage`).
    pub(crate) fn of(analysis: &'static str, path: &str) -> usize {
        PARSES
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&(analysis, path.to_owned()))
            .copied()
            .unwrap_or(0)
    }

    /// Whole-file facts parses ([`super::graph_facts`]) of `path`.
    pub(crate) fn facts_of(path: &str) -> usize {
        of("graphFacts", path)
    }
}
