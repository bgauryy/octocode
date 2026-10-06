// Integration test crate — assertions use unwrap/expect/panic freely.
#![allow(clippy::expect_used, clippy::panic)]

//! Catalog-shape switches (S13 arms): `tools.enabled`/`tools.disabled` narrow
//! availability, and the `mcp.*` presentation switches reach hosts through the
//! runtime catalog. Defaults keep the full catalog.

use crate::support;

use serde_json::{Value, json};
use support::Workspace;

fn tool<'a>(catalog: &'a Value, name: &str) -> &'a Value {
    catalog["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .find(|tool| tool["name"] == name)
        .unwrap_or_else(|| panic!("{name} entry"))
}

#[tokio::test]
async fn default_family_keeps_every_family() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    for name in ["localSearch", "ghSearchCode", "artifactSearch"] {
        assert!(runtime.is_available(name), "{name}");
    }
    let catalog = runtime.catalog().expect("catalog");
    assert_eq!(catalog["presentation"], json!({"deferred":[]}));
    runtime.close().await;
}

#[tokio::test]
async fn tool_lists_select_tools_and_name_the_reason() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("TOOLS_TO_RUN", "localSearch,ghSearchCode".into())]);
    assert!(runtime.is_available("localSearch"));
    assert!(runtime.is_available("ghSearchCode"));
    assert!(!runtime.is_available("localFetch"));
    // The retired family preset no longer narrows anything.
    let retired = workspace.runtime(&[("OCTOCODE_TOOL_FAMILY", "local".into())]);
    assert!(retired.is_available("ghSearchCode"));
    let catalog = runtime.catalog().expect("catalog");
    assert_eq!(
        tool(&catalog, "localFetch")["unavailableReason"],
        "toolsList"
    );
    runtime.close().await;
    retired.close().await;
}

#[tokio::test]
async fn presentation_switches_reach_the_catalog() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[
        // Unknown names and unavailable tools (localSearch is disabled) are
        // never deferred: the dispatcher serves only tools the runtime runs.
        (
            "OCTOCODE_DEFER_TOOLS",
            "nope,ghSearchRepo,localSearch,ghStructure".into(),
        ),
        ("DISABLE_TOOLS", "localSearch".into()),
    ]);
    let catalog = runtime.catalog().expect("catalog");
    assert_eq!(
        catalog["presentation"],
        json!({"deferred":["ghSearchRepo","ghStructure"]})
    );
    assert!(
        runtime.is_available("ghSearchRepo"),
        "deferred tools stay executable"
    );
    runtime.close().await;
}
