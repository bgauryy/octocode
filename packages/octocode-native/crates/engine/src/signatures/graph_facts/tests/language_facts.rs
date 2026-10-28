//! Per-language declarations, calls and import facts.

use super::*;

#[test]
fn rust_graph_facts_include_pub_declarations_and_calls() {
    let src = r#"
use crate::other::helper;

pub struct Point {
    x: f64,
}

pub fn distance(point: Point) -> f64 {
    helper(point.x)
}
"#;
    let graph = facts(src, "geo.rs");
    assert_eq!(
        graph["schemaVersion"],
        crate::signatures::GRAPH_FACTS_SCHEMA_VERSION
    );
    assert!(
        graph
            .get("language")
            .is_some_and(|language| language == "rust")
    );
    assert!(
        graph
            .get("declarations")
            .and_then(Value::as_array)
            .is_some_and(|decls| decls
                .iter()
                .any(|decl| decl.get("name").is_some_and(|name| name == "Point")))
    );
    assert!(
        graph
            .get("declarations")
            .and_then(Value::as_array)
            .is_some_and(|decls| decls.iter().any(|decl| decl
                .get("name")
                .is_some_and(|name| name == "distance")
                && decl
                    .get("exported")
                    .is_some_and(|exported| exported == true)))
    );
    assert!(
        graph
            .get("imports")
            .and_then(Value::as_array)
            .is_some_and(|imports| imports.iter().any(|import| import
                .get("specifier")
                .and_then(Value::as_str)
                .is_some_and(|specifier| specifier.contains("crate::other"))))
    );
    assert!(
        graph
            .get("calls")
            .and_then(Value::as_array)
            .is_some_and(|calls| calls
                .iter()
                .any(|call| call.get("callee").is_some_and(|callee| callee == "helper")))
    );
}

#[test]
fn python_import_facts_preserve_module_paths_and_alias_bindings() {
    let graph = facts(
        "import os, pkg.worker as worker\nfrom .target import run as execute\nfrom . import sibling\n",
        "pkg/service.py",
    );
    let imports = graph["imports"].as_array().unwrap();
    assert_eq!(imports.len(), 4);
    assert_eq!(imports[0]["specifier"], "os");
    assert_eq!(imports[1]["specifier"], "pkg.worker");
    assert_eq!(imports[1]["localName"], "worker");
    assert_eq!(imports[1]["resolutionHint"], "python-absolute");
    assert_eq!(imports[2]["specifier"], ".target");
    assert_eq!(imports[2]["importedName"], "run");
    assert_eq!(imports[2]["localName"], "execute");
    assert_eq!(imports[2]["resolutionHint"], "python-relative");
    assert_eq!(imports[3]["specifier"], ".");
    assert_eq!(imports[3]["importedName"], "sibling");
}

#[test]
fn c_import_facts_distinguish_quoted_system_and_computed_headers() {
    let graph = facts(
        "#include \"local.h\"\n#include <system.h>\n#include HEADER\n",
        "entry.c",
    );
    let imports = graph["imports"].as_array().unwrap();
    assert_eq!(imports.len(), 3);
    assert_eq!(imports[0]["specifier"], "local.h");
    assert_eq!(imports[0]["resolutionHint"], "c-relative");
    assert_eq!(imports[1]["resolutionHint"], "c-system");
    assert_eq!(imports[2]["resolutionHint"], "unsupported");
}

/// A file-inclusion macro with a literal path is an `include` import of
/// that path; a computed path or another macro is none.
#[test]
fn rust_include_macros_name_their_literal_file() {
    let graph = facts(
        "include!(\"gen/types.rs\");\nconst T: &str = include_str!(\"../data.txt\");\n\
         include!(concat!(env!(\"OUT_DIR\"), \"/x.rs\"));\nfn f() { println!(\"a.rs\"); }\n",
        "lib.rs",
    );
    let includes: Vec<_> = graph["imports"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|import| {
            matches!(
                import["importKind"].as_str(),
                Some("include" | "include-data")
            )
        })
        .map(|import| {
            (
                import["importKind"].clone(),
                import["specifier"].clone(),
                import["line"].clone(),
            )
        })
        .collect();
    assert_eq!(
        includes,
        [
            (
                serde_json::json!("include"),
                serde_json::json!("gen/types.rs"),
                serde_json::json!(1)
            ),
            (
                serde_json::json!("include-data"),
                serde_json::json!("../data.txt"),
                serde_json::json!(2)
            ),
        ]
    );
}

#[test]
fn python_graph_facts_include_module_public_defs() {
    let src = r#"
import os

class Service:
    def run(self):
        helper()

def helper():
    return os.getcwd()
"#;
    let graph = facts(src, "service.py");
    assert!(
        graph
            .get("language")
            .is_some_and(|language| language == "python")
    );
    assert!(
        graph
            .get("declarations")
            .and_then(Value::as_array)
            .is_some_and(|decls| decls
                .iter()
                .any(|decl| decl.get("name").is_some_and(|name| name == "Service")))
    );
    assert!(
        graph
            .get("declarations")
            .and_then(Value::as_array)
            .is_some_and(|decls| decls.iter().any(|decl| decl
                .get("name")
                .is_some_and(|name| name == "helper")
                && decl
                    .get("exported")
                    .is_some_and(|exported| exported == true)))
    );
    assert!(
        graph
            .get("imports")
            .and_then(Value::as_array)
            .is_some_and(|imports| imports.iter().any(|import| import
                .get("specifier")
                .and_then(Value::as_str)
                .is_some_and(|specifier| specifier.contains("os"))))
    );
    assert!(
        graph
            .get("calls")
            .and_then(Value::as_array)
            .is_some_and(|calls| calls
                .iter()
                .any(|call| call.get("callee").is_some_and(|callee| callee == "helper")))
    );
}

/// A language's alias extensions (`.pyi` stubs, C++ `.hpp`) read exactly as
/// its first extension: exports, heritage and imports dispatch on one
/// spelling.
#[test]
fn an_alias_extension_yields_the_facts_of_its_canonical_extension() {
    let python = "import os\nfrom pkg import thing\n\nclass Base:\n    pass\n\nclass Child(Base):\n    def run(self):\n        pass\n\ndef _private():\n    pass\n";
    #[cfg(feature = "tree-sitter-cpp")]
    let cpp = "#include <vector>\nclass Shape {};\nclass Circle : public Shape {};\n";
    let assert_alias = |canonical: &str, alias: &str, source: &str| {
        // Only the file name (in the file field and every id) may differ.
        let spelled = |path: &str| facts(source, path).to_string().replace(path, "FILE");
        assert_eq!(spelled(canonical), spelled(alias), "facts of {alias}");
    };
    assert_alias("a.py", "a.pyi", python);
    #[cfg(feature = "tree-sitter-cpp")]
    assert_alias("a.cpp", "a.hpp", cpp);
    let exported = |graph: &Value, name: &str| {
        graph["declarations"]
            .as_array()
            .and_then(|all| all.iter().find(|d| d["name"] == name))
            .map(|d| d["exported"].clone())
    };
    let stub = facts(python, "a.pyi");
    assert_eq!(exported(&stub, "Child"), Some(Value::Bool(true)));
    assert_eq!(exported(&stub, "_private"), Some(Value::Bool(false)));
    assert!(
        heritage(python, "a.pyi")
            .iter()
            .any(|(relation, from, to, _)| relation == "extends"
                && from.ends_with("Child")
                && to == "Base"),
        "a stub's class bases are read"
    );
}
