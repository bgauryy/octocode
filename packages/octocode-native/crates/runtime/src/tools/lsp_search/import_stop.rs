//! Where an importer candidate's definition chain stopped, and whether that
//! proves the candidate does not import the declaration.
//!
//! A chain that ends inside the candidate itself proves nothing when the
//! server merely did not follow an import. It proves the candidate does not
//! reach the declaration when it ends on a local declaration (a vendored
//! copy, an interface property of the same name) or on an import whose
//! relative module path names no file. Everything else stays unverified.

use crate::policy::PolicyErrorCode;
use crate::policy::path::PathPolicy;
use octocode_engine::text::lexical;
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
    let facts = crate::tools::declarations_cache::file_document(content, file)?;
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
mod tests {
    use super::*;

    fn never_missing(_: &Path) -> bool {
        false
    }

    /// A chain that never left the occurrence file proves nothing when the
    /// declaration lives elsewhere; one reaching another file does.
    #[test]
    fn a_chain_stuck_in_its_own_file_is_unresolved_not_rejected() {
        let set = |items: &[&str]| {
            items
                .iter()
                .map(|s| (*s).to_owned())
                .collect::<HashSet<_>>()
        };
        let declaration = set(&["/r/strings.ts:10:16:10:35"]);
        let importing = format!("{}import {{ strings }} from './a';\n", "\n".repeat(11));
        assert!(stays_on_import(
            &set(&["/r/b.ts:11:9:11:16"]),
            "/r/b.ts",
            &declaration,
            &importing,
            &never_missing,
        ));
        // No definition at all (a mention in a string or comment) is not a
        // binding the server failed to follow.
        assert!(!stays_on_import(
            &set(&[]),
            "/r/b.ts",
            &declaration,
            "",
            &never_missing,
        ));
        assert!(!stays_on_import(
            &set(&["/r/other.ts:1:0:1:5"]),
            "/r/b.ts",
            &declaration,
            "",
            &never_missing,
        ));
        // A declaration in the candidate itself makes a same-file answer real.
        let local = set(&["/r/b.ts:3:0:3:5"]);
        assert!(!stays_on_import(
            &set(&["/r/b.ts:9:0:9:5"]),
            "/r/b.ts",
            &local,
            "",
            &never_missing,
        ));
    }

    /// A candidate that declares its own symbol of the same name (a vendored
    /// copy) resolves to that declaration: a different symbol, not an import
    /// the server failed to follow.
    #[test]
    fn a_declaration_in_the_candidate_is_rejected_while_an_import_binding_is_unresolved() {
        let set = |items: &[&str]| {
            items
                .iter()
                .map(|s| (*s).to_owned())
                .collect::<HashSet<_>>()
        };
        let declaration = set(&["/r/lifecycle.ts:564:13:564:30"]);
        let copy = "export class MutableDisposable {}\nnew MutableDisposable();\n";
        assert!(!stays_on_import(
            &set(&["/r/copy.ts:0:13:0:30"]),
            "/r/copy.ts",
            &declaration,
            copy,
            &never_missing,
        ));
        let importer = "import { MutableDisposable } from './missing';\nnew MutableDisposable();\n";
        assert!(stays_on_import(
            &set(&["/r/b.ts:0:9:0:26"]),
            "/r/b.ts",
            &declaration,
            importer,
            &never_missing,
        ));
    }

    /// A candidate whose own interface declares a property of the
    /// queried name (read through `input.name`) never imports it; the
    /// unrelated imports of the file do not make the stop an import.
    #[test]
    fn a_same_named_property_in_the_candidate_is_a_local_declaration() {
        let declaration = ["/r/lib.ts:0:16:0:36".to_owned()].into();
        let content = "import { REPORT } from './report.js';\nexport interface Input {\n  octocodeInstructions?: string;\n}\nexport const f = (input: Input) => input.octocodeInstructions + REPORT;\n";
        // The chain from `input.octocodeInstructions` ends on the property.
        let resolved = ["/r/prompt.ts:2:2:2:22".to_owned()].into();
        assert!(!stays_on_import(
            &resolved,
            "/r/prompt.ts",
            &declaration,
            content,
            &never_missing,
        ));
        // Imports that cannot be read keep the stop unverified: nothing is proven.
        let unreadable = ["/r/prompt.unknown:2:2:2:22".to_owned()].into();
        assert!(stays_on_import(
            &unreadable,
            "/r/prompt.unknown",
            &declaration,
            "import { x",
            &never_missing,
        ));
    }

    /// An import whose relative path names no file binds nothing, so the
    /// stop is settled; one that resolves (extension probe, compiled name,
    /// directory index) or cannot be judged (bare specifier) stays unverified.
    #[test]
    fn an_import_of_a_missing_relative_module_is_settled() {
        let root = tempfile::tempdir().expect("dir");
        let root = root.path().canonicalize().expect("canonical");
        let touch = |relative: &str| {
            let path = root.join(relative);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
            std::fs::write(path, "export {};\n").expect("write");
        };
        touch("src/a.ts");
        touch("src/dir/index.ts");
        touch("src/styles.css");
        touch("src/view.d.ts");
        let policy = crate::tools::test_support::workspace_policy(&root);
        let missing = proven_missing(&policy);
        let file = root.join("src/b.ts").to_string_lossy().into_owned();
        let declaration = [format!("{}:0:0:0:1", root.join("src/c.ts").display())].into();
        let stop = |specifier: &str| {
            let content = format!("import {{ X }} from '{specifier}';\nnew X();\n");
            let resolved = [format!("{file}:0:9:0:10")].into();
            stays_on_import(&resolved, &file, &declaration, &content, &missing)
        };
        assert!(!stop("./gone.js"), "no file, no source swap");
        assert!(!stop("../gone/deep.js"), "missing inside the workspace");
        assert!(
            stop("../../../../../../outside/gone"),
            "a path the policy denies cannot be judged"
        );
        assert!(stop("./a.js"), "compiled name of a.ts");
        assert!(stop("./view.jsx"), "declaration of a compiled .jsx name");
        assert!(stop("./a"), "extension probe");
        assert!(stop("./dir"), "directory index");
        assert!(stop("./styles.css"), "an exact file");
        assert!(
            stop("vs/base/common/lifecycle"),
            "bare specifiers cannot be judged"
        );
        // A path the policy denies is never proven missing.
        assert!(!missing(Path::new("/definitely/outside/the/workspace")));
    }
}
