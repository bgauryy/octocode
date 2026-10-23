//! Rust modules, `use` trees, attributes and item-level macro bodies.

use super::*;

#[test]
fn rust_cfg_and_path_attributes_survive_comments_and_inner_attributes() {
    let value = facts(
        "#[cfg(feature = \"x\")]\n// note\nmod child;\n#[path = \"actual.rs\"]\n/// documented\nmod alias;\nmod gated { #![cfg(feature = \"x\")] use super::Thing; }",
        "src/lib.rs",
    );
    assert!(
        value["modules"]
            .as_array()
            .unwrap()
            .iter()
            .any(|module| module["name"] == "child" && module["unsupported"] != true)
    );
    assert!(
        value["modules"]
            .as_array()
            .unwrap()
            .iter()
            .any(|module| module["name"] == "alias" && module["path"] == "actual.rs")
    );
    // `cfg` gates compilation, not the module's file: the edge stays.
    assert!(
        value["modules"]
            .as_array()
            .unwrap()
            .iter()
            .any(|module| module["name"] == "gated" && module["unsupported"] != true)
    );
    let root = facts("#![cfg(feature = \"x\")]\nmod child;", "src/lib.rs");
    assert_ne!(root["rustRootUnsupported"], true);
    // A conditional attribute that rewrites the path is still unknown.
    let rewritten = facts(
        "#[cfg_attr(unix, path = \"u.rs\")] mod child;",
        "src/lib.rs",
    );
    assert!(
        rewritten["modules"]
            .as_array()
            .unwrap()
            .iter()
            .all(|module| module["unsupported"] == true)
    );
    let local = facts(
        "fn f() { mod hidden { #[path = \"child.rs\"] mod child; } }",
        "src/lib.rs",
    );
    assert!(
        local["modules"]
            .as_array()
            .unwrap()
            .iter()
            .all(|module| module["unsupported"] == true)
    );
}

#[test]
fn rust_modules_preserve_literal_paths_inline_scopes_and_unknown_cfg() {
    let value = facts(
        "#[path = \"actual.rs\"] mod alias;\nmod inside { use super::Thing; #[path = \"nested.rs\"] mod child; }\n#[cfg(feature = \"x\")] mod conditional;",
        "src/lib.rs",
    );
    let modules = value["modules"].as_array().unwrap();
    assert!(modules.iter().any(|module| module["name"] == "alias"
        && module["path"] == "actual.rs"
        && module["unsupported"] != true));
    assert!(modules.iter().any(|module| module["name"] == "child"
        && module["scope"] == serde_json::json!(["inside"])
        && module["path"] == "nested.rs"));
    assert!(
        modules
            .iter()
            .any(|module| module["name"] == "conditional" && module["unsupported"] != true)
    );
    assert!(
        value["imports"]
            .as_array()
            .unwrap()
            .iter()
            .any(|import| import["specifier"] == "super::Thing"
                && import["moduleScope"] == serde_json::json!(["inside"])
                && import["resolutionHint"].is_null())
    );
}

#[test]
fn declaration_occurrences_do_not_alias_equal_names_or_impl_blocks() {
    for (source, path) in [
        (
            "struct A; impl A { fn run() { work(); } } struct B; impl B { fn run() { work(); } }",
            "names.rs",
        ),
        (
            "class A:\n def run(self): work()\nclass B:\n def run(self): work()\n",
            "names.py",
        ),
    ] {
        let value = facts(source, path);
        let declarations = value["declarations"].as_array().unwrap();
        let ids: std::collections::HashSet<_> = declarations
            .iter()
            .map(|d| d["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids.len(), declarations.len());
        for edge in value["edges"].as_array().unwrap() {
            if edge["relation"] == "calls" {
                assert!(ids.contains(edge["from"].as_str().unwrap()));
                assert!(edge["to"].as_str().unwrap().starts_with("reference:"));
                assert_eq!(edge["resolution"], "unresolved");
            }
        }
    }
}

#[test]
fn rust_use_trees_expand_multiline_groups_aliases_and_modules() {
    let value = facts(
        "mod child;\nuse super::{\n types::{Thing as Alias, Other},\n language::AgLanguage,\n};\n",
        "src/structural/files.rs",
    );
    let imports = value["imports"].as_array().unwrap();
    for expected in [
        "self::child",
        "super::types::Thing",
        "super::types::Other",
        "super::language::AgLanguage",
    ] {
        assert!(
            imports.iter().any(|i| i["specifier"] == expected),
            "missing {expected}: {imports:?}"
        );
    }
    assert!(
        imports
            .iter()
            .any(|i| i["localName"] == "Alias" && i["importedName"] == "Thing")
    );
}

#[test]
fn use_heavy_rust_file_keeps_its_imports_inside_the_deadline() {
    // Per-`use` `parent()`/sibling walks made this O(n²) and the
    // walk hit the deadline, discarding every import.
    let mut source = String::from("mod item0 { pub struct Name; }\n");
    for index in 0..6_000 {
        source.push_str(&format!("use crate::item{index}::Name;\n"));
    }
    let (value, elapsed) =
        crate::test_cpu::cpu_time_of(|| facts_with_load_proof_deadline(&source, "src/lib.rs"));
    let diagnostics = value["diagnostics"].as_array().unwrap();
    assert!(
        !codes(&value).iter().any(|code| code.ends_with("Deadline")),
        "{diagnostics:?}"
    );
    let imports = value["imports"].as_array().unwrap();
    assert_eq!(imports.len(), 6_000);
    assert_eq!(imports[5_999]["specifier"], "crate::item5999::Name");
    assert_eq!(imports[0]["moduleScope"], serde_json::json!([]));
    assert!(imports.iter().all(|i| i.get("resolutionHint").is_none()));
    assert!(
        elapsed < crate::signatures::extractor::AST_EXECUTION_TIMEOUT / 2,
        "took {elapsed:?}"
    );
}

#[test]
fn rust_item_macro_bodies_are_read_as_items_and_expression_macros_are_not_gaps() {
    // tokio-style `cfg_rt! { ... }` wrappers hold real mod/use items.
    let value = facts(
        "cfg_rt! {\n    pub mod runtime;\n    pub use crate::runtime::Handle;\n}\nmod outer { cfg_net! { mod tcp; } }\nfn f() { println!(\"{}\", 1); let v = vec![1]; }\n",
        "src/lib.rs",
    );
    let modules = value["modules"].as_array().unwrap();
    assert!(
        modules
            .iter()
            .any(|m| m["name"] == "runtime" && m["line"] == 2),
        "{modules:?}"
    );
    assert!(
        modules
            .iter()
            .any(|m| m["name"] == "tcp" && m["scope"] == serde_json::json!(["outer"])),
        "{modules:?}"
    );
    assert!(
        value["imports"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["specifier"] == "crate::runtime::Handle")
    );
    assert!(
        !codes(&value).contains(&"rustMacroExpansion".to_owned()),
        "{}",
        value["diagnostics"]
    );
}

#[test]
fn rust_nonconventional_modules_and_macros_remain_explicitly_unsupported() {
    let value = facts(
        "#[cfg_attr(unix, path = \"other.rs\")] mod child;\nmod inline { use super::Thing; }\nmake_imports!();",
        "src/lib.rs",
    );
    let imports = value["imports"].as_array().unwrap();
    assert!(
        imports
            .iter()
            .any(|item| item["specifier"] == "self::child"
                && item["resolutionHint"] == "unsupported")
    );
    assert!(
        imports
            .iter()
            .any(|item| item["specifier"] == "super::Thing"
                && item["moduleScope"] == serde_json::json!(["inline"]))
    );
    assert!(codes(&value).contains(&"rustMacroExpansion".to_owned()));
}
