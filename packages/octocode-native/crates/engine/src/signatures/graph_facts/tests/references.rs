//! Value references, format captures and exact import ranges.

use super::*;

#[test]
fn rust_paths_outside_calls_and_uses_are_facts() {
    let source = "use crate::util::run;\nconst A: u32 = crate::limits::MAX;\nfn f() -> crate::types::Out {\n    let g = crate::rules::globs;\n    crate::calls::direct();\n    json!({\"x\": crate::macros::recovery(a), \"y\": $crate::m::Z});\n    let _ = Self::new;\n    mod inner { fn h() { super::super::up::X; } }\n}\n";
    let facts = extract_graph_facts_with_metadata(source, "src/lib.rs")
        .expect("graph facts")
        .facts;
    let paths = facts
        .paths
        .iter()
        .map(|p| {
            (
                p.path.as_str(),
                p.line,
                p.caller_id.is_some(),
                p.inline_depth,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        paths,
        [
            ("crate::limits::MAX", 2, true, 0),
            ("crate::types::Out", 3, true, 0),
            ("crate::rules::globs", 4, true, 0),
            // `crate::macros::recovery(a)` in a `json!` value is a call, read as one.
            ("crate::m::Z", 6, true, 0),
            ("super::super::up::X", 8, true, 1),
        ]
    );
}

/// A bare type name written in type or struct-literal position is a fact,
/// by first line; declarations, generic parameters, `Self` and scoped types
/// are not.
#[test]
fn rust_bare_type_names_in_type_position_are_facts() {
    let source = "struct Holder<T> {\n    field: FieldTy,\n    param: T,\n}\nenum Kind {\n    A(VariantTy),\n}\ntrait Tr<U: Bound> {\n    type Out;\n    fn go(&self) -> Self;\n}\nimpl Holder<GenericArg> {\n    fn f(_: &ParamTy) -> RetTy {\n        let _ = LitTy { a: 1 };\n        let _: a::Scoped = todo!();\n        todo!()\n    }\n}\nimpl Tr<()> for ImplTy {}\nfn g<W>(_: Option<Arc<Nested>>, _: W) -> impl Iterator<Item = ItemTy> {}\n";
    let facts = extract_graph_facts_with_metadata(source, "src/lib.rs")
        .expect("graph facts")
        .facts;
    let names = facts
        .type_names
        .iter()
        .map(|t| (t.name.as_str(), t.line))
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        [
            ("Arc", 20),
            ("Bound", 8),
            ("FieldTy", 2),
            ("GenericArg", 12),
            ("Holder", 12),
            ("ImplTy", 19),
            ("ItemTy", 20),
            ("Iterator", 20),
            ("LitTy", 14),
            ("Nested", 20),
            ("Option", 20),
            ("ParamTy", 13),
            ("RetTy", 13),
            ("T", 3),
            ("Tr", 19),
            ("VariantTy", 6),
            ("W", 20),
        ]
    );
}

/// A restricted visibility names where an item is visible, not a use of
/// that module: `pub(in crate::local_search)` records no path.
#[test]
fn a_restricted_visibility_path_is_no_reference() {
    let source = "pub(in crate::local_search) enum Proof { A }\npub(in crate::local_search) fn f() -> crate::types::Out { todo!() }\n";
    let facts = extract_graph_facts_with_metadata(source, "src/guard.rs")
        .expect("graph facts")
        .facts;
    let paths = facts
        .paths
        .iter()
        .map(|p| p.path.as_str())
        .collect::<Vec<_>>();
    assert_eq!(paths, ["crate::types::Out"]);
}

#[test]
fn rust_comments_and_strings_are_not_references() {
    let source = "/// `helper` is documented; see helper.\npub fn helper() {}\n// helper\n/* helper */\npub fn caller() -> &'static str { \"helper\" }\n";
    assert_eq!(reference_count(source, "lib.rs", "helper"), 0);
}

#[test]
fn rust_value_uses_and_format_captures_count_but_calls_do_not() {
    let source = "pub fn callback() {}\npub fn called() {}\npub const LIMIT: u32 = 1;\npub struct Svc;\nimpl Svc { pub fn run(&self) {} pub fn go(&self) { self.run(); called(); register(callback); println!(\"{LIMIT} {{LIMIT}}\"); } }\n";
    assert_eq!(reference_count(source, "lib.rs", "callback"), 1);
    assert_eq!(reference_count(source, "lib.rs", "called"), 0);
    assert_eq!(reference_count(source, "lib.rs", "run"), 0);
    assert_eq!(reference_count(source, "lib.rs", "LIMIT"), 1);
    assert_eq!(
        reference_count(source, "lib.rs", "Svc"),
        0,
        "the impl names the struct as a declaration, not a use"
    );
}

#[test]
fn inline_format_captures_skip_escapes_and_positional_arguments() {
    assert_eq!(
        inline_format_captures("\"{a} {b:?} {{c}} {0} {} {_d:>4}\""),
        ["a", "b", "_d"]
    );
}

#[test]
fn rust_import_ranges_do_not_invent_synthetic_name_tokens() {
    let value = facts(
        "use crate::origin::{self as module_alias, *};\nuse crate::origin::plain;\n",
        "imports.rs",
    );
    let imports = value["imports"].as_array().unwrap();
    assert!(imports[0].get("importedRange").is_none());
    assert!(imports[0].get("localRange").is_some());
    assert!(imports[1].get("importedRange").is_none());
    assert!(imports[1].get("localRange").is_none());
    assert_eq!(imports[2]["localRange"], imports[2]["importedRange"]);
    assert!(imports[2].get("importedRange").is_some());
}

#[test]
fn rust_import_binding_ranges_are_exact_utf16() {
    let value = facts(
        "/*😀*/ use crate::origin::{target as first, target as second};\nuse crate::origin::{\n target as third,\n};\nuse crate::origin::target as fourth;\n",
        "aliases.rs",
    );
    let imports = value["imports"].as_array().unwrap();
    assert_eq!(imports.len(), 4);
    for (index, (line, imported, local, length)) in [
        (0, 27, 37, 5),
        (0, 44, 54, 6),
        (2, 1, 11, 5),
        (4, 19, 29, 6),
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            imports[index]["importedRange"],
            serde_json::json!({"start":{"line":line,"character":imported},"end":{"line":line,"character":imported+6}})
        );
        assert_eq!(
            imports[index]["localRange"],
            serde_json::json!({"start":{"line":line,"character":local},"end":{"line":line,"character":local+length}})
        );
    }
}

#[test]
fn python_import_ranges_do_not_invent_synthetic_name_tokens() {
    let value = facts(
        "import package.sub as alias\nfrom origin import plain\nfrom origin import *\n",
        "imports.py",
    );
    let imports = value["imports"].as_array().unwrap();
    assert!(imports[0].get("importedRange").is_none());
    assert_eq!(
        imports[0]["localRange"],
        serde_json::json!({"start":{"line":0,"character":22},"end":{"line":0,"character":27}})
    );
    assert_eq!(imports[1]["localRange"], imports[1]["importedRange"]);
    assert!(imports[2].get("importedRange").is_none());
    assert!(imports[2].get("localRange").is_none());
}

#[test]
fn python_import_binding_ranges_are_exact_utf16() {
    let value = facts(
        "marker = \"😀\"; from origin import target as first, target as second\nfrom origin import (\n    target as third,\n)\n",
        "aliases.py",
    );
    let imports = value["imports"].as_array().unwrap();
    assert_eq!(imports.len(), 3);
    for (index, (line, imported, local, length)) in [(0, 34, 44, 5), (0, 51, 61, 6), (2, 4, 14, 5)]
        .into_iter()
        .enumerate()
    {
        assert_eq!(
            imports[index]["importedRange"],
            serde_json::json!({"start":{"line":line,"character":imported},"end":{"line":line,"character":imported+6}})
        );
        assert_eq!(
            imports[index]["localRange"],
            serde_json::json!({"start":{"line":line,"character":local},"end":{"line":line,"character":local+length}})
        );
    }
}
