//! Where a definition location lands, by the declarations parse of its
//! file: an alias binding (the chain hops on) or a declaration (it ends).
use super::LspFailure;
use octocode_runtime_base::execution::cancel::CancellationCheck;

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
        octocode_tools_shared::declarations::file_document(&source.content, &path)
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
    // A declaration export (`export function f`) names no other binding;
    // a specifier carries its local name or source module.
    let re_exported = || {
        document.exports.iter().any(|export| {
            (export.local_name.is_some() || export.source.is_some())
                && export.line == line.saturating_add(1)
        })
    };
    match binding_role(document, line, character) {
        Some(BindingRole::Declaration) => Landing::Declaration,
        Some(_) => Landing::Alias,
        None if re_exported() => Landing::Alias,
        None => Landing::Unknown,
    }
}

/// A parsed name's role at a reference site. Only distinct source ranges
/// can identify the two sides of an explicitly aliased import.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(in super::super) enum BindingRole {
    Declaration,
    Import,
    ImportedName,
    LocalAlias,
}

pub(in super::super) fn binding_role(
    document: &octocode_engine::graph::GraphFactsDocument,
    line: u32,
    character: u32,
) -> Option<BindingRole> {
    let at = (line, character);
    let within = |range: &octocode_engine::graph::GraphRange| {
        (range.start.line, range.start.character) <= at
            && at < (range.end.line, range.end.character)
    };
    for import in &document.imports {
        let local = import.local_range.as_ref().is_some_and(within);
        let imported = import.imported_range.as_ref().is_some_and(within);
        if local || imported {
            return Some(match (local, imported) {
                (true, false) if import.imported_range.is_some() => BindingRole::LocalAlias,
                (false, true) if import.local_range.is_some() => BindingRole::ImportedName,
                _ => BindingRole::Import,
            });
        }
    }
    // An impl block refers to the type it implements.
    let starts = |r: &octocode_engine::graph::GraphRange| (r.start.line, r.start.character) == at;
    document
        .declarations
        .iter()
        .any(|d| d.kind != "impl" && (starts(&d.selection_range) || starts(&d.range)))
        .then_some(BindingRole::Declaration)
}

#[cfg(test)]
mod tests;
