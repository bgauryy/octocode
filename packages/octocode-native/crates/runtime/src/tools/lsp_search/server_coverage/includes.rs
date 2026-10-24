//! C/C++ include coverage: unloaded includes, text coverage records and the
//! package's include directories.
use super::*;

/// rust-analyzer loads a package's own directory, and its build-script
/// output (`OUT_DIR`) only when `rustContext` runs build scripts. A type
/// defined in source the package `include!`s from elsewhere is unknown, so
/// uses that reach a symbol through it (a method called through `Deref` to
/// one, a field of one) do not resolve, and the server answers without
/// them. The row is partial when files that use the name in code are
/// missing from the answer, the lexical lead names them, and the reason
/// says which load is missing: `includeOutsidePackage` (no build context
/// loads it) or `buildScriptsOff` (with its rerun). The prose is static: a
/// notice, said once ([`flag_noted`]).
pub(super) fn annotate_includes(
    row: &mut Value,
    query: &LspSearchQuery,
    anchor: &str,
    scope: &Scope,
) {
    let Some(includes) = package_includes(anchor) else {
        return;
    };
    if let Some(outside) = includes.outside.first() {
        // Named as rows name files: relative to the search scope.
        let shown = outside
            .strip_prefix(&scope.root)
            .unwrap_or(outside)
            .to_string_lossy()
            .into_owned();
        let note = format!(
            "rust-analyzer never loads {shown}{} (include!d from outside this package): uses through its types are missing under any rustContext; confirm with {}.",
            if includes.outside.len() > 1 {
                format!(" and {} more", includes.outside.len() - 1)
            } else {
                String::new()
            },
            text_check()
        );
        flag_noted(row, query, OUTSIDE_INCLUDE_REASON, note, scope);
    }
    // Several files declare the anchor as a module (`#[path]` from a bench
    // and a test): each crate compiles its own copy, and rust-analyzer
    // resolves the anchor in one, so uses of the others' copies are not
    // references of this symbol.
    if includes.declared_by.len() > 1 {
        let shown = |path: &Path| {
            path.strip_prefix(&scope.root)
                .unwrap_or(path)
                .to_string_lossy()
                .into_owned()
        };
        let note = format!(
            "{} is declared as a module by {} files ({}), so each compiles its own copy: rust-analyzer answers for one copy, and uses through the others are missing; confirm with {}.",
            shown(Path::new(anchor)),
            includes.declared_by.len(),
            includes
                .declared_by
                .iter()
                .map(|path| shown(path))
                .collect::<Vec<_>>()
                .join(", "),
            text_check()
        );
        flag_noted(row, query, SHARED_MODULE_REASON, note, scope);
    }
    if includes.generated.is_empty() || runs_build_scripts(query) {
        return;
    }
    // Which load hides a missed file is not known: when the outside include
    // may hide it too, the rerun (~2 GB) can recover nothing, so the text
    // check comes first and the rerun is no lead.
    let outside = lists_reason(row, OUTSIDE_INCLUDE_REASON);
    // The note is the one statement of the fact and of the rerun's cost and
    // risk ([`BUILD_SCRIPTS_COST`]); an offered rerun carries the call, so
    // the note names the lead instead of restating it, and the `findText`
    // lead is the text check.
    let note = if outside {
        format!(
            "rust-analyzer also did not run this package's build scripts; confirm the missing uses with {} first: rustContext {{buildScripts:true}} loads only the generated types, never the outside include, and {BUILD_SCRIPTS_COST}.",
            text_check()
        )
    } else {
        format!(
            "rust-analyzer did not run this package's build scripts, so uses through the types they generate are missing; the rerun with rustContext.buildScripts enabled {BUILD_SCRIPTS_COST}."
        )
    };
    flag_noted(row, query, BUILD_SCRIPTS_REASON, note, scope);
    // The rerun repeats the whole walk: offered once, on its first page,
    // and only where the reason holds alone.
    if !continues_a_page(query) && !outside && lists_reason(row, BUILD_SCRIPTS_REASON) {
        crate::response::result::attach_related(
            row,
            super::super::failure::build_scripts_rerun(query),
        );
    }
}

/// State the text check on every Rust incoming row, whatever its crate:
/// `coverage.textOnlyFiles` (files that use the name in code but are not in
/// the answer), so a checked, complete row reads differently from an
/// unchecked one. A workspace-symbol row lists declarations, not uses.
pub(super) fn record_text_coverage(row: &mut Value, query: &LspSearchQuery, scope: &Scope) {
    if lsp_search_query::op(query) == Op::WorkspaceSymbol {
        return;
    }
    let Some(files) = lsp_search_query::symbol_name(query)
        .filter(|name| !name.trim().is_empty())
        .and_then(|name| scope.text_only_files(name))
    else {
        return;
    };
    if let Some(payload) = row.as_object_mut() {
        payload.entry("coverage").or_insert_with(|| json!({}))["textOnlyFiles"] =
            json!(files.len());
    }
}

/// Bound of a package's include scan; a larger package reports what the
/// scanned files include.
pub(super) const MAX_INCLUDE_SCAN_FILES: usize = 5_000;

/// The includes of the Cargo package `anchor` belongs to.
pub(in super::super) fn package_includes(
    anchor: &str,
) -> Option<octocode_engine::lsp::workspace::PackageIncludes> {
    octocode_engine::lsp::workspace::cargo_package_includes(
        Path::new(anchor),
        MAX_INCLUDE_SCAN_FILES,
    )
}
