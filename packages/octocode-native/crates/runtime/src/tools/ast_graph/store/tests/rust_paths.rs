//! Rust module-path calls, re-exports, evidence lines and includes.
use super::*;

#[test]
fn rust_bare_calls_do_not_resolve_same_named_methods() {
    let dir = git_app(&[
        (
            "Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
        ),
        ("src/helpers.rs", "pub fn route() -> u8 { 1 }\n"),
        (
            "src/lib.rs",
            "mod helpers;\nuse helpers::route;\npub struct Query;\nimpl Query {\n    pub fn route(&self) -> u8 { route() }\n    pub fn receiver(&self) -> u8 { self.route() }\n    pub fn qualified(&self) -> u8 { Self::route(self) }\n    pub fn named(&self) -> u8 { Query::route(self) }\n    pub fn nested(&self) -> u8 { fn inside() -> u8 { 2 } inside() }\n}\n",
        ),
    ]);
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let root = dir.0.as_path();
    let bare = ask(root, "callees", Some("src/lib.rs#Query.route"), |o| {
        o.confidence = Some("high".into());
    });
    assert_eq!(ids(&bare), vec!["src/helpers.rs#route"], "{}", bare.value);
    assert_eq!(bare.value["rows"][0]["via"], "import");
    assert_eq!(bare.value["rows"][0]["edgeLine"], 5);
    for caller in [
        "src/lib.rs#Query.receiver",
        "src/lib.rs#Query.qualified",
        "src/lib.rs#Query.named",
    ] {
        let qualified = ask(root, "callees", Some(caller), |o| {
            o.confidence = Some("high".into());
        });
        assert_eq!(
            ids(&qualified),
            vec!["src/lib.rs#Query.route"],
            "{caller}: {}",
            qualified.value
        );
    }
    let nested = ask(root, "callees", Some("src/lib.rs#Query.nested"), |o| {
        o.confidence = Some("high".into());
    });
    assert_eq!(
        ids(&nested),
        vec!["src/lib.rs#Query.nested.inside"],
        "{}",
        nested.value
    );
    let cycles = ask(root, "cycles", None, |o| {
        o.edges = vec!["calls".into()];
        o.confidence = Some("high".into());
    });
    assert_eq!(cycles.value["pagination"]["totalItems"], 0, "{}", cycles.value);
}

#[test]
fn implicit_receiver_languages_keep_bare_method_calls() {
    let dir = git_app(&[
        (
            "Query.java",
            "class Query { int route() { return work(); } int work() { return 1; } }\n",
        ),
        (
            "query.cpp",
            "struct Query { int route() { return work(); } int work() { return 1; } };\n",
        ),
    ]);
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    for file in ["Query.java", "query.cpp"] {
        let out = ask(
            dir.0.as_path(),
            "callees",
            Some(&format!("{file}#Query.route")),
            |o| {
                o.confidence = Some("high".into());
            },
        );
        assert_eq!(
            ids(&out),
            vec![format!("{file}#Query.work")],
            "{}",
            out.value
        );
    }
}

#[test]
fn plan_3_rust_module_path_calls_link() {
    let dir = git_app(&[
        (
            "Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
        ),
        ("src/lib.rs", "pub mod portable;\npub mod api;\n"),
        ("src/portable.rs", "pub fn sanitize(x: u8) -> u8 { x }\n"),
        (
            "src/api.rs",
            "pub fn handle() -> u8 {\n    crate::portable::sanitize(1)\n}\npub fn nested() -> u8 {\n    super::portable::sanitize(2)\n}\n",
        ),
    ]);
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    for caller in ["src/api.rs#handle", "src/api.rs#nested"] {
        let out = ask(dir.0.as_path(), "callees", Some(caller), |_| {});
        let rows = out.value["rows"].as_array().expect("rows");
        assert!(
            rows.iter()
                .any(|r| r["id"] == "src/portable.rs#sanitize" && r["via"] == "module-path"),
            "{caller}: {}",
            out.value
        );
    }
}

/// `crate::a::f()` links through `pub use b::f;` in `a`: a module-path call
/// follows the re-export to the declaring file, like a namespace call does.
#[test]
fn rust_module_path_calls_follow_pub_use_reexports() {
    let dir = git_app(&[
        (
            "Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
        ),
        ("src/lib.rs", "pub mod a;\npub mod api;\n"),
        ("src/a/mod.rs", "mod b;\npub(crate) use b::f;\n"),
        ("src/a/b.rs", "pub fn f() -> u8 { 1 }\n"),
        ("src/api.rs", "pub fn g() -> u8 {\n    crate::a::f()\n}\n"),
    ]);
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let out = ask(dir.0.as_path(), "callees", Some("src/api.rs#g"), |_| {});
    let rows = out.value["rows"].as_array().expect("rows");
    assert!(
        rows.iter()
            .any(|r| r["id"] == "src/a/b.rs#f" && r["via"] == "module-path"),
        "{}",
        out.value
    );
}

/// Every file edge carries the line of its own evidence: a Rust path-qualified
/// use (`crate::x::f()`) is its own line.
#[test]
fn path_qualified_rust_edges_keep_their_evidence_line() {
    let dir = git_app(&[
        (
            "Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
        ),
        ("src/lib.rs", "pub mod portable;\npub mod api;\n"),
        (
            "src/portable.rs",
            "pub const LIMIT: u8 = 1;\npub fn sanitize(x: u8) -> u8 { x }\n",
        ),
        (
            "src/api.rs",
            "pub fn handle() -> u8 {\n    crate::portable::sanitize(crate::portable::LIMIT)\n}\n",
        ),
    ]);
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let deps = ask(dir.0.as_path(), "deps", Some("src/api.rs"), |_| {});
    let rows = deps.value["rows"].as_array().expect("rows");
    let row = rows
        .iter()
        .find(|r| r["id"] == "src/portable.rs")
        .unwrap_or_else(|| panic!("portable dep: {}", deps.value));
    assert_eq!(row["edgeLine"], json!(2), "{}", deps.value);
    // With a `use` and a later qualified path to one file, the row cites the
    // first reference: the `use` line.
    dir.write(
        "app/src/api.rs",
        "use crate::portable::LIMIT;\npub fn handle() -> u8 {\n    crate::portable::sanitize(LIMIT)\n}\n",
    );
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let deps = ask(dir.0.as_path(), "deps", Some("src/api.rs"), |_| {});
    let rows = deps.value["rows"].as_array().expect("rows");
    let row = rows
        .iter()
        .find(|r| r["id"] == "src/portable.rs")
        .unwrap_or_else(|| panic!("portable dep: {}", deps.value));
    assert_eq!(row["edgeLine"], json!(1), "{}", deps.value);
}

/// `include!("gen/types.rs")` makes the included file a dependency of the
/// including one, resolved from the including file's directory.
#[test]
fn rust_include_links_the_included_file() {
    let dir = git_app(&[
        (
            "Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
        ),
        ("src/lib.rs", "pub mod api;\n"),
        (
            "src/api.rs",
            "include!(\"gen/types.rs\");\npub fn handle() -> u8 {\n    generated()\n}\n",
        ),
        ("src/gen/types.rs", "pub fn generated() -> u8 { 1 }\n"),
    ]);
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let deps = ask(dir.0.as_path(), "deps", Some("src/api.rs"), |_| {});
    assert!(
        ids(&deps).contains(&"src/gen/types.rs".to_owned()),
        "{}",
        deps.value
    );
}

/// A Rust value is never a module: `driver.events.probe()` reads a local
/// named like the imported module `driver`, so the module is no namespace
/// for it, in plain code or a macro argument; `driver::f()` still is.
#[test]
fn rust_member_calls_on_a_local_named_like_a_module_do_not_link_by_namespace() {
    let dir = git_app(&[
        (
            "Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
        ),
        ("src/lib.rs", "pub mod driver;\npub mod io;\n"),
        (
            "src/driver.rs",
            "pub struct Handle;\nimpl Handle {\n    pub fn probe_one(&self) -> u8 { 1 }\n    pub fn probe_two(&self) -> u8 { 2 }\n}\npub fn start() -> u8 { 0 }\n",
        ),
        (
            "src/io.rs",
            "use crate::driver;\npub struct Io { pub events: u8 }\npub fn plain(driver: Io) { let _ = driver.events.probe_one(); }\npub fn asserted() { let driver = Io { events: 0 }; assert_eq!(driver.events.probe_two(), 1); }\npub fn moduled() -> u8 { driver::start() }\n",
        ),
    ]);
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let root = dir.0.as_path();
    for target in [
        "src/driver.rs#Handle.probe_one",
        "src/driver.rs#Handle.probe_two",
    ] {
        let callers = ask(root, "callers", Some(target), |o| {
            o.confidence = Some("low".into());
        });
        assert_eq!(ids(&callers), Vec::<String>::new(), "{}", callers.value);
    }
    let start = ask(root, "callers", Some("src/driver.rs#start"), |o| {
        o.confidence = Some("low".into());
    });
    assert_eq!(ids(&start), vec!["src/io.rs#moduled"], "{}", start.value);
    assert_eq!(
        start.value["rows"][0]["via"], "namespace",
        "{}",
        start.value
    );
}
