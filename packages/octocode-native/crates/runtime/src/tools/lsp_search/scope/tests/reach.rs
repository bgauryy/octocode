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
    write(
        &root.join("alpha/build.rs"),
        "#[path = \"build/embed.rs\"]\nmod embed;\nfn main() {}\n",
    );
    write(
        &root.join("alpha/build/embed.rs"),
        "fn page_of() {}\nfn e() {\n    page_of();\n}\n",
    );
    let scope = checked(&root, "page_of", (&anchor, 0, 14), &[&anchor]).await;
    assert_eq!(
        scope.text_only_files("page_of"),
        Some(vec![shown(&relocated)])
    );
    assert_eq!(scope.out_of_reach("page_of").map(|out| out.files), Some(1));
}
