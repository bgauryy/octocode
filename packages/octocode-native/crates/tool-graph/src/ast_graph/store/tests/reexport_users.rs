//! A file that names a re-exported item, by glob, by module path or by a
//! call, is a dependent of the file that declares it; one that does not name
//! it is not.
use super::*;

const CARGO: (&str, &str) = (
    "Cargo.toml",
    "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
);

fn build(files: &[(&str, &str)]) -> Fixture {
    let mut all = vec![CARGO];
    all.extend_from_slice(files);
    let dir = git_app(&all);
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    dir
}

fn dependents(dir: &Fixture, target: &str) -> Vec<String> {
    let out = ask(dir.0.as_path(), "dependents", Some(target), |_| {});
    ids(&out)
}

fn assert_lists(listed: &[String], users: &[&str], bystanders: &[&str]) {
    for user in users {
        assert!(listed.contains(&(*user).to_owned()), "{user}: {listed:?}");
    }
    for bystander in bystanders {
        assert!(
            !listed.contains(&(*bystander).to_owned()),
            "{bystander}: {listed:?}"
        );
    }
}

/// `use super::*` reaches names the parent takes from a child with its own
/// `use child::*`, whose `pub use`s it makes public and whose declarations
/// are public too; a module that names none of them is no dependent.
#[test]
fn a_parents_child_glob_passes_the_childs_public_names_to_its_glob_importers() {
    let dir = build(&[
        ("src/lib.rs", "mod store;\n"),
        (
            "src/store.rs",
            "mod edges;\nmod idle;\nmod resolve;\nmod stats;\nuse resolve::*;\n",
        ),
        (
            "src/store/resolve.rs",
            "mod exports;\nmod inherited;\nmod hidden;\nuse hidden::Secret;\npub use exports::Bindings;\npub use inherited::Index;\npub struct Own {}\npub fn keep(_: Option<Secret>) {}\n",
        ),
        (
            "src/store/resolve/exports.rs",
            "pub struct Bindings {}\nimpl Bindings {\n    pub fn new() -> Self {\n        Bindings {}\n    }\n}\n",
        ),
        ("src/store/resolve/inherited.rs", "pub struct Index {}\n"),
        ("src/store/resolve/hidden.rs", "pub struct Secret {}\n"),
        (
            "src/store/edges.rs",
            "use super::*;\npub fn make() {\n    let _ = Bindings::new();\n    let _: Option<Index> = None;\n}\n",
        ),
        (
            "src/store/stats.rs",
            "use super::*;\npub fn one(_: &Bindings, _: Own) {}\n",
        ),
        ("src/store/idle.rs", "use super::*;\npub fn idle() {}\n"),
    ]);
    let exports = dependents(&dir, "app/src/store/resolve/exports.rs");
    assert_lists(
        &exports,
        &["app/src/store/edges.rs", "app/src/store/stats.rs"],
        &["app/src/store/idle.rs"],
    );
    let inherited = dependents(&dir, "app/src/store/resolve/inherited.rs");
    assert_lists(
        &inherited,
        &["app/src/store/edges.rs"],
        &["app/src/store/stats.rs", "app/src/store/idle.rs"],
    );
    // A child's private `use` is not passed on.
    let hidden = dependents(&dir, "app/src/store/resolve/hidden.rs");
    assert_lists(
        &hidden,
        &["app/src/store/resolve.rs"],
        &["app/src/store/edges.rs", "app/src/store/stats.rs"],
    );
    // A child's own declaration is public to its parent's glob importers.
    let resolve = dependents(&dir, "app/src/store/resolve.rs");
    assert_lists(
        &resolve,
        &["app/src/store/stats.rs"],
        &["app/src/store/idle.rs", "app/src/store/edges.rs"],
    );
}

/// `pub(super)` and `pub(crate)` items reach the parent as `pub` ones do, so
/// a sibling naming one through the parent's `use child::*` is a dependent
/// of the child, whether as a type or a call.
#[test]
fn restricted_visibility_items_a_child_glob_passes_on_are_dependents() {
    let dir = build(&[
        ("src/lib.rs", "mod query;\n"),
        (
            "src/query.rs",
            "mod find;\nmod idle;\nmod paging;\nmod walks;\nuse paging::*;\n",
        ),
        (
            "src/query/paging.rs",
            "pub(super) struct Page {}\npub(crate) fn settle() {}\n",
        ),
        (
            "src/query/find.rs",
            "use super::*;\npub fn first(_: &Page) {}\n",
        ),
        (
            "src/query/walks.rs",
            "use super::*;\npub fn walk() {\n    settle();\n}\n",
        ),
        ("src/query/idle.rs", "use super::*;\npub fn idle() {}\n"),
    ]);
    let paging = dependents(&dir, "app/src/query/paging.rs");
    assert_lists(
        &paging,
        &["app/src/query/find.rs", "app/src/query/walks.rs"],
        &["app/src/query/idle.rs"],
    );
}

/// A file naming an item through its own `use child::*`, where the child
/// re-exports it, is a dependent of the declaring file, as a struct literal
/// or a type; one whose glob names nothing of it is not.
#[test]
fn a_files_own_child_glob_reaches_the_items_the_child_reexports() {
    let dir = build(&[
        ("src/lib.rs", "mod tables;\nmod quiet;\n"),
        (
            "src/tables.rs",
            "mod resolve;\nuse resolve::*;\npub fn project() {\n    let _ = Resolver {};\n}\n",
        ),
        (
            "src/tables/resolve.rs",
            "mod exports;\npub(super) use exports::{Bindings, Resolver};\n",
        ),
        (
            "src/tables/resolve/exports.rs",
            "pub(crate) struct Resolver {}\npub(crate) struct Bindings {}\n",
        ),
        (
            "src/quiet.rs",
            "mod inner;\nuse inner::*;\npub fn quiet() {}\n",
        ),
        ("src/quiet/inner.rs", "pub struct Resolver {}\n"),
    ]);
    let exports = dependents(&dir, "app/src/tables/resolve/exports.rs");
    assert_lists(
        &exports,
        &["app/src/tables.rs", "app/src/tables/resolve.rs"],
        &["app/src/quiet.rs"],
    );
}

/// A struct's users reach it by `uses` edges, which the symbol default
/// (`calls`) leaves out: the empty answer leads to the edges that list them.
#[test]
fn an_empty_symbol_walk_leads_to_the_edges_that_reach_it() {
    let dir = build(&[
        ("src/lib.rs", "mod page;\nmod find;\n"),
        ("src/page.rs", "pub struct Page {}\n"),
        (
            "src/find.rs",
            "use crate::page::Page;\npub fn first(_: &Page) {}\n",
        ),
    ]);
    let out = ask(
        dir.0.as_path(),
        "dependents",
        Some("app/src/page.rs#Page"),
        |_| {},
    );
    assert!(ids(&out).is_empty(), "{}", out.value);
    let related = out.value["related"].to_string();
    assert!(
        related.contains("\"edges\"") && related.contains("\"uses\""),
        "{}",
        out.value
    );
    let wider = ask(
        dir.0.as_path(),
        "dependents",
        Some("app/src/page.rs#Page"),
        |o| o.edges = vec!["uses".into()],
    );
    assert_eq!(ids(&wider), vec!["app/src/find.rs"], "{}", wider.value);
}

/// A method call on a receiver whose type the syntax does not state links
/// no edge: the file's dependents say such sites may be missing rows.
#[test]
fn file_dependents_disclose_unlinked_calls_of_its_declarations() {
    let dir = build(&[
        ("src/lib.rs", "mod reader;\nmod user;\n"),
        (
            "src/reader.rs",
            "pub struct Reader {}\nimpl Reader {\n    pub fn load_origin(&self) {}\n}\n",
        ),
        (
            "src/user.rs",
            "pub fn run(cell: std::cell::RefCell<Option<u8>>) {\n    cell.borrow_mut().load_origin();\n}\n",
        ),
    ]);
    let out = ask(
        dir.0.as_path(),
        "dependents",
        Some("app/src/reader.rs"),
        |_| {},
    );
    assert_eq!(
        out.value["partialReasons"],
        serde_json::json!(["callSitesUnresolved"]),
        "{}",
        out.value
    );
    assert!(
        out.value["related"].to_string().contains("load_origin"),
        "{}",
        out.value
    );
}

/// Globs that import each other do not loop, and a module that declares the
/// name itself ends the walk.
#[test]
fn child_glob_chains_terminate_and_a_declaring_module_stops_them() {
    let dir = build(&[
        ("src/lib.rs", "mod store;\n"),
        (
            "src/store.rs",
            "mod a;\nmod b;\nmod user;\nuse a::*;\npub struct Thing {}\n",
        ),
        ("src/store/a.rs", "use super::*;\npub use super::b::*;\n"),
        (
            "src/store/b.rs",
            "pub use super::a::*;\npub struct Thing {}\n",
        ),
        (
            "src/store/user.rs",
            "use super::*;\npub fn f(_: Thing) {}\n",
        ),
    ]);
    // `Thing` is the parent's own declaration: the glob of `a` never reaches
    // `b`'s one.
    let b = dependents(&dir, "app/src/store/b.rs");
    assert_lists(&b, &[], &["app/src/store/user.rs"]);
}

/// A call or path through a module that re-exports a type
/// (`format::Reader::open()`) uses the re-export: its file is a dependent
/// of the declaring file; one that names only the module's own items is not.
#[test]
fn module_qualified_calls_and_paths_through_a_reexport_are_dependents() {
    let dir = build(&[
        ("src/lib.rs", "mod store;\n"),
        (
            "src/store.rs",
            "mod bystander;\nmod format;\nmod ingest;\nmod paths;\n",
        ),
        (
            "src/store/format.rs",
            "mod selective;\npub use selective::{Reader, Strings};\npub struct Writer {}\nimpl Writer {\n    pub fn new() -> Self {\n        Writer {}\n    }\n}\n",
        ),
        (
            "src/store/format/selective.rs",
            "pub struct Reader {}\nimpl Reader {\n    pub fn open() -> Self {\n        Reader {}\n    }\n}\npub struct Strings {}\n",
        ),
        (
            "src/store/ingest.rs",
            "use super::format;\npub fn run() {\n    let _ = format::Reader::open();\n}\n",
        ),
        (
            "src/store/paths.rs",
            "use super::format;\npub fn keep(_: &format::Strings) {}\n",
        ),
        (
            "src/store/bystander.rs",
            "use super::format;\npub fn other() {\n    let _ = format::Writer::new();\n}\n",
        ),
    ]);
    let listed = dependents(&dir, "app/src/store/format/selective.rs");
    assert_lists(
        &listed,
        &["app/src/store/ingest.rs", "app/src/store/paths.rs"],
        &["app/src/store/bystander.rs"],
    );
}

/// `super::query::typed_row::<T>()` calls a function `query` re-exports: the
/// call stays a call to the declaration, and its file is a dependent of the
/// declaring file, and its function of the declared function.
#[test]
fn calls_through_a_reexport_are_dependents_at_file_and_symbol_level() {
    let dir = build(&[
        ("src/lib.rs", "mod store;\n"),
        ("src/store.rs", "mod bystander;\nmod query;\nmod tools;\n"),
        (
            "src/store/query.rs",
            "mod paging;\npub use paging::typed_row;\npub fn local() {}\n",
        ),
        (
            "src/store/query/paging.rs",
            "pub fn typed_row<T>() -> Option<T> {\n    None\n}\n",
        ),
        (
            "src/store/tools.rs",
            "pub fn call() {\n    let _ = super::query::typed_row::<u8>();\n}\n",
        ),
        (
            "src/store/bystander.rs",
            "pub fn other() {\n    super::query::local();\n}\n",
        ),
    ]);
    let files = dependents(&dir, "app/src/store/query/paging.rs");
    assert_lists(
        &files,
        &["app/src/store/tools.rs", "app/src/store/query.rs"],
        &["app/src/store/bystander.rs"],
    );
    let symbols = dependents(&dir, "app/src/store/query/paging.rs#typed_row");
    assert_eq!(symbols, vec!["app/src/store/tools.rs#call"], "{symbols:?}");
    let callers = ask(
        dir.0.as_path(),
        "callers",
        Some("app/src/store/query/paging.rs#typed_row"),
        |_| {},
    );
    assert_eq!(ids(&callers), vec!["app/src/store/tools.rs#call"]);
    assert_eq!(callers.value["rows"][0]["edge"], "calls");
}

/// A bare call bound by `use super::*` of a module that re-exports the
/// function uses the re-export: its file is a dependent of the declaring
/// file. A sibling glob importer that calls only the module's own function
/// is not.
#[test]
fn bare_calls_through_a_glob_of_a_reexport_are_file_dependents() {
    let dir = build(&[
        ("src/lib.rs", "mod store;\n"),
        ("src/store.rs", "mod query;\n"),
        (
            "src/store/query.rs",
            "mod bystander;\nmod health;\nmod paging;\npub(crate) use paging::typed_row;\npub fn local() {}\n",
        ),
        (
            "src/store/query/paging.rs",
            "pub fn typed_row() -> u8 {\n    1\n}\n",
        ),
        (
            "src/store/query/health.rs",
            "use super::*;\npub fn h() -> u8 {\n    typed_row()\n}\n",
        ),
        (
            "src/store/query/bystander.rs",
            "use super::*;\npub fn o() {\n    local();\n}\n",
        ),
    ]);
    let files = dependents(&dir, "app/src/store/query/paging.rs");
    assert_lists(
        &files,
        &["app/src/store/query/health.rs"],
        &["app/src/store/query/bystander.rs"],
    );
}

/// `format::Reader::open()` in a module whose `use super::*` reaches, through
/// another `use super::*`, the parent that declares `mod format;` uses
/// `Reader` as `format` re-exports it from `selective`; a call of an item
/// `format` declares itself does not.
#[test]
fn qualified_calls_whose_module_is_bound_two_globs_up_reach_the_reexported_item() {
    let dir = build(&[
        ("src/lib.rs", "mod store;\n"),
        ("src/store.rs", "mod format;\nmod ingest;\n"),
        (
            "src/store/format.rs",
            "mod selective;\npub(crate) use selective::Reader;\npub fn check() {}\n",
        ),
        (
            "src/store/format/selective.rs",
            "pub struct Reader {}\nimpl Reader {\n    pub fn open() -> Self {\n        Reader {}\n    }\n}\n",
        ),
        (
            "src/store/ingest.rs",
            "use super::*;\nmod redact;\nmod unchanged;\n",
        ),
        (
            "src/store/ingest/redact.rs",
            "use super::*;\npub fn r() {\n    let _ = format::Reader::open();\n}\n",
        ),
        (
            "src/store/ingest/unchanged.rs",
            "use super::*;\npub fn u() {\n    format::check();\n}\n",
        ),
    ]);
    let files = dependents(&dir, "app/src/store/format/selective.rs");
    assert_lists(
        &files,
        &["app/src/store/ingest/redact.rs"],
        &[
            "app/src/store/ingest/unchanged.rs",
            "app/src/store/ingest.rs",
        ],
    );
}
