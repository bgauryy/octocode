use super::{WarmState, anchor_file, lead_file};
use octocode_test_support::json_fixture;
use serde_json::json;

#[test]
fn completed_prewarms_do_not_exhaust_future_workspaces() {
    let mut state = WarmState::default();
    for i in 0..3 {
        let key = format!("workspace-{i}");
        assert!(state.begin(&key));
        state.finish(&key, true);
    }
    assert!(state.begin("workspace-3"));
    assert!(!state.begin("workspace-3"));
}

#[test]
fn a_previously_warm_server_can_be_warmed_again_after_cooldown() {
    let mut state = WarmState::default();
    assert!(state.begin("workspace"));
    state.finish("workspace", true);
    assert!(!state.begin("workspace"));
    state.recent.front_mut().unwrap().1 =
        std::time::Instant::now() - super::PREWARM_COOLDOWN - std::time::Duration::from_secs(1);
    assert!(state.begin("workspace"));
}

#[test]
fn anchor_prefers_the_top_hit_then_a_file_query_path() {
    let dir_dir = octocode_runtime_base::test_support::Fixture::new();
    let dir = dir_dir.0.clone();
    std::fs::create_dir_all(&dir).expect("dir");
    let hit = dir.join("hit.ts");
    let read = dir.join("read.ts");
    std::fs::write(&hit, "export const a = 1;\n").expect("hit");
    std::fs::write(&read, "export const b = 1;\n").expect("read");
    let search = json!({"path": dir});
    let data = json!({"files": [{"path": hit}, {"path": read}]});
    assert_eq!(anchor_file(&search, &data), Some(hit.clone()));
    assert_eq!(anchor_file(&json!({"path": read}), &json!({})), Some(read));
    // A directory query with no hits, or relative paths, name no file.
    assert_eq!(
        anchor_file(&search, &json_fixture(r#"{"files": []}"#)),
        None
    );
    assert_eq!(
        anchor_file(
            &json_fixture(r#"{"path": "hit.ts"}"#),
            &json_fixture(r#"{"files": [{"path": "hit.ts"}]}"#)
        ),
        None
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// The settled response is the input: a served lead (absolute, a file
/// uri, or workspace-relative as the stage leaves it) resolves; a lead the
/// stage stripped, another tool's call, a row that did not answer and
/// provider-embedded data name no file.
#[test]
fn a_served_lsp_lead_points_at_its_anchor_file() {
    let fixture = octocode_runtime_base::test_support::Fixture::new();
    let dir = fixture.0.clone();
    std::fs::create_dir_all(&dir).expect("dir");
    let file = dir.join("a.ts");
    std::fs::write(&file, "export const a = 1;\n").expect("file");
    let paths = octocode_runtime_base::policy::path::PathPolicy::new(
        octocode_runtime_base::policy::path::PathPolicyConfig {
            workspace_root: Some(dir.clone()),
            ..octocode_runtime_base::policy::path::PathPolicyConfig::default()
        },
    )
    .expect("path policy");
    let tool = octocode_runtime_base::id::ToolId::LocalSearch;
    let served = |input: serde_json::Value, row: serde_json::Value| {
        let mut row = row;
        row["data"]["related"] = json!([{"tool": "lspSearch", "kind": "related", "input": input}]);
        json!({"results": [row]})
    };
    let query = |path: String| json!({"queries": [{"path": path, "symbolName": "a", "line": 1}]});
    let row = || json_fixture(r#"{"index": 0, "data": {"files": [{"path": "a.ts"}]}}"#);
    let uri = format!("file://{}", file.display());
    assert_eq!(
        lead_file(&paths, tool, &served(query(uri), row())),
        Some(file.clone())
    );
    assert_eq!(
        lead_file(
            &paths,
            tool,
            &served(query(file.display().to_string()), row())
        ),
        Some(file.clone())
    );
    assert_eq!(
        lead_file(&paths, tool, &served(query("a.ts".into()), row())),
        Some(file.clone()),
        "a workspace-relative served lead resolves"
    );
    let encoded = dir.join("a b.ts");
    std::fs::write(&encoded, "export const b = 1;\n").expect("spaced file");
    let uri = format!("file://{}/a%20b.ts", dir.display());
    assert_eq!(
        lead_file(&paths, tool, &served(query(uri), row())),
        Some(encoded)
    );
    // The stage stripped the lead: no cause, no server.
    assert_eq!(lead_file(&paths, tool, &json!({"results": [row()]})), None);
    let other = json_fixture(
        r#"{
                "results": [
                    {
                        "index": 0,
                        "data": {
                            "related": [
                                {
                                    "tool": "localFetch",
                                    "kind": "related",
                                    "input": {"queries": [{"path": "a.ts"}]}
                                }
                            ]
                        }
                    }
                ]
            }"#,
    );
    assert_eq!(lead_file(&paths, tool, &other), None);
    let empty = json_fixture(r#"{"index": 0, "status": "empty", "data": {}}"#);
    assert_eq!(
        lead_file(&paths, tool, &served(query("a.ts".into()), empty)),
        None
    );
    let provider = json!({"results": [{"index": 0, "data": {"files": [{"path": "a.ts",
            "provider": {"tool": "lspSearch", "kind": "related", "input": query("a.ts".into())}}]}}]});
    assert_eq!(lead_file(&paths, tool, &provider), None);
    let _ = std::fs::remove_dir_all(dir);
}
