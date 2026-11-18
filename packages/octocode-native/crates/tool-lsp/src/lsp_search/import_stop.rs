//! Where an importer candidate's definition chain stopped, and whether that
//! proves the candidate does not import the declaration.
//!
//! A chain that ends inside the candidate itself proves nothing when the
//! server merely did not follow an import. It proves the candidate does not
//! reach the declaration when it ends on a local declaration (a vendored
//! copy, an interface property of the same name) or on an import whose
//! relative module path names no file. Everything else stays unverified.

use octocode_engine::text::lexical;
use octocode_runtime_base::policy::PolicyErrorCode;
use octocode_runtime_base::policy::path::PathPolicy;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Zero-based start of an import binding in its file.
type Spot = (u32, u32);

/// A predicate that is true only for a path proven absent: a path that
/// exists, is denied by policy, or cannot be judged is never missing.
pub(super) fn proven_missing(policy: &PathPolicy) -> impl Fn(&Path) -> bool + '_ {
    move |path| {
        policy
            .validate(path)
            .is_err_and(|error| error.code == PolicyErrorCode::NotFound)
    }
}

/// Whether a definition chain from an occurrence in `file` (canonical)
/// stopped on an import binding of that file while the declaration lives in
/// another one: the server answered with the import it did not follow, which
/// proves nothing about the symbol's identity. A chain ending on a
/// declaration in `content` (a vendored copy of the symbol, a property of the
/// same name) is a different symbol, not an unfollowed import; so is one
/// ending only on imports whose relative module path resolves to no file
/// (`missing` says which paths are proven absent): they bind nothing.
pub(super) fn stays_on_import(
    resolved: &HashSet<String>,
    file: &str,
    declaration: &HashSet<String>,
    content: &str,
    missing: &dyn Fn(&Path) -> bool,
) -> bool {
    let prefix = format!("{file}:");
    let declared_here = declaration
        .iter()
        .any(|identity| identity.starts_with(&prefix));
    if declared_here
        || resolved.is_empty()
        || !resolved
            .iter()
            .all(|identity| identity.starts_with(&prefix))
    {
        return false;
    }
    let bindings = import_bindings(content, file);
    let stops = resolved
        .iter()
        .map(|identity| {
            let mut parts = identity[prefix.len()..].split(':');
            let line = parts.next().and_then(|part| part.parse::<u32>().ok());
            let character = parts.next().and_then(|part| part.parse::<u32>().ok());
            line.zip(character)
        })
        .collect::<Vec<_>>();
    // Unreadable imports or an unreadable position: unknown, so unfollowed.
    let Some(bindings) = bindings else {
        return true;
    };
    let mut any_unfollowed = false;
    for stop in stops {
        let Some(spot) = stop else {
            return true;
        };
        let Some(specifier) = bindings.get(&spot) else {
            // Not an import binding: a local declaration.
            return false;
        };
        any_unfollowed |= !resolves_nowhere(specifier, file, missing);
    }
    any_unfollowed
}

/// Zero-based start of every import's local binding in `content`, with its
/// module specifier; `None` when the file's imports cannot be read. The
/// imports come from the digest-cached declarations parse.
fn import_bindings(content: &str, file: &str) -> Option<HashMap<Spot, String>> {
    let facts = octocode_tools_shared::declarations::file_document(content, file)?;
    Some(
        facts
            .imports
            .iter()
            .filter_map(|import| {
                let start = import.local_range.as_ref()?.start.clone();
                Some(((start.line, start.character), import.specifier.clone()))
            })
            .collect(),
    )
}

/// Extensions a TypeScript/JavaScript module path may resolve to.
const MODULE_EXTENSIONS: [&str; 12] = [
    "ts", "tsx", "d.ts", "js", "jsx", "mts", "cts", "mjs", "cjs", "json", "d.mts", "d.cts",
];

/// Whether a relative `specifier` imported from `file` names no file: no
/// exact path, no compiled-name source swap, no extension probe, no directory
/// and no directory index exists (all proven by `missing`). A bare or
/// aliased specifier cannot be judged without the project's resolution
/// settings, so it is never `true`.
fn resolves_nowhere(specifier: &str, file: &str, missing: &dyn Fn(&Path) -> bool) -> bool {
    if !(specifier == "."
        || specifier == ".."
        || specifier.starts_with("./")
        || specifier.starts_with("../"))
    {
        return false;
    }
    let Some(directory) = Path::new(file).parent() else {
        return false;
    };
    let base = lexical(&directory.join(specifier));
    module_candidates(&base).iter().all(|path| missing(path))
}

/// The paths a relative module path `base` may resolve to, in resolution
/// order: the exact path, its compiled-name source swaps, each extension
/// probe and directory index, and a package manifest.
pub(super) fn module_candidates(base: &Path) -> Vec<PathBuf> {
    let name = base
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut candidates = vec![base.to_path_buf()];
    for (compiled, sources) in octocode_engine::signatures::languages::JS_SOURCE_SWAPS {
        if let Some(stem) = name.strip_suffix(&format!(".{compiled}")) {
            candidates.extend(
                sources
                    .iter()
                    .map(|extension| base.with_file_name(format!("{stem}.{extension}"))),
            );
        }
    }
    for extension in MODULE_EXTENSIONS {
        candidates.push(base.with_file_name(format!("{name}.{extension}")));
        candidates.push(base.join(format!("index.{extension}")));
    }
    candidates.push(base.join("package.json"));
    candidates
}

#[cfg(test)]
mod tests;
