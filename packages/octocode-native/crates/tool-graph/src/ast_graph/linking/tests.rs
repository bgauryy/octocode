use super::{Linker, LinkerGraph};

/// Every spelling of a grammar's extensions links alike, from the
/// registry: a `.hxx` header like a `.c` file, `.pyi` like `.py`; an
/// extension no compiled grammar parses links nothing.
#[test]
fn linkers_follow_the_registry_grammar() {
    for (file, linker) in [
        ("a.ts", Linker::JsTs),
        ("a.d.ts", Linker::JsTs),
        ("a.mjs", Linker::JsTs),
        ("a.tsx", Linker::JsTs),
        ("a.pyi", Linker::Python),
        ("a.rs", Linker::Rust),
        ("a.go", Linker::Go),
        ("A.java", Linker::Java),
        ("a.h", Linker::CFamily),
        ("a.hxx", Linker::CFamily),
        ("a.cs", Linker::Unsupported),
        ("README", Linker::Unsupported),
    ] {
        assert_eq!(Linker::of(file), linker, "{file}");
    }
    let cuda = octocode_engine::signatures::languages::find_entry("cuh").is_some();
    assert_eq!(
        Linker::of("a.cuh"),
        if cuda {
            Linker::CFamily
        } else {
            Linker::Unsupported
        }
    );
    assert_eq!(Linker::Rust.import_edge("module"), "rust-module");
    assert_eq!(Linker::JsTs.import_edge("type"), "type-import");
}
