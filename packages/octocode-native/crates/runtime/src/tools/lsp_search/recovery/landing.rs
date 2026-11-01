//! Where a definition location lands, by the declarations parse of its
//! file: an alias binding (the chain hops on) or a declaration (it ends).
use super::{LspFailure, spans};
use crate::execution::cancel::CancellationCheck;

/// What a definition location starts, by the declarations parse of its
/// file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) enum Landing {
    /// A name bound by an import (`import { f }`, `use a::f as g`) or an
    /// export specifier (`export { f }`, `export { f } from`): an alias
    /// whose declaration lies elsewhere.
    Alias,
    /// The name (or the start) of a declaration of the file.
    Declaration,
    /// Neither, or a file without a declarations parse.
    Unknown,
}

/// The [`Landing`] at zero-based `(line, character)` of `path`, read from
/// the digest-cached declarations parse, off the async workers.
pub(in super::super) async fn landing(
    cancel: &dyn CancellationCheck,
    path: &str,
    source: &std::sync::Arc<super::super::source::Source>,
    line: u32,
    character: u32,
) -> Result<Landing, LspFailure> {
    let (path, source) = (path.to_owned(), std::sync::Arc::clone(source));
    let landing = super::super::blocking_cancellable(cancel, move |_| {
        crate::tools::declarations_cache::file_document(&source.content, &path)
            .map_or(Landing::Unknown, |document| {
                lands(&document, line, character)
            })
    })
    .await?;
    Ok(landing.unwrap_or(Landing::Unknown))
}

/// [`landing`] over one parsed document. An import binding is an alias
/// even where the parse also lists it as a declaration; a declaration's
/// name is no alias even on a line the parse lists as an export (Python
/// and Rust record their module-level declarations as exports).
pub(in super::super) fn lands(
    document: &octocode_engine::graph::GraphFactsDocument,
    line: u32,
    character: u32,
) -> Landing {
    let within = |range: &octocode_engine::graph::GraphRange| {
        spans(
            (range.start.line, range.start.character),
            (range.end.line, range.end.character),
            (line, character),
        )
    };
    let starts = |range: &octocode_engine::graph::GraphRange| {
        (range.start.line, range.start.character) == (line, character)
    };
    let imported = document.imports.iter().any(|import| {
        import.local_range.as_ref().is_some_and(within)
            || import.imported_range.as_ref().is_some_and(within)
    });
    let declared = || {
        document
            .declarations
            .iter()
            .any(|declaration| starts(&declaration.selection_range) || starts(&declaration.range))
    };
    // A declaration export (`export function f`) names no other binding;
    // a specifier carries its local name or source module.
    let re_exported = || {
        document.exports.iter().any(|export| {
            (export.local_name.is_some() || export.source.is_some())
                && export.line == line.saturating_add(1)
        })
    };
    if imported {
        Landing::Alias
    } else if declared() {
        Landing::Declaration
    } else if re_exported() {
        Landing::Alias
    } else {
        Landing::Unknown
    }
}

#[cfg(test)]
mod tests;
