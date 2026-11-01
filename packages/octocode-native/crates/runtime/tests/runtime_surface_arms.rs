use crate::support::Workspace;
use serde_json::Value;

fn tool<'a>(catalog: &'a Value, name: &str) -> &'a Value {
    catalog["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .find(|tool| tool["name"] == name)
        .unwrap_or_else(|| panic!("{name} entry"))
}

#[tokio::test]
async fn catalog_availability_matches_execution_gates() {
    let workspace = Workspace::new();
    let default = workspace.runtime(&[]);
    let mut restricted_config = workspace.config(&[
        ("TOOLS_TO_RUN", "localSearch,ghSearchCode".into()),
        ("DISABLE_TOOLS", "ghSearchCode".into()),
    ]);
    restricted_config.runtime_surface = octocode_native::config::RuntimeSurface::Mcp;
    let restricted =
        octocode_native::runtime::ToolRuntime::new(restricted_config).expect("MCP runtime");
    for name in [
        "localSearch",
        "localFetch",
        "ghSearchCode",
        "artifactSearch",
    ] {
        assert!(default.is_available(name), "{name}");
        let available = restricted.is_available(name);
        assert_eq!(available, name == "localSearch", "{name}");
        let catalog = restricted.catalog();
        let entry = tool(&catalog, name);
        assert_eq!(entry["available"], available, "{entry}");
        if !available {
            assert!(entry["unavailableReason"].is_string(), "{entry}");
        }
    }
    default.close().await;
    restricted.close().await;
}
