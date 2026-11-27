//! Module-level calls, C# `using`, error spans and typed diagnostics.

use super::*;

#[test]
fn module_level_calls_have_no_caller_id() {
    let source = "import app\n\n@app.route('/')\ndef index():\n    helper()\n\nsetup()\nif __name__ == '__main__':\n    index()\n";
    let facts = facts_json(source, "main.py");
    let calls: Vec<(String, String, bool)> = facts["calls"]
        .as_array()
        .expect("calls")
        .iter()
        .map(|call| {
            (
                call["caller"].as_str().unwrap_or_default().to_owned(),
                call["callee"].as_str().unwrap_or_default().to_owned(),
                call.get("callerId").is_some(),
            )
        })
        .collect();
    assert_eq!(
        calls,
        vec![
            ("module".to_owned(), "app.route".to_owned(), false),
            ("index".to_owned(), "helper".to_owned(), true),
            ("module".to_owned(), "setup".to_owned(), false),
            ("module".to_owned(), "index".to_owned(), false),
        ]
    );
    let module_edge = facts["edges"]
        .as_array()
        .expect("edges")
        .iter()
        .find(|edge| edge["relation"] == "calls" && edge["line"] == 7)
        .expect("module call edge");
    assert_eq!(module_edge["from"], "file:main.py");
    // A module-level call target is an edge, not a value reference.
    assert_eq!(reference_count(source, "main.py", "index"), 0);
}

#[test]
fn module_level_calls_cover_go_rust_and_c() {
    let callers = |source: &str, path: &str| -> Vec<(String, String)> {
        facts_json(source, path)["calls"]
            .as_array()
            .expect("calls")
            .iter()
            .map(|call| {
                (
                    call["caller"].as_str().unwrap_or_default().to_owned(),
                    call["callee"].as_str().unwrap_or_default().to_owned(),
                )
            })
            .collect()
    };
    assert!(
        callers("package p\n\nvar x = build()\n", "p.go")
            .contains(&("module".to_owned(), "build".to_owned()))
    );
    assert!(
        callers(
            "lazy_static! { static ref X: u8 = 1; }\nfn f() { g(); }\n",
            "lib.rs"
        )
        .contains(&("module".to_owned(), "lazy_static".to_owned()))
    );
    let c_family_call = |path: &str| {
        assert!(
            callers("int f(void);\nint x = f();\n", path)
                .contains(&("module".to_owned(), "f".to_owned())),
            "{path}"
        );
    };
    c_family_call("a.c");
    #[cfg(feature = "tree-sitter-cpp")]
    c_family_call("a.cpp");
}

#[cfg(feature = "tree-sitter-c-sharp")]
#[test]
fn csharp_using_directives_are_imports() {
    let source = "using System.Text;\nusing static System.Math;\nglobal using System;\nusing Json = Newtonsoft.Json.JsonConvert;\nnamespace App { using Inner.Pkg; class A { void M() { using (var x = Open()) {} } } }\n";
    let facts = facts_json(source, "A.cs");
    let imports: Vec<(String, Option<String>, u64)> = facts["imports"]
        .as_array()
        .expect("imports")
        .iter()
        .map(|import| {
            (
                import["specifier"].as_str().unwrap_or_default().to_owned(),
                import["localName"].as_str().map(str::to_owned),
                import["line"].as_u64().unwrap_or_default(),
            )
        })
        .collect();
    assert_eq!(
        imports,
        vec![
            ("System.Text".to_owned(), None, 1),
            ("System.Math".to_owned(), None, 2),
            ("System".to_owned(), None, 3),
            (
                "Newtonsoft.Json.JsonConvert".to_owned(),
                Some("Json".to_owned()),
                4
            ),
            ("Inner.Pkg".to_owned(), None, 5),
        ]
    );
}

/// A recovered parse names the 1-based line spans its syntax errors cover,
/// so a consumer can tell whether a missing declaration could hide there.
#[test]
fn recovered_parse_reports_error_line_spans() {
    let source = "int ok(void) { return 1; }\n\nint broken( {\n  return 2;\n}\nint tail(void) { return 3; }\n";
    let facts = extract_graph_facts_with_metadata(source, "a.c")
        .expect("graph facts")
        .facts;
    assert!(
        facts
            .diagnostics
            .iter()
            .any(|note| note.code == crate::graph::GraphFactCode::RecoveredParse),
        "{facts:?}"
    );
    assert!(!facts.error_lines.is_empty(), "{facts:?}");
    assert!(
        facts
            .error_lines
            .iter()
            .all(|[start, end]| *start >= 3 && start <= end),
        "{:?}",
        facts.error_lines
    );
    assert!(
        !facts
            .error_lines
            .iter()
            .any(|[start, end]| *start <= 1 && 1 <= *end),
        "line 1 parses: {:?}",
        facts.error_lines
    );
    let clean = extract_graph_facts_with_metadata("int ok(void) { return 1; }\n", "b.c")
        .expect("graph facts")
        .facts;
    assert!(clean.error_lines.is_empty());
    let json = serde_json::to_value(&clean).expect("json");
    assert!(json.get("errorLines").is_none(), "{json}");
}

/// An error that runs to the end of a file with a trailing newline ends on
/// its last line, never the line after it (an end at column 0 is the
/// previous line's newline).
#[test]
fn an_error_reaching_the_file_end_ends_on_the_last_line() {
    for (path, source) in [
        (
            "a.c",
            "int ok(void) { return 1; }\nint broken( {\n  return 2;\n  more(;\n",
        ),
        (
            "a.js",
            "// @flow\ntype T = {| a: number |};\nconst x: Array<T> = [\n  {a: 1},\n",
        ),
    ] {
        let lines = u32::try_from(source.lines().count()).expect("lines");
        let facts = extract_graph_facts_with_metadata(source, path)
            .expect("graph facts")
            .facts;
        assert!(!facts.error_lines.is_empty(), "{path}: {facts:?}");
        assert!(
            facts.error_lines.iter().all(|[_, end]| *end <= lines),
            "{path}: {:?} past line {lines}",
            facts.error_lines
        );
        assert!(
            facts.error_lines.iter().any(|[_, end]| *end == lines),
            "{path}: {:?} reaches line {lines}",
            facts.error_lines
        );
    }
}

#[test]
fn rust_macro_body_members_nest_under_their_container_in_source_order() {
    // tokio's `impl NamedPipeServer { … cfg_io_util! { pub fn try_read_buf … } … }`:
    // a member generated inside an item-level macro of an impl body belongs to
    // that impl, as a method, at its source position.
    let value = facts(
        "struct S;\nimpl S {\n    fn a() {}\n    cfg_x! {\n        pub fn b() {}\n    }\n    fn c() {}\n}\nimpl T for S {\n    cfg_y! { fn d() {} }\n}\nfn z() {}\ncfg_z! { fn top() {} }\n",
        "src/lib.rs",
    );
    let declarations = value["declarations"].as_array().unwrap();
    let names: Vec<&str> = declarations
        .iter()
        .map(|d| d["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["S", "S", "a", "b", "c", "S", "d", "z", "top"]);
    let by_name = |name: &str| declarations.iter().find(|d| d["name"] == name).unwrap();
    assert_eq!(by_name("b")["parent"], declarations[1]["id"]);
    assert_eq!(by_name("b")["kind"], "method");
    assert_eq!(by_name("d")["parent"], declarations[5]["id"]);
    assert_eq!(by_name("d")["kind"], "method");
    assert!(by_name("top")["parent"].is_null());
    assert_eq!(by_name("top")["kind"], "function");
}

/// Every diagnostic carries its code; a module's attribute note names its
/// line in a field, not in its text.
#[test]
fn graph_fact_diagnostics_carry_typed_codes() {
    let broken = facts("fn f( {\n", "lib.rs");
    assert_eq!(codes(&broken), ["syntaxOnly", "recoveredParse"], "{broken}");
    let graph = facts("#![custom]\n#[custom]\nmod t;\n", "lib.rs");
    let module = graph["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|note| note["code"] == "rustModuleAttributes")
        .unwrap_or_else(|| panic!("{graph}"));
    assert_eq!(module["line"], 3, "{graph}");
    assert!(
        !module["message"].as_str().unwrap().contains("line"),
        "{graph}"
    );
    assert_eq!(
        codes(&graph),
        ["syntaxOnly", "rustCrateAttributes", "rustModuleAttributes"],
        "{graph}"
    );
    use crate::graph::GraphFactCode;
    assert!(GraphFactCode::RustModuleAttributes.is_linking_only());
    assert!(!GraphFactCode::RecoveredParse.is_linking_only());
    assert!(!GraphFactCode::TraversalDeadline.is_linking_only());
}
