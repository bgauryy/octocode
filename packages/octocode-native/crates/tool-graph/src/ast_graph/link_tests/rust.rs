//! Rust module trees, qualified paths, crate paths and `pub use` re-exports.
use super::*;
use octocode_test_support::json_fixture;

// ── Rust ─────────────────────────────────────────────────────────────────────

/// A `mod` tree loops by construction (the parent declares the child, the
/// child uses its parent): no cycle. Two modules using each other are one.
#[test]
fn rust_module_tree_loops_are_not_cycles_but_use_cycles_are() {
    let temp = fixture(&[
        CARGO_APP,
        (
            "src/lib.rs",
            "pub mod tools;\npub mod a;\npub mod b;\npub fn root() {}\n",
        ),
        ("src/tools/mod.rs", "pub mod inner;\npub fn shared() {}\n"),
        (
            "src/tools/inner.rs",
            "use super::shared;\npub fn run() { shared(); }\n",
        ),
        ("src/a.rs", "use crate::b::bee;\npub fn ay() { bee(); }\n"),
        ("src/b.rs", "use crate::a::ay;\npub fn bee() { ay(); }\n"),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        cycle_members(&ask(root, "cycles", None)),
        [json_fixture(r#"["src/a.rs", "src/b.rs"]"#)]
    );
}

/// `include_str!` embeds a file as data: a dependency its dependents see,
/// but never a loop in the code graph.
#[test]
fn rust_data_includes_link_files_but_close_no_cycle() {
    let temp = fixture(&[
        CARGO_APP,
        ("src/lib.rs", "pub mod a;\npub mod b;\n"),
        (
            "src/a.rs",
            "pub const SAMPLE: &str = include_str!(\"b.rs\");\npub fn ay() {}\n",
        ),
        ("src/b.rs", "use crate::a::ay;\npub fn bee() { ay(); }\n"),
    ]);
    let root = ingested(&temp);
    assert!(cycle_members(&ask(root, "cycles", None)).is_empty());
    let dependents = ask(root, "dependents", Some("src/b.rs"));
    assert_eq!(
        result_row(&dependents, "src/a.rs")["via"],
        "rust-include-data",
        "{dependents}"
    );
}

#[test]
fn rust_qualified_path_calls_are_dependencies_that_close_cycles() {
    let temp = fixture(&[
        CARGO_APP,
        ("src/lib.rs", "pub mod a;\npub mod b;\n"),
        // `a` names `b` only through a path in a body; `b` imports `a`.
        ("src/a.rs", "pub fn ay() { crate::b::bee(); }\n"),
        (
            "src/b.rs",
            "use crate::a::ay;\npub fn bee() {}\npub fn other() { ay(); }\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        cycle_members(&ask(root, "cycles", None)),
        [json_fixture(r#"["src/a.rs", "src/b.rs"]"#)]
    );
    // The dependency cites the path's line as its evidence.
    let deps = ask(root, "deps", Some("src/a.rs"));
    assert_eq!(result_row(&deps, "src/b.rs")["edgeLine"], 1, "{deps}");
}

/// Called module paths, and qualified paths outside a call (a constant, a
/// function value, a path in a macro body, a type), are edges to their
/// module.
#[test]
fn rust_qualified_paths_link_to_their_module() {
    let temp = fixture(&[
        CARGO_APP,
        (
            "src/lib.rs",
            "pub mod tools;\npub mod portable;\npub mod rules;\npub mod user;\npub mod api;\n",
        ),
        ("src/tools/mod.rs", "pub mod inner;\n"),
        ("src/tools/inner.rs", "pub fn used() {}\n"),
        ("src/portable.rs", "pub fn sanitize() {}\n"),
        (
            "src/api.rs",
            "pub fn handle() {\n    crate::portable::sanitize();\n    crate::tools::inner::used();\n}\n",
        ),
        (
            "src/rules.rs",
            "pub const HINT: &str = \"h\";\npub fn globs(x: u8) -> u8 { x }\npub fn recovery() -> u8 { 1 }\npub struct Out;\n",
        ),
        (
            "src/user.rs",
            "pub fn run() -> Option<crate::rules::Out> {\n    let _ = crate::rules::HINT;\n    let _ = [1u8].map(crate::rules::globs);\n    let _ = format!(\"{}\", crate::rules::recovery());\n    None\n}\n",
        ),
    ]);
    let root = ingested(&temp);
    let deps = ask(root, "deps", Some("src/api.rs"));
    assert_eq!(
        files(&deps),
        ["src/portable.rs", "src/tools/inner.rs"],
        "{deps}"
    );
    let deps = ask(root, "deps", Some("src/user.rs"));
    assert_eq!(files(&deps), ["src/rules.rs"], "{deps}");
    assert_eq!(unresolved_internal(root), 0);
}

/// A `crate::` path that cannot be linked is an internal gap, never a
/// complete graph.
#[test]
fn rust_unlinkable_crate_paths_count_as_unresolved_internal() {
    let temp = fixture(&[(
        "src/user.rs",
        "pub fn run() -> u8 { let _ = crate::missing::LIMIT; 1 }\n",
    )]);
    let root = ingested(&temp);
    assert_eq!(unresolved_internal(root), 1);
}

/// A scan of a directory inside a crate (its `src/lib.rs` above the scan)
/// links `crate::` paths into the scanned modules from the scan root; a
/// path that leaves the scanned modules stays an unresolved gap.
#[test]
fn crate_paths_link_inside_a_subdirectory_scan() {
    let temp = fixture(&[
        CARGO_APP,
        ("src/lib.rs", "pub mod tools;\n"),
        (
            "src/tools/mod.rs",
            "pub mod a;\npub mod b;\npub const TOP: u8 = 1;\n",
        ),
        (
            "src/tools/a.rs",
            "use crate::tools::b::helper;\npub fn run() -> u8 { helper() + crate::tools::TOP + crate::config::LIMIT }\n",
        ),
        ("src/tools/b.rs", "pub fn helper() -> u8 { 1 }\n"),
    ]);
    let root = temp.0.as_path();
    ingest(root, &root.join("src/tools"));
    let deps = ask(root, "deps", Some("a.rs"));
    assert_eq!(
        files(&deps),
        ["src/tools/b.rs", "src/tools/mod.rs"],
        "{deps}"
    );
    assert_eq!(unresolved_internal(root), 1);
}

#[test]
fn rust_super_past_an_inline_module_climbs_from_the_enclosing_module() {
    let temp = fixture(&[
        CARGO_APP,
        ("src/lib.rs", "pub mod x;\npub mod z;\n"),
        ("src/z.rs", "pub fn top() {}\n"),
        ("src/x/mod.rs", "pub mod y;\npub mod z;\n"),
        ("src/x/z.rs", "pub fn near() {}\n"),
        // Inside `mod tests` (one inline level), `super::super` is the
        // parent of the file's module: `crate::x`, so `z` is `x/z.rs`.
        (
            "src/x/y.rs",
            "pub fn y() {}\n#[cfg(test)]\nmod tests {\n    use super::super::z::near;\n}\n",
        ),
    ]);
    let root = temp.0.as_path();
    ingest(root, &root.join("src"));
    let deps = ask(root, "deps", Some("x/y.rs"));
    assert_eq!(files(&deps), ["src/x/z.rs"], "{deps}");
}

/// A file's dependents include the modules that use its items through a
/// `pub use` re-export, and only those naming a re-exported item.
#[test]
fn dependents_follow_rust_pub_use_reexports() {
    let temp = fixture(&[
        CARGO_APP,
        ("src/lib.rs", "pub mod sync;\npub mod task;\n"),
        (
            "src/sync/mod.rs",
            "mod notify;\npub use notify::Notify;\nmod broadcast;\nmod other;\npub struct Other;\n",
        ),
        ("src/sync/notify.rs", "pub struct Notify;\n"),
        (
            "src/sync/broadcast.rs",
            "use super::Notify;\npub fn b(_: Notify) {}\n",
        ),
        (
            "src/sync/other.rs",
            "use super::Other;\npub fn o(_: Other) {}\n",
        ),
        ("src/task/mod.rs", "pub mod local;\n"),
        (
            "src/task/local.rs",
            "use crate::sync::Notify;\npub fn l(_: Notify) {}\n",
        ),
    ]);
    let root = ingested(&temp);
    let dependents = ask(root, "dependents", Some("src/sync/notify.rs"));
    let mut found = files(&dependents);
    found.sort_unstable();
    assert_eq!(
        found,
        [
            "src/sync/broadcast.rs",
            "src/sync/mod.rs",
            "src/task/local.rs"
        ],
        "{dependents}"
    );
    assert_eq!(
        result_row(&dependents, "src/task/local.rs")["via"],
        "reexport",
        "{dependents}"
    );
}

/// A snapshot rooted inside a crate: `aliases.rs` declares `mod config;` but
/// its own parent module is outside the scan. Its children still live in
/// `aliases/`, so the declaration links and calls through it resolve.
#[test]
fn an_orphan_module_file_finds_its_children_in_its_own_directory() {
    let temp = fixture(&[
        (
            "aliases.rs",
            "mod config;\npub fn load() { config::read(); }\n",
        ),
        ("aliases/config.rs", "pub fn read() {}\n"),
    ]);
    let root = ingested(&temp);
    assert_eq!(unresolved_internal(root), json!(0));
    assert_eq!(
        call_edges(root, "aliases.rs#load"),
        [edge("aliases/config.rs#read", "namespace", "high")]
    );
}

/// A snapshot rooted at a module directory: `super::capture::Item` from a
/// root-level file names its sibling module, also when the file declares an
/// inline `mod tests` (which makes it a parentless module-tree root).
#[test]
fn a_super_path_to_a_root_level_sibling_links_inside_a_module_directory() {
    let temp = fixture(&[
        (
            "scope.rs",
            "pub fn charge(source: &super::capture::CapturedSource) -> usize { source.len() }\n#[cfg(test)]\nmod tests {\n    use super::*;\n}\n",
        ),
        (
            "capture.rs",
            "pub struct CapturedSource;\nimpl CapturedSource { pub fn len(&self) -> usize { 0 } }\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(unresolved_internal(root), json!(0));
    let deps = ask(root, "deps", Some("scope.rs"));
    assert_eq!(files(&deps), ["capture.rs"]);
}

/// A lone path keyword is a module, never a receiver: `super::f()` names the
/// parent module's item (not a same-named one of the caller's file),
/// `self::f()` the caller's own module, and `crate::f()` what the crate root
/// declares or binds through `pub use`, also from inside an inline module.
#[test]
fn lone_super_self_and_crate_paths_reach_their_module() {
    let temp = fixture(&[
        CARGO_APP,
        (
            "src/lib.rs",
            "pub mod budget;\nmod content;\npub use content::is_full;\npub fn root() -> u8 { 1 }\n",
        ),
        ("src/content.rs", "pub fn is_full() -> bool { true }\n"),
        (
            "src/budget.rs",
            "mod circuit;\npub(crate) fn millis() -> u64 { 1 }\npub fn own() -> u64 { self::millis() }\n",
        ),
        (
            "src/budget/circuit.rs",
            "pub fn millis() -> u64 { super::millis() }\npub fn open() -> u64 { super::millis() }\npub fn sha() -> bool { crate::is_full() }\npub fn top() -> u8 { crate::root() }\nmod inner {\n    pub fn deep() -> u8 { crate::root() }\n}\n",
        ),
    ]);
    let root = ingested(&temp);
    let parent = [edge("src/budget.rs#millis", "module-path", "high")];
    assert_eq!(call_edges(root, "src/budget/circuit.rs#open"), parent);
    assert_eq!(call_edges(root, "src/budget/circuit.rs#millis"), parent);
    assert_eq!(call_edges(root, "src/budget.rs#own"), parent);
    assert_eq!(
        call_edges(root, "src/budget/circuit.rs#sha"),
        [edge("src/content.rs#is_full", "module-path", "high")]
    );
    let crate_root = [edge("src/lib.rs#root", "module-path", "high")];
    assert_eq!(call_edges(root, "src/budget/circuit.rs#top"), crate_root);
    assert_eq!(
        call_edges(root, "src/budget/circuit.rs#inner.deep"),
        crate_root
    );
    assert_eq!(unresolved_internal(root), json!(0));
}

/// A directory with `lib.rs` and `main.rs` roots two crates: `crate::f()`
/// from a module binds in the root that has `f`, and each root's own
/// `crate::` path stays in that root.
#[test]
fn a_crate_path_picks_the_root_that_binds_the_name() {
    let temp = fixture(&[
        CARGO_APP,
        (
            "src/lib.rs",
            "pub mod api;\npub fn shared() {}\npub fn both() {}\npub fn lib_own() { crate::both() }\n",
        ),
        (
            "src/main.rs",
            "mod cli;\nfn boot() {}\nfn both() {}\nfn main() { crate::both() }\n",
        ),
        ("src/api.rs", "pub fn run() { crate::shared() }\n"),
        (
            "src/cli.rs",
            "pub fn go() { crate::boot() }\npub fn either() { crate::both() }\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        call_edges(root, "src/api.rs#run"),
        [edge("src/lib.rs#shared", "module-path", "high")]
    );
    assert_eq!(
        call_edges(root, "src/cli.rs#go"),
        [edge("src/main.rs#boot", "module-path", "high")]
    );
    assert_eq!(
        call_edges(root, "src/lib.rs#lib_own"),
        [edge("src/lib.rs#both", "module-path", "high")]
    );
    assert_eq!(
        call_edges(root, "src/main.rs#main"),
        [edge("src/main.rs#both", "module-path", "high")]
    );
    // Nothing scanned says which root declares `cli`: both stay candidates.
    assert_eq!(
        call_edges(root, "src/cli.rs#either"),
        [
            edge("src/lib.rs#both", "module-path", "medium"),
            edge("src/main.rs#both", "module-path", "medium")
        ]
    );
}

/// A keyword path its module does not bind stays unresolved and says why:
/// the name is declared elsewhere in the repo (`other`), or nowhere
/// (`not-declared-in-repo`). It has no receiver, and never links by name.
#[test]
fn an_unbound_keyword_path_stays_unresolved_with_a_truthful_reason() {
    let temp = fixture(&[
        CARGO_APP,
        ("src/lib.rs", "pub mod a;\npub mod b;\n"),
        ("src/a.rs", "pub fn elsewhere() {}\n"),
        (
            "src/b.rs",
            "pub fn run() {\n    crate::elsewhere();\n    super::elsewhere();\n    crate::nowhere();\n}\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(call_edges(root, "src/b.rs#run"), []);
    let out = ask(root, "unresolved", Some("src/b.rs#run"));
    let mut reasons = out["rows"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|row| {
            (
                row["line"].as_u64().unwrap_or_default(),
                row["reason"].as_str().unwrap_or_default(),
            )
        })
        .collect::<Vec<_>>();
    reasons.sort_unstable();
    assert_eq!(
        reasons,
        [(2, "other"), (3, "other"), (4, "not-declared-in-repo")],
        "{out}"
    );
}

/// A `use` path starts at any name the module binds (Rust 2018 uniform
/// paths), a `use` of a module included: `use streamed::walk_cut` after
/// `use scanned::streamed` names the item in that module's file, which is
/// in this repository, never a package.
#[test]
fn a_use_path_through_a_module_another_use_binds_links_its_item() {
    let temp = fixture(&[
        CARGO_APP,
        ("src/lib.rs", "pub mod exec;\n"),
        (
            "src/exec.rs",
            "mod scanned;\npub(super) use scanned::{Scanned, streamed};\nuse streamed::{WalkCut, walk_cut};\npub fn shape(_: Scanned, _: WalkCut) -> u8 { walk_cut() }\n",
        ),
        (
            "src/exec/scanned.rs",
            "pub(super) mod streamed;\npub struct Scanned;\n",
        ),
        (
            "src/exec/scanned/streamed.rs",
            "pub struct WalkCut;\npub fn walk_cut() -> u8 { 1 }\n",
        ),
    ]);
    let root = ingested(&temp);
    let targets = call_edges(root, "src/exec.rs#shape")
        .into_iter()
        .map(|(id, _, _)| id)
        .collect::<Vec<_>>();
    assert_eq!(targets, ["src/exec/scanned/streamed.rs#walk_cut"]);
    let deps = ask(root, "deps", Some("src/exec.rs"));
    assert!(
        files(&deps).contains(&"src/exec/scanned/streamed.rs"),
        "{deps}"
    );
    let unresolved = ask(root, "unresolved", Some("src/exec.rs"));
    assert_eq!(unresolved["rows"], json!([]), "{unresolved}");
}

/// `use super::*` binds every name the parent module binds, its private
/// `use` of a module included: `query::view()` in the child calls that
/// module's item, and the child depends on the module's file.
#[test]
fn a_parent_glob_binds_the_modules_the_parent_uses() {
    let temp = fixture(&[
        CARGO_APP,
        ("src/lib.rs", "pub mod query;\npub mod verify;\n"),
        ("src/query.rs", "pub fn view() -> u8 { 1 }\n"),
        ("src/verify.rs", "use crate::query;\nmod guard;\n"),
        (
            "src/verify/guard.rs",
            "use super::*;\npub fn of() -> u8 { query::view() }\n",
        ),
    ]);
    let root = ingested(&temp);
    let targets = call_edges(root, "src/verify/guard.rs#of")
        .into_iter()
        .map(|(id, _, _)| id)
        .collect::<Vec<_>>();
    assert_eq!(targets, ["src/query.rs#view"]);
    let dependents = ask(root, "dependents", Some("src/query.rs"));
    assert!(
        files(&dependents).contains(&"src/verify/guard.rs"),
        "{dependents}"
    );
}
