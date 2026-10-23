//! Rust module-path calls, re-exports, evidence lines and includes.
use super::*;

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
        let rows = out.value["results"].as_array().expect("rows");
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
    let rows = out.value["results"].as_array().expect("rows");
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
    let rows = deps.value["results"].as_array().expect("rows");
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
    let rows = deps.value["results"].as_array().expect("rows");
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
