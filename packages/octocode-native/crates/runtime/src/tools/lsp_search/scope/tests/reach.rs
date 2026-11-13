//! Reach: a text-scan file that spells the name counts as a missed site
//! (`textOnlyFiles`) only when it could reference the anchor's declaration;
//! the rest are out of reach, counted apart.
use super::*;
use crate::tools::lsp_search::failure::flag_partial;
use serde_json::json;

/// A Cargo workspace of `alpha` (the defining crate), `beta` (depends on
/// it), `gamma` (depends on `beta` only) and `delta` (unrelated).
fn workspace(root: &Path) {
    write(
        &root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"alpha\", \"beta\", \"gamma\", \"delta\"]\n",
    );
    write(
        &root.join("alpha/Cargo.toml"),
        "[package]\nname = \"alpha\"\nversion = \"0.1.0\"\n",
    );
    write(
        &root.join("beta/Cargo.toml"),
        "[package]\nname = \"beta\"\nversion = \"0.1.0\"\n\n[dependencies]\nalpha = { path = \"../alpha\" }\n",
    );
    write(
        &root.join("gamma/Cargo.toml"),
        "[package]\nname = \"gamma\"\nversion = \"0.1.0\"\n\n[dependencies]\nbeta = { path = \"../beta\" }\n",
    );
    write(
        &root.join("delta/Cargo.toml"),
        "[package]\nname = \"delta\"\nversion = \"0.1.0\"\n",
    );
}

fn shown(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// The scope of `root` after its text scan of `symbol`, the answer
/// `answered`, the anchor at zero-based `(line, character)` of `anchor`,
/// and the code-use check.
async fn checked(
    root: &Path,
    symbol: &str,
    anchor: (&Path, u32, u32),
    answered: &[&Path],
) -> Scope {
    let scope = Scope::new(shown(root), vec!["*.rs".into()]);
    let cancel = crate::execution::cancel::NeverCancel;
    scope
        .text_files(symbol, &workspace_policy(root), &cancel)
        .await
        .expect("scan")
        .expect("files");
    scope.anchor_site(&shown(anchor.0), anchor.1, anchor.2);
    scope.answer(answered.iter().map(|path| shown(path)));
    scope
        .check_code_uses(symbol, &cancel)
        .await
        .expect("checked");
    scope
}

fn callers(anchor: &Path, symbol: &str) -> octocode_contract_types::LspSearchQuery {
    serde_json::from_value(json!({
        "operation": "callers", "path": shown(anchor), "symbolName": symbol, "line": 1
    }))
    .expect("query")
}

/// Same-name functions of other modules and crates cannot call a
/// `pub(super)` function: out of reach, so the row is not partial.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn same_name_functions_outside_visibility_are_out_of_reach() {
    let (_dir, root) = fixture();
    workspace(&root);
    write(
        &root.join("alpha/src/lib.rs"),
        "pub mod search;\nmod other;\n",
    );
    write(
        &root.join("alpha/src/search.rs"),
        "mod executor;\nmod run;\n",
    );
    let anchor = root.join("alpha/src/search/executor.rs");
    write(&anchor, "pub(super) fn page_of() {}\n");
    let run = root.join("alpha/src/search/run.rs");
    write(&run, "fn go() {\n    super::executor::page_of();\n}\n");
    write(
        &root.join("alpha/src/other.rs"),
        "fn page_of() {}\nfn x() {\n    page_of();\n}\n",
    );
    write(
        &root.join("beta/src/lib.rs"),
        "fn page_of() {}\nfn y() {\n    page_of();\n}\n",
    );
    write(
        &root.join("delta/src/lib.rs"),
        "fn page_of() {}\nfn z() {\n    page_of();\n}\n",
    );
    let scope = checked(&root, "page_of", (&anchor, 0, 14), &[&anchor, &run]).await;
    assert_eq!(scope.text_only_files("page_of"), Some(Vec::new()));
    let out = scope.out_of_reach("page_of").expect("out of reach");
    assert_eq!(out.files, 3, "{out:?}");
    let mut row = json!({"operation": "callers"});
    flag_partial(
        &mut row,
        &callers(&anchor, "page_of"),
        crate::tools::lsp_search::server_coverage::BUILD_SCRIPTS_REASON,
        None,
        &scope,
    );
    assert!(row.get("isPartial").is_none(), "{row}");
    assert!(
        row.get("next").is_none() && row.get("related").is_none(),
        "{row}"
    );
    assert_eq!(row["coverage"]["textOnlyFiles"], 0, "{row}");
    assert_eq!(row["coverage"]["outOfReachFiles"], 3, "{row}");
}

/// A method's reach is its type's too: an inherent method of a
/// `pub(super)` type is out of reach of a dependent crate and of another
/// module, though the method itself is `pub`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_restricted_container_bounds_its_methods() {
    let (_dir, root) = fixture();
    workspace(&root);
    write(&root.join("alpha/src/lib.rs"), "mod engine;\nmod files;\n");
    write(
        &root.join("alpha/src/engine.rs"),
        "mod cursors;\nmod use_it;\n",
    );
    let anchor = root.join("alpha/src/engine/cursors.rs");
    write(
        &anchor,
        "pub(super) struct Registry;\n\nimpl Registry {\n    pub fn open(&self) {}\n}\n",
    );
    let user = root.join("alpha/src/engine/use_it.rs");
    write(
        &user,
        "fn u(registry: &super::cursors::Registry) {\n    registry.open();\n}\n",
    );
    write(
        &root.join("alpha/src/files.rs"),
        "pub fn open() {}\nfn w() {\n    open();\n}\n",
    );
    write(
        &root.join("beta/src/lib.rs"),
        "fn v() {\n    let _ = std::fs::File::open(\"x\");\n}\n",
    );
    let scope = checked(&root, "open", (&anchor, 3, 11), &[&anchor, &user]).await;
    assert_eq!(scope.text_only_files("open"), Some(Vec::new()));
    assert_eq!(scope.out_of_reach("open").map(|out| out.files), Some(2));
}

/// A real caller the answer missed, inside the declaration's reach, still
/// makes the row partial and is the lead's only file; a public item is in
/// reach of every crate that depends on its crate, directly or not.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_missed_caller_within_reach_stays_partial() {
    let (_dir, root) = fixture();
    workspace(&root);
    write(
        &root.join("alpha/src/lib.rs"),
        "pub mod search;\nmod other;\n",
    );
    write(
        &root.join("alpha/src/search.rs"),
        "mod executor;\nmod extra;\n",
    );
    let anchor = root.join("alpha/src/search/executor.rs");
    write(&anchor, "pub(super) fn page_of() {}\n");
    let extra = root.join("alpha/src/search/extra.rs");
    write(&extra, "fn z() {\n    super::executor::page_of();\n}\n");
    write(
        &root.join("alpha/src/other.rs"),
        "fn page_of() {}\nfn x() {\n    page_of();\n}\n",
    );
    let scope = checked(&root, "page_of", (&anchor, 0, 14), &[&anchor]).await;
    assert_eq!(scope.text_only_files("page_of"), Some(vec![shown(&extra)]));
    assert_eq!(scope.out_of_reach("page_of").map(|out| out.files), Some(1));
    let mut row = json!({"operation": "callers"});
    flag_partial(
        &mut row,
        &callers(&anchor, "page_of"),
        crate::tools::lsp_search::server_coverage::BUILD_SCRIPTS_REASON,
        None,
        &scope,
    );
    assert_eq!(row["isPartial"], true, "{row}");
    assert_eq!(row["coverage"]["textOnlyFiles"], 1, "{row}");
    let lead = &crate::tools::test_support::checks::related_call(&row, "localSearch", |_| true)["input"]
        ["queries"];
    assert_eq!(lead.as_array().map(Vec::len), Some(1), "{row}");
    assert_eq!(lead[0]["path"], shown(&extra), "{row}");

    // `pub fn shared` in alpha: beta (a dependent) and gamma (through
    // beta) may call it; delta, which depends on neither, may not.
    let lib = root.join("alpha/src/lib.rs");
    write(&lib, "pub mod search;\nmod other;\npub fn shared() {}\n");
    let beta = root.join("beta/src/lib.rs");
    write(&beta, "fn b() {\n    alpha::shared();\n}\n");
    let gamma = root.join("gamma/src/lib.rs");
    write(&gamma, "fn g() {\n    shared();\n}\n");
    write(
        &root.join("delta/src/lib.rs"),
        "fn shared() {}\nfn d() {\n    shared();\n}\n",
    );
    let scope = checked(&root, "shared", (&lib, 2, 7), &[&lib]).await;
    let mut missed = scope.text_only_files("shared").expect("checked");
    missed.sort();
    assert_eq!(missed, vec![shown(&beta), shown(&gamma)]);
    assert_eq!(scope.out_of_reach("shared").map(|out| out.files), Some(1));
}

/// Without a declaration at the anchor (a use site), or outside Rust, no
/// bound is known: every file stays in reach, as before.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_anchor_off_a_declaration_keeps_every_file() {
    let (_dir, root) = fixture();
    workspace(&root);
    write(&root.join("alpha/src/lib.rs"), "mod a;\nmod b;\n");
    let anchor = root.join("alpha/src/a.rs");
    write(&anchor, "fn f() {\n    page_of();\n}\n");
    let other = root.join("alpha/src/b.rs");
    write(&other, "fn page_of() {}\nfn g() {\n    page_of();\n}\n");
    let scope = checked(&root, "page_of", (&anchor, 1, 4), &[&anchor]).await;
    assert_eq!(scope.text_only_files("page_of"), Some(vec![shown(&other)]));
    assert!(scope.out_of_reach("page_of").is_none());
}

/// A module placed by `#[path]` outside the territory's directory is still
/// a module of it: in reach. One a build script declares stays the build
/// script's: out of reach.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_path_attribute_module_stays_in_reach() {
    let (_dir, root) = fixture();
    workspace(&root);
    write(&root.join("alpha/src/lib.rs"), "mod engine;\n");
    write(
        &root.join("alpha/src/engine.rs"),
        "mod cursors;\n#[path = \"../relocated.rs\"]\nmod relocated;\n",
    );
    let anchor = root.join("alpha/src/engine/cursors.rs");
    write(&anchor, "pub(super) fn page_of() {}\n");
    let relocated = root.join("alpha/relocated.rs");
    write(&relocated, "fn r() {\n    super::cursors::page_of();\n}\n");
    // Not under `build/`: the text scan prunes that name outside a git work
    // tree, as every local walk does.
    write(
        &root.join("alpha/build.rs"),
        "#[path = \"embedded/embed.rs\"]\nmod embed;\nfn main() {}\n",
    );
    write(
        &root.join("alpha/embedded/embed.rs"),
        "fn page_of() {}\nfn e() {\n    page_of();\n}\n",
    );
    let scope = checked(&root, "page_of", (&anchor, 0, 14), &[&anchor]).await;
    assert_eq!(
        scope.text_only_files("page_of"),
        Some(vec![shown(&relocated)])
    );
    assert_eq!(scope.out_of_reach("page_of").map(|out| out.files), Some(1));
}

/// A Python parameter is named by keyword from other files
/// (`scale(value=3)`): a file the answer misses that spells it so stays a
/// missed site, never out of reach (B230).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_missed_keyword_argument_in_another_file_stays_a_gap() {
    let (_dir, root) = fixture();
    let anchor = root.join("lib.py");
    write(
        &anchor,
        "def scale(value, factor=2):\n    return value * factor\n",
    );
    let caller = root.join("use.py");
    write(&caller, "from lib import scale\n\nscale(value=3)\n");
    let scope = Scope::new(shown(&root), vec!["*.py".into()]);
    let cancel = crate::execution::cancel::NeverCancel;
    scope
        .text_files("value", &workspace_policy(&root), &cancel)
        .await
        .expect("scan")
        .expect("files");
    scope.anchor_site(&shown(&anchor), 0, 10);
    scope.answer([shown(&anchor)]);
    scope
        .check_code_uses("value", &cancel)
        .await
        .expect("checked");
    assert_eq!(scope.text_only_files("value"), Some(vec![shown(&caller)]));
    assert!(scope.out_of_reach("value").is_none());
}

/// A local binding (here a parameter) reaches only its function: the
/// same-spelled parameters of the other functions in its file are no uses
/// the answer missed, and no other file can name it, so the row is complete
/// (D1: `value` at `num.rs:11`).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_local_binding_reaches_only_its_scope() {
    let (_dir, root) = fixture();
    workspace(&root);
    write(&root.join("alpha/src/lib.rs"), "mod num;\nmod other;\n");
    let anchor = root.join("alpha/src/num.rs");
    write(
        &anchor,
        "pub fn saturating_u32(value: impl TryInto<u32>) -> u32 {\n    value.try_into().unwrap_or(u32::MAX)\n}\n\npub fn saturating_i64(value: impl TryInto<i64>) -> i64 {\n    value.try_into().unwrap_or(i64::MAX)\n}\n",
    );
    write(
        &root.join("alpha/src/other.rs"),
        "fn f(value: u32) -> u32 {\n    value\n}\n",
    );
    let scope = checked(&root, "value", (&anchor, 0, 22), &[&anchor]).await;
    let path = shown(&anchor);
    scope.answer_lines(false, [(path.as_str(), 0), (path.as_str(), 1)]);
    assert!(
        scope.answer_gaps("value").is_empty(),
        "same-spelled parameters are no gaps"
    );
    assert_eq!(scope.text_only_files("value"), Some(Vec::new()));
    let out = scope.out_of_reach("value").expect("out of reach");
    assert_eq!(out.files, 1, "{out:?}");
    assert!(
        out.bound
            .contains("local binding reaches only lines 1-3 of"),
        "{out:?}"
    );
    let query = serde_json::from_value(json!({
        "operation": "references", "path": path, "symbolName": "value", "line": 1
    }))
    .expect("query");
    let mut row = json!({"operation": "references"});
    crate::tools::lsp_search::failure::flag_answer_gaps(
        &mut row,
        &query,
        &scope,
        &workspace_policy(&root),
    );
    flag_partial(
        &mut row,
        &query,
        crate::tools::lsp_search::server_coverage::BUILD_SCRIPTS_REASON,
        None,
        &scope,
    );
    assert!(row.get("isPartial").is_none(), "{row}");
    assert!(row.get("related").is_none(), "{row}");
    assert_eq!(row["coverage"]["textOnlyFiles"], 0, "{row}");

    // A use the answer leaves out inside the function is still a gap.
    let scope = checked(&root, "value", (&anchor, 0, 22), &[&anchor]).await;
    scope.answer_lines(false, [(path.as_str(), 0)]);
    assert_eq!(
        *scope.answer_gaps("value"),
        [answered::Gap {
            file: path.clone(),
            left_out: Vec::new(),
            unverified: vec![1],
        }]
    );
}

/// A file no Cargo package compiles (a scratch copy of the module, under a
/// directory with no manifest) cannot name a `pub(super)` or `pub(crate)`
/// item: out of reach, so it neither leaves the row partial nor becomes a
/// lead. One the defining crate places by `#[path]` or `include!` is source
/// of that crate: in reach.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_file_no_package_compiles_is_out_of_reach_unless_placed() {
    let (_dir, root) = fixture();
    workspace(&root);
    write(&root.join("alpha/src/lib.rs"), "mod search;\nmod other;\n");
    write(
        &root.join("alpha/src/search.rs"),
        "mod executor;\n#[path = \"../../shared/relocated.rs\"]\nmod relocated;\n",
    );
    let anchor = root.join("alpha/src/search/executor.rs");
    write(
        &anchor,
        "pub(super) fn page_of() {}\npub(crate) fn crate_of() {}\n",
    );
    let copy = root.join("scratch/executor.rs");
    write(
        &copy,
        "pub(super) fn page_of() {}\nfn c() {\n    page_of();\n    crate_of();\n}\n",
    );
    let relocated = root.join("shared/relocated.rs");
    write(&relocated, "fn r() {\n    super::executor::page_of();\n}\n");
    write(
        &root.join("alpha/src/other.rs"),
        "fn o() {\n    include!(\"../../shared/included.rs\");\n}\n",
    );
    let included = root.join("shared/included.rs");
    write(&included, "crate::search::executor::crate_of();\n");

    let scope = checked(&root, "page_of", (&anchor, 0, 14), &[&anchor]).await;
    assert_eq!(
        scope.text_only_files("page_of"),
        Some(vec![shown(&relocated)])
    );
    assert_eq!(scope.out_of_reach("page_of").map(|out| out.files), Some(1));

    let scope = checked(&root, "crate_of", (&anchor, 1, 14), &[&anchor]).await;
    assert_eq!(
        scope.text_only_files("crate_of"),
        Some(vec![shown(&included)])
    );
    assert_eq!(scope.out_of_reach("crate_of").map(|out| out.files), Some(1));
}

/// A dependency names the crate its source holds, not any crate of that
/// name: a copy of a dependent crate (a snapshot of the workspace) whose
/// path dependency is the copy's own `alpha`, or one whose path names no
/// directory, cannot reference the defining `alpha`, so its spelling is out
/// of reach. A crate whose path dependency is the defining `alpha` stays in
/// reach, and so does a registry dependency of that name, which a patch
/// this does not read may point at it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_dependency_on_a_same_named_crate_elsewhere_is_out_of_reach() {
    let (_dir, root) = fixture();
    workspace(&root);
    let lib = root.join("alpha/src/lib.rs");
    write(&lib, "pub fn shared() {}\n");
    let beta = root.join("beta/src/lib.rs");
    write(&beta, "fn b() {\n    alpha::shared();\n}\n");
    let copy = root.join("copy");
    write(
        &copy.join("Cargo.toml"),
        "[workspace]\nmembers = [\"alpha\", \"beta\"]\n",
    );
    write(
        &copy.join("alpha/Cargo.toml"),
        "[package]\nname = \"alpha\"\nversion = \"0.1.0\"\n",
    );
    write(&copy.join("alpha/src/lib.rs"), "pub fn shared() {}\n");
    write(
        &copy.join("beta/Cargo.toml"),
        "[package]\nname = \"beta\"\nversion = \"0.1.0\"\n\n[dependencies]\nalpha = { path = \"../alpha\" }\n",
    );
    write(
        &copy.join("beta/src/lib.rs"),
        "fn b() {\n    alpha::shared();\n}\n",
    );
    let dangling = root.join("dangling");
    write(
        &dangling.join("Cargo.toml"),
        "[package]\nname = \"dangling\"\nversion = \"0.1.0\"\n\n[dependencies]\nalpha = { path = \"../missing\" }\n",
    );
    write(
        &dangling.join("src/lib.rs"),
        "fn d() {\n    alpha::shared();\n}\n",
    );
    let registry = root.join("registry");
    write(
        &registry.join("Cargo.toml"),
        "[package]\nname = \"registry\"\nversion = \"0.1.0\"\n\n[dependencies]\nalpha = \"0.1\"\n",
    );
    let pinned = registry.join("src/lib.rs");
    write(&pinned, "fn r() {\n    alpha::shared();\n}\n");

    let scope = checked(&root, "shared", (&lib, 0, 7), &[&lib]).await;
    let mut missed = scope.text_only_files("shared").expect("checked");
    missed.sort();
    let mut expected = vec![shown(&beta), shown(&pinned)];
    expected.sort();
    assert_eq!(missed, expected);
    // The copy's `alpha` and `beta`, and the dangling dependent.
    assert_eq!(scope.out_of_reach("shared").map(|out| out.files), Some(3));
}
