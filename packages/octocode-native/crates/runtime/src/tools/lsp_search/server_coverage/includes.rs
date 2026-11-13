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
/// loads it) or `buildScriptsOff` (with its rerun), the latter only when a
/// missed file is not one a target gate leaves out (`target_gated`). A
/// reason's meaning is core's; a note names only this answer's files, said
/// once ([`flag_noted`]).
pub(super) fn annotate_includes(
    row: &mut Value,
    query: &LspSearchQuery,
    anchor: &str,
    scope: &Scope,
    rust: super::super::receipt::RustAnalysis,
    target_gated: &[super::targets::TargetGated],
    stopped: &dyn Fn() -> bool,
) {
    let Some(includes) = package_includes(anchor, stopped) else {
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
            "include!d from outside this package: {shown}{}.",
            if includes.outside.len() > 1 {
                format!(" and {} more", includes.outside.len() - 1)
            } else {
                String::new()
            },
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
            "{} is declared as a module by {} files: {}.",
            shown(Path::new(anchor)),
            includes.declared_by.len(),
            includes
                .declared_by
                .iter()
                .map(|path| shown(path))
                .collect::<Vec<_>>()
                .join(", "),
        );
        flag_noted(row, query, SHARED_MODULE_REASON, note, scope);
    }
    if includes.generated.is_empty() || rust.build_scripts {
        return;
    }
    // Build-script output can hide a use only in a file nothing else
    // explains: one a target gate leaves out is missing with or without it.
    let unexplained = lsp_search_query::symbol_name(query)
        .filter(|name| !name.trim().is_empty())
        .and_then(|name| scope.text_only_files(name))
        .is_none_or(|missed| {
            missed
                .iter()
                .any(|file| !target_gated.iter().any(|gated| gated.file == *file))
        });
    if !unexplained {
        return;
    }
    let allowed = rust.build_scripts_allowed;
    // Which load hides a missed file is not known: when the outside include
    // may hide it too, the rerun (~2 GB) can recover nothing, so the text
    // check comes first and the rerun is no lead.
    let outside = lists_reason(row, OUTSIDE_INCLUDE_REASON);
    // `buildScriptsOff` is the whole statement: its meaning is core's, and
    // the rerun's cost and opt-in are the `rustContext.buildScripts` field's.
    // An offered rerun carries the call; the `findText` lead is the text
    // check.
    flag_partial(row, query, BUILD_SCRIPTS_REASON, None, scope);
    // The rerun repeats the whole walk: offered once, on its first page,
    // and only where the reason holds alone.
    if allowed
        && !continues_a_page(query)
        && !outside
        && lists_reason(row, BUILD_SCRIPTS_REASON)
        && let Some(rerun) = super::super::failure::next_action(query, BUILD_SCRIPTS_REASON)
    {
        crate::response::result::attach_related(row, rerun);
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
    let Some((name, files)) = lsp_search_query::symbol_name(query)
        .filter(|name| !name.trim().is_empty())
        .and_then(|name| Some((name, scope.text_only_files(name)?)))
    else {
        return;
    };
    super::super::failure::record_text_only(row, scope, name, files.len());
}

/// The includes of the Cargo package `anchor` belongs to: every package
/// file, until the request stops the scan (`has_more`).
pub(in super::super) fn package_includes(
    anchor: &str,
    stopped: &dyn Fn() -> bool,
) -> Option<octocode_engine::lsp::workspace::PackageIncludes> {
    octocode_engine::lsp::workspace::cargo_package_includes(Path::new(anchor), stopped)
}

/// Where an outside `include!` declares a name: the include target and the
/// package that includes it.
#[derive(Debug)]
pub(in super::super) struct IncludedDeclaration {
    pub(in super::super) target: std::path::PathBuf,
    pub(in super::super) package: String,
}

/// The outside `include!` target that declares `name`, of the package of
/// `anchor` or of a package it reaches through local path dependencies
/// (the anchor's own first): the crate that defines a name decides why it
/// has no location, not the anchor's crate. `None` when no such target
/// declares it, or `stopped` ended the look.
pub(in super::super) fn include_declaring(
    anchor: &str,
    name: &str,
    stopped: &dyn Fn() -> bool,
) -> Option<IncludedDeclaration> {
    let start = super::super::scope::package_of(Path::new(anchor))?;
    for package in super::super::scope::local_packages(&start) {
        if stopped() {
            return None;
        }
        let Some(includes) =
            package_includes(&package.join("Cargo.toml").to_string_lossy(), stopped)
        else {
            continue;
        };
        for target in includes.outside {
            if declares(&target, name) {
                return Some(IncludedDeclaration {
                    target,
                    package: super::super::scope::package_name(&package)?,
                });
            }
        }
    }
    None
}

/// Whether a source file of the Cargo package `anchor` belongs to declares
/// `name`: a name the package itself declares is not one a build script
/// generates. `false` when the package is unknown or `stopped` ended the look
/// (no evidence that it declares the name).
pub(in super::super) fn package_declares(
    anchor: &str,
    name: &str,
    stopped: &dyn Fn() -> bool,
) -> bool {
    let Some(package) = super::super::scope::package_of(Path::new(anchor)) else {
        return false;
    };
    let walker = crate::tools::package_walk(&package).build();
    for entry in walker.flatten() {
        if stopped() {
            return false;
        }
        let path = entry.path();
        if entry.file_type().is_some_and(|kind| kind.is_file())
            && path.extension().is_some_and(|ext| ext == "rs")
            && declares(path, name)
        {
            return true;
        }
    }
    false
}

/// Whether the Rust source at `file` declares `name` (a read or parse that
/// fails declares nothing).
fn declares(file: &Path, name: &str) -> bool {
    let Ok(bytes) =
        crate::tools::source::read_bounded(file, crate::security::SourceLimits::REAL.whole_source)
    else {
        return false;
    };
    let Ok(text) = super::super::source::admitted_text(&bytes) else {
        return false;
    };
    // Most targets never spell the name: they are not parsed.
    super::super::words::spells_word(text.as_bytes(), name.as_bytes(), true, true)
        && crate::tools::declarations::file_document(&text, &file.to_string_lossy())
            .is_some_and(|facts| facts.declarations.iter().any(|item| item.name == name))
}
