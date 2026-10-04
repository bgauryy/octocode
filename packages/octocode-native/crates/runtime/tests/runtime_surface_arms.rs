// Integration test crate — assertions use unwrap/expect/panic freely.
#![allow(clippy::expect_used, clippy::panic)]

//! Catalog-shape switches (S13 arms): `tools.family` narrows availability by
//! core policy family, and the `mcp.*` presentation switches reach hosts
//! through the runtime catalog. Defaults keep the full catalog.

mod support;

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
    assert_eq!(
        catalog["presentation"],
        json!({"publishedView":"queries","instructions":"default","deferred":[]})
    );
    runtime.close().await;
}

#[tokio::test]
async fn github_family_keeps_github_and_remote_tools() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("OCTOCODE_TOOL_FAMILY", "github".into())]);
    assert!(runtime.is_available("ghSearchCode"));
    assert!(runtime.is_available("artifactSearch"));
    assert!(!runtime.is_available("localSearch"));
    assert!(!runtime.is_available("lspSearch"));
    let catalog = runtime.catalog().expect("catalog");
    assert_eq!(tool(&catalog, "localFetch")["unavailableReason"], "family");
    runtime.close().await;
}

#[tokio::test]
async fn local_family_keeps_local_and_remote_tools() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("OCTOCODE_TOOL_FAMILY", "local".into())]);
    assert!(runtime.is_available("localSearch"));
    assert!(runtime.is_available("artifactSearch"));
    assert!(!runtime.is_available("ghSearchCode"));
    assert!(!runtime.is_available("ghGetHistoryItem"));
    let catalog = runtime.catalog().expect("catalog");
    assert_eq!(tool(&catalog, "ghStructure")["unavailableReason"], "family");
    runtime.close().await;
}

#[tokio::test]
async fn family_intersects_the_tool_lists_and_never_widens_them() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[
        ("OCTOCODE_TOOL_FAMILY", "local".into()),
        ("TOOLS_TO_RUN", "localSearch,ghSearchCode".into()),
    ]);
    assert!(runtime.is_available("localSearch"));
    assert!(
        !runtime.is_available("ghSearchCode"),
        "family narrows TOOLS_TO_RUN"
    );
    assert!(
        !runtime.is_available("localFetch"),
        "family never widens TOOLS_TO_RUN"
    );
    assert!(!runtime.is_available("artifactSearch"));
    let catalog = runtime.catalog().expect("catalog");
    assert_eq!(
        tool(&catalog, "localFetch")["unavailableReason"],
        "toolsList"
    );
    runtime.close().await;
}

#[tokio::test]
async fn presentation_switches_reach_the_catalog() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[
        ("OCTOCODE_PUBLISHED_VIEW", "flat".into()),
        ("OCTOCODE_INSTRUCTIONS", "guide".into()),
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
        json!({"publishedView":"flat","instructions":"guide","deferred":["ghSearchRepo","ghStructure"]})
    );
    assert!(
        runtime.is_available("ghSearchRepo"),
        "deferred tools stay executable"
    );
    runtime.close().await;
}
