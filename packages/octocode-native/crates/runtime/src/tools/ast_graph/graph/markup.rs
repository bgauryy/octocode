//! Markup files (Vue, Svelte, Astro, MDX, HTML) have no grammar, so they are
//! no graph nodes. What their scripts import still depends on scanned code, so
//! the scan counts them and each import of a scanned file is recorded against
//! that file: a `dependents` answer states it and is never silently empty.
use super::*;
use octocode_engine::signatures::markup::{MarkupScripts, markup_scripts};
use octocode_engine::text::{Decoding, extension_of, read_source};
use octocode_engine::types::FileSystemEntry;

/// The scripts of one walked markup file. A file the scan cannot read may
/// hold a script it did not see, so it counts as holding one.
fn read_scripts(entry: &FileSystemEntry, relative: &str, extension: &str) -> MarkupScripts {
    let unread = MarkupScripts {
        has_script: true,
        imports: Vec::new(),
    };
    let Ok(source) = read_source(
        Path::new(&entry.path),
        u64::from(octocode_engine::fact_scan::MAX_GRAPH_SOURCE_BYTES),
    ) else {
        return unread;
    };
    match source.admit(Decoding::Unicode) {
        Ok(decoded) => markup_scripts(&decoded.text, relative, extension),
        Err(_) => unread,
    }
}

pub(super) fn link_markup(
    b: &mut BuiltGraph,
    inputs: &LinkInputs<'_>,
    mut entries: Vec<FileSystemEntry>,
    cancel: &dyn CancellationCheck,
) -> Result<(), ToolError> {
    let width = crate::tools::workers::workers(entries.len(), None);
    let read = crate::tools::workers::each_parallel(&mut entries, width, |entry| {
        cancel.check().ok()?;
        let relative = normalize(&entry.relative_path);
        let extension = extension_of(&relative, true, "");
        let scripts = read_scripts(entry, &relative, &extension);
        Some((relative, extension, scripts))
    });
    cancel.check().map_err(ToolError::cancelled)?;
    for (relative, extension, scripts) in read.into_iter().flatten() {
        // `parserGlobs` can map a markup file to a grammar: then it is a node.
        if !scripts.has_script || inputs.known.contains(&relative) {
            continue;
        }
        *b.markup_not_linked.entry(extension).or_default() += 1;
        for import in scripts.imports {
            let Some(target) = resolve(
                &import.specifier,
                &relative,
                Linker::JsTs,
                None,
                &import.imported,
                inputs.known,
                inputs.cargo_crates,
                inputs.ctx,
            )
            .filter(|target| inputs.known.contains(target)) else {
                continue;
            };
            let message = format!(
                "A script in {relative} imports {:?}, which names {target}; markup files are not graph nodes, so it is not a dependent edge.",
                import.specifier
            );
            b.diagnostics.push(Diagnostic {
                target: Some(target),
                ..Diagnostic::new(
                    relative.as_str(),
                    Some(import.line),
                    DiagnosticCode::UnlinkedPathImport,
                    sanitize(inputs.security, &message),
                )
            });
        }
    }
    Ok(())
}
