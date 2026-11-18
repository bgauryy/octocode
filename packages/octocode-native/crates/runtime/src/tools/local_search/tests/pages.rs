use octocode_runtime_base::execution::cancel::NeverCancel;
use octocode_tool_local::test_support::ls_query;
use octocode_tool_local::test_support::policy_for;
use octocode_tool_local::test_support::search_in;

use std::fs;

/// A declaration in localSearch `declarations[]` is its identity and one
/// locator: `symbolName`, `kind`, `line` (the name line astSearch `symbols`
/// prints) and `endLine`; `line`-`endLine` is the localFetch range that reads
/// it. Its attribute and doc lines stay with astSearch `symbols`, so a row
/// carries neither `startLine` nor `docStartLine`.
#[test]
fn declarations_carry_one_locator_and_no_start_lines() {
    let root = tempfile::tempdir().expect("fixture directory");
    let source = root.path().join("a.rs");
    let text = "/// Checks the needle.\n/// Twice.\n#[test]\n#[allow(dead_code)]\nfn checked() {\n    needle();\n}\nfn plain() {\n    needle();\n}\n";
    fs::write(&source, text).expect("fixture");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy(), "matchString":["needle"]}),
        None,
    );
    let result = search_in(&request, &policy, &security, None)
        .expect("search")
        .to_row()
        .expect("serialize");
    let declarations = result["files"][0]["declarations"]
        .as_array()
        .expect("declarations")
        .clone();
    let symbols_row = octocode_runtime_base::test_support::opened(
        octocode_runtime_base::id::ToolId::AstSearch,
        &serde_json::json!({"operation":"symbols","path":source}),
    );
    let symbols = octocode_tool_ast::ast_search::execute_ast(
        octocode_runtime_base::test_support::BUILD_IDENTITY,
        &serde_json::from_value(symbols_row).expect("astSearch row"),
        &policy,
        &security,
        &NeverCancel,
        None,
    )
    .expect("symbols");
    let symbol = |name: &str| {
        symbols["symbols"]
            .as_array()
            .expect("symbols rows")
            .iter()
            .find(|row| row["symbolName"] == name)
            .cloned()
            .unwrap_or_else(|| panic!("{name} in {symbols}"))
    };
    assert_eq!(declarations.len(), 2, "{result}");
    let source_lines: Vec<&str> = text.lines().collect();
    for (declaration, (name, line, end)) in declarations
        .iter()
        .zip([("checked", 5, 7), ("plain", 8, 10)])
    {
        let fields: Vec<&str> = declaration
            .as_object()
            .expect("declaration row")
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            fields,
            ["symbolName", "kind", "line", "endLine"],
            "{declaration}"
        );
        assert_eq!(declaration["symbolName"], name, "{declaration}");
        assert_eq!(declaration["line"], line, "{declaration}");
        assert_eq!(declaration["endLine"], end, "{declaration}");
        assert_eq!(declaration["line"], symbol(name)["line"], "{declaration}");
        assert_eq!(
            declaration["endLine"],
            symbol(name)["endLine"],
            "{declaration}"
        );
        // The range reads the declaration whole: its header to its closing brace.
        assert!(source_lines[line - 1].starts_with(&format!("fn {name}(")));
        assert_eq!(source_lines[end - 1], "}");
    }
}
