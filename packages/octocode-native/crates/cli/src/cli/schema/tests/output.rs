use super::*;

/// The output view is the meaning of each output field and coverage reason,
/// the same one the MCP resource serves.
#[test]
fn the_output_view_is_the_tools_output_reference() {
    let m = machine(&[]);
    let view = tool_view(&m, "localSearch", Some("output"), None).expect("output view");
    assert_eq!(view["name"], "localSearch");
    assert!(!view["fields"].is_null());
    assert_eq!(crate::cli::schema::output_of("localSearch"), Some(view));
    assert_eq!(crate::cli::schema::output_of("noSuchTool"), None);
}

#[test]
fn output_views_expose_the_embedded_collection_routes_for_lossless_reassembly() {
    let m = machine(&[]);
    let contract = octocode_runtime_base::contracts::parsed_contract().expect("embedded contract");
    for tool in contract["tools"].as_array().expect("tools") {
        let name = tool["name"].as_str().expect("tool name");
        let view = tool_view(&m, name, Some("output"), None).expect("output view");
        for field in ["collectionPaths", "collectionRefs"] {
            assert_eq!(view.get(field), tool.get(field), "{name}: {field}");
        }
    }
}

/// A tool's output view lists the coverage reasons that tool can emit, not
/// the one dictionary of every tool: a GitHub search reads no language
/// server reason, a language server read no provider cap.
#[test]
fn each_output_view_lists_only_the_reasons_its_tool_emits() {
    let m = machine(&[]);
    let reasons = |name: &str| {
        let view = tool_view(&m, name, Some("output"), None).expect("output view");
        view["partialReasons"]
            .as_object()
            .unwrap_or_else(|| panic!("{name} lists reasons: {view}"))
            .clone()
    };
    let (repo, lsp, search) = (
        reasons("ghSearchCode"),
        reasons("lspSearch"),
        reasons("localSearch"),
    );
    assert!(repo.contains_key("providerResultCap"));
    assert!(!repo.contains_key("serverUnhealthy"));
    assert!(lsp.contains_key("serverUnhealthy") && !lsp.contains_key("providerResultCap"));
    assert!(search.contains_key("binaryQuit") && !search.contains_key("walkWindow"));
    assert_ne!(repo, lsp);
}
