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
    let policy = octocode_runtime_base::test_support::workspace_policy(&root);
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
