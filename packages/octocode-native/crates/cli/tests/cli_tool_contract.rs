#![allow(clippy::expect_used, clippy::unwrap_used)]

//! A fresh CLI process parses only the called tool's slice of the embedded
//! contract. Each slice must load: a slice that does not match the embedded
//! text failed every call with a JSON parse error as the only detail.

mod support;
use octocode_native::tools::id::ToolId;
use serde_json::json;
use support::Workspace;

#[test]
fn every_tool_loads_its_contract_slice_in_a_fresh_process() {
    let workspace = Workspace::new();
    for tool in ToolId::ALL {
        let output = workspace
            .cli()
            .args([tool.as_str(), r#"{"queries":[{}]}"#])
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        for parse_error in [
            "trailing characters",
            "EOF while parsing",
            "expected value at",
        ] {
            assert!(!stdout.contains(parse_error), "{}: {stdout}", tool.as_str());
        }
    }
}

#[test]
fn a_brief_free_local_search_row_runs() {
    let workspace = Workspace::new();
    let file = workspace.write("src/a.txt", "needle\n");
    let root = file.parent().unwrap().to_string_lossy().into_owned();
    let output = workspace
        .cli()
        .args([
            "localSearch",
            &json!({"queries":[{"path":root,"searchText":"needle"}]}).to_string(),
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{stdout}");
    assert!(stdout.contains("needle"), "{stdout}");
}
