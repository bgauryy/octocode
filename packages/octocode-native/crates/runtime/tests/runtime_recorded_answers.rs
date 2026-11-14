//! GitHub tools on real recorded answers (`test-utils/provider-answers`), through
//! the public call: every output is checked against the published contract, and
//! what must come through is read from the recording, not copied here.

use crate::support::{self, Workspace, recorded};
use serde_json::{Value, json};
use wiremock::MockServer;

async fn answered(case: &str) -> (Value, Value) {
    let server = MockServer::builder().start().await;
    let recording = recorded::mount_github(&server, case).await;
    let workspace = Workspace::new();
    let mut env = support::github_api(&server).to_vec();
    env.push(("GITHUB_TOKEN", "recorded-answers-fake-token".to_owned()));
    let runtime = workspace.runtime_with(&env, support::PROVIDER_LIMITS);
    let row = support::run(
        &runtime,
        recording["tool"].as_str().expect("tool"),
        json!({"queries": [recording["query"]], "responseChars": 1_000_000}),
    )
    .await;
    let data = row.data().clone();
    runtime.close().await;
    (recording, data)
}

fn answer<'a>(recording: &'a Value, url_end: &str) -> &'a Value {
    recording["exchanges"]
        .as_array()
        .expect("exchanges")
        .iter()
        .find(|exchange| {
            exchange["request"]["url"]
                .as_str()
                .is_some_and(|url| url.ends_with(url_end))
        })
        .map(|exchange| &exchange["body"])
        .expect("a recorded answer")
}

#[tokio::test]
async fn a_repository_row_states_what_github_answered() {
    let (recording, data) = answered("github/repo-get").await;
    let github = answer(&recording, "/repos/octocat/Hello-World");
    let row = &data["repositories"][0];
    assert_eq!(row["owner"], github["owner"]["login"], "{data}");
    assert_eq!(row["repo"], github["name"], "{data}");
    assert_eq!(row["defaultBranch"], github["default_branch"], "{data}");
}

#[tokio::test]
async fn a_tree_row_lists_every_path_github_answered() {
    let (recording, data) = answered("github/structure-tree").await;
    let tree = recording["exchanges"]
        .as_array()
        .expect("exchanges")
        .iter()
        .find_map(|exchange| exchange["body"]["tree"].as_array())
        .expect("a recorded tree");
    assert!(!tree.is_empty());
    let shown = data.to_string();
    for entry in tree {
        let name = entry["path"].as_str().expect("a path");
        assert!(shown.contains(name), "{name} missing from {shown}");
    }
}

/// Every recorded GitHub answer, through the public call: a row that answered
/// whole offers its tool's one workflow lead and only the calls that carry a
/// fact it does not state ([`support::assert_workflow_leads`], which
/// `support::run` applies).
#[tokio::test]
async fn every_recorded_github_answer_offers_at_most_its_workflow_lead() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../test-utils/provider-answers/github");
    let mut cases = std::fs::read_dir(&dir)
        .expect("recordings")
        .filter_map(|entry| {
            let name = entry.ok()?.file_name().into_string().ok()?;
            Some(format!("github/{}", name.strip_suffix(".json")?))
        })
        .collect::<Vec<_>>();
    cases.sort();
    assert!(!cases.is_empty());
    for case in cases {
        answered(&case).await;
    }
}
