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
    let bare = ask(root, "callees", Some("app/src/lib.rs#Query.route"), |o| {
        o.confidence = Some("high".into());
    });
    assert_eq!(
        ids(&bare),
        vec!["app/src/helpers.rs#route"],
        "{}",
        bare.value
    );
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
            vec!["app/src/lib.rs#Query.route"],
            "{caller}: {}",
            qualified.value
        );
    }
    let nested = ask(root, "callees", Some("app/src/lib.rs#Query.nested"), |o| {
        o.confidence = Some("high".into());
    });
    assert_eq!(
        ids(&nested),
        vec!["app/src/lib.rs#Query.nested.inside"],
        "{}",
        nested.value
    );
    let cycles = ask(root, "cycles", None, |o| {
        o.edges = vec!["calls".into()];
        o.confidence = Some("high".into());
    });
    assert_eq!(cycles.value["pagination"]["total"], 0, "{}", cycles.value);
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
            vec![format!("app/{file}#Query.work")],
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
    for caller in ["app/src/api.rs#handle", "src/api.rs#nested"] {
        let out = ask(dir.0.as_path(), "callees", Some(caller), |_| {});
        let rows = out.value["rows"].as_array().expect("rows");
        assert!(
            rows.iter()
                .any(|r| r["id"] == "app/src/portable.rs#sanitize" && r["via"] == "module-path"),
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
    let out = ask(dir.0.as_path(), "callees", Some("app/src/api.rs#g"), |_| {});
    let rows = out.value["rows"].as_array().expect("rows");
    assert!(
        rows.iter()
            .any(|r| r["id"] == "app/src/a/b.rs#f" && r["via"] == "module-path"),
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
        .find(|r| r["id"] == "app/src/portable.rs")
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
        .find(|r| r["id"] == "app/src/portable.rs")
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
        ids(&deps).contains(&"app/src/gen/types.rs".to_owned()),
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
    let start = ask(root, "callers", Some("app/src/driver.rs#start"), |o| {
        o.confidence = Some("low".into());
    });
    assert_eq!(
        ids(&start),
        vec!["app/src/io.rs#moduled"],
        "{}",
        start.value
    );
    assert_eq!(
        start.value["rows"][0]["via"], "namespace",
        "{}",
        start.value
    );
}

/// A qualified path names one item of its module, in call position, as a
/// value (`.map(crate::num::f)`) or in a `macro_rules!` body: symbol impact
/// of that item lists the file, and a file whose paths name only a sibling
/// item of the module stays out.
#[test]
fn symbol_impact_follows_qualified_paths_to_the_named_item() {
    let dir = git_app(&[
        (
            "Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
        ),
        (
            "src/lib.rs",
            "pub mod num;\npub mod called;\npub mod valued;\npub mod sibling;\npub mod both;\npub mod expanded;\n",
        ),
        (
            "src/num.rs",
            "pub fn narrow(v: u64) -> u32 { v as u32 }\npub fn widen(v: u64) -> usize { v as usize }\n",
        ),
        (
            "src/called.rs",
            "pub fn f() -> u32 {\n    crate::num::narrow(1)\n}\n",
        ),
        (
            "src/valued.rs",
            "pub fn f(v: Option<u64>) -> Option<u32> {\n    drop(vec![1]);\n    v.map(crate::num::narrow)\n}\n",
        ),
        (
            "src/sibling.rs",
            "pub fn f(v: Option<u64>) -> Option<usize> {\n    drop(vec![1]);\n    v.map(crate::num::widen)\n}\n",
        ),
        (
            "src/both.rs",
            "pub fn f() -> usize {\n    crate::num::widen(1)\n}\npub fn g(v: Option<u64>) -> Option<u32> {\n    v.map(crate::num::narrow)\n}\n",
        ),
        (
            "src/expanded.rs",
            "pub trait Narrow {\n    fn narrowed(&self) -> Option<u32>;\n}\nmacro_rules! narrowing {\n    ($t:ty) => {\n        impl Narrow for $t {\n            fn narrowed(&self) -> Option<u32> {\n                self.0.map(|v| crate::num::narrow(v))\n            }\n        }\n    };\n}\npub struct Q(Option<u64>);\nnarrowing!(Q);\n",
        ),
    ]);
    let root = dir.0.as_path();
    assert_eq!(ingest_fixture(root, None).class, ExitClass::Success);
    let out = ask(root, "impact", Some("src/num.rs#narrow"), |o| {
        o.depth = Some(1);
    });
    let rows = out.value["rows"].as_array().cloned().unwrap_or_default();
    let risk = |file: &str| {
        rows.iter()
            .find(|row| row["id"] == format!("app/src/{file}.rs"))
            .map(|row| row["risk"].as_str().unwrap_or_default().to_owned())
    };
    for file in ["called", "valued", "both", "expanded"] {
        assert_eq!(
            risk(file).as_deref(),
            Some("direct"),
            "{file} names narrow: {}",
            out.value
        );
    }
    let files = impacted(&out)
        .into_iter()
        .map(|(file, _)| file)
        .collect::<Vec<_>>();
    assert!(
        !files.contains(&"app/src/sibling.rs".to_owned()),
        "sibling names only widen; its `vec!` group names no changed item: {}",
        out.value
    );
}

/// An importer whose linked names into the changed module are all other
/// items is dropped from symbol impact unless a macro invocation's
/// arguments, an unextracted token tree, write the changed symbol's name
/// (or an alias that renames it): then it stays a `candidate`. Groups that
/// write no changed name (`format!`, `vec!`) keep nothing.
#[test]
fn symbol_impact_keeps_importers_with_unextracted_arguments_as_candidates() {
    let dir = git_app(&[
        (
            "Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
        ),
        (
            "src/lib.rs",
            "pub mod num;\npub mod outer;\npub mod plain;\npub mod formatted;\npub mod aliased;\n",
        ),
        (
            "src/outer.rs",
            "use crate::num::narrow;\npub mod macroed;\npub fn g(v: u64) -> u32 {\n    narrow(v)\n}\n",
        ),
        (
            "src/num.rs",
            "pub fn narrow(v: u64) -> u32 { v as u32 }\npub fn widen(v: u64) -> usize { v as usize }\n",
        ),
        (
            "src/outer/macroed.rs",
            "use super::*;\nmacro_rules! keep {\n    ($e:expr) => { $e };\n}\npub fn f(v: Option<u64>) -> (u32, Option<usize>) {\n    (keep!(narrow(1)), keep!(v.map(|v| crate::num::widen(v))))\n}\n",
        ),
        (
            "src/plain.rs",
            "pub fn f(v: u64) -> usize {\n    crate::num::widen(v)\n}\n",
        ),
        (
            "src/formatted.rs",
            "pub fn f(v: u64) -> String {\n    assert!(v > 0, \"{}\", v);\n    let _ = vec![1, v as u8];\n    format!(\"{} {}\", v, crate::num::widen(v))\n}\n",
        ),
        (
            "src/aliased.rs",
            "use crate::num::narrow as small;\nmacro_rules! keep {\n    ($e:expr) => { $e };\n}\npub fn f(v: u64) -> (u32, usize) {\n    (keep!(small(1)), crate::num::widen(v))\n}\n",
        ),
    ]);
    let root = dir.0.as_path();
    assert_eq!(ingest_fixture(root, None).class, ExitClass::Success);
    let out = ask(root, "impact", Some("src/num.rs#narrow"), |o| {
        o.depth = Some(1);
    });
    let risk = |file: &str| {
        out.value["rows"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|row| row["id"] == format!("app/src/{file}.rs"))
            .map(|row| row["risk"].as_str().unwrap_or_default().to_owned())
    };
    assert_eq!(
        risk("outer/macroed").as_deref(),
        Some("candidate"),
        "macro arguments may call narrow: {}",
        out.value
    );
    // The renaming `use` itself links `narrow` (direct); a macro group that
    // writes the alias never drops the importer.
    assert!(
        risk("aliased").is_some(),
        "`small` renames narrow inside a macro group: {}",
        out.value
    );
    assert_eq!(
        risk("plain"),
        None,
        "plain names only widen, all extracted: {}",
        out.value
    );
    assert_eq!(
        risk("formatted"),
        None,
        "its `format!`, `assert!` and `vec!` groups name no changed item: {}",
        out.value
    );
}

/// A module file another crate also mounts by `#[path]` (a fuzz target, a
/// bench sharing the source) keeps its home crate's links: `crate::` and
/// `super::` paths resolve through the module tree that declares it with a
/// plain `mod`, so its imports, calls and callers stay linked.
#[test]
fn a_path_mounted_module_keeps_its_home_crates_links() {
    let dir = git_app(&[
        (
            "Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
        ),
        ("src/lib.rs", "pub mod num;\npub mod text;\n"),
        (
            "src/num.rs",
            "pub fn saturate(v: u64) -> u32 { v as u32 }\n",
        ),
        ("src/text.rs", "mod offsets;\npub use offsets::width;\n"),
        (
            "src/text/offsets.rs",
            "use crate::num::saturate;\npub fn width(v: u64) -> u32 { saturate(v) }\n",
        ),
        (
            "fuzz/Cargo.toml",
            "[package]\nname = \"app-fuzz\"\nversion = \"0.0.0\"\n\n[[bin]]\nname = \"offsets\"\npath = \"fuzz_targets/offsets.rs\"\n",
        ),
        (
            "fuzz/fuzz_targets/offsets.rs",
            "#[path = \"../../src/text/offsets.rs\"]\nmod offsets;\nfn main() { offsets::width(1); }\n",
        ),
    ]);
    let root = dir.0.as_path();
    assert_eq!(ingest_fixture(root, None).class, ExitClass::Success);
    let deps = ask(root, "deps", Some("src/text/offsets.rs"), |_| {});
    assert!(
        ids(&deps).iter().any(|id| id == "app/src/num.rs"),
        "{}",
        deps.value
    );
    let callers = ask(root, "callers", Some("app/src/num.rs#saturate"), |_| {});
    assert_eq!(
        ids(&callers),
        ["app/src/text/offsets.rs#width"],
        "{}",
        callers.value
    );
    let diagnostics = ask(root, "diagnostics", Some("unresolved-internal"), |_| {});
    assert!(
        !diagnostics.value["rows"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|row| row["path"] == "app/src/text/offsets.rs"),
        "{}",
        diagnostics.value
    );
}
