use super::*;
use crate::execution::ExecutionContext;
use crate::id::ToolId;
use crate::runtime::engine::cursors::{Pending, Registry, SavedWork};
use std::time::Instant;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn changed_plan_expires_disk_handles_before_execution_but_current_handles_reopen() {
    let home = tempfile::tempdir().unwrap();
    let runtime = super::super::construction_tests::runtime(
        home.path(),
        &[("OCTOCODE_STORAGE_MODE", "persistent")],
    );
    let access = page_replay_key("", true, &[], &[], None);
    let context = ExecutionContext {
        cancellation: CancellationToken::new(),
        deadline: Instant::now() + Duration::from_secs(30),
        walk_threads: None,
        response_window: None,
        github_credential: None,
    };
    let legacy = octocode_engine::digest::sha256(
        format!(
            "{}:{}:{}",
            runtime.config.identity_digest(),
            runtime.input.cwd.display(),
            octocode_contract_types::TOOL_TYPES_CONTRACT_FINGERPRINT
        )
        .as_bytes(),
    );
    let previous = replay_policy(&runtime.config, &runtime.input.cwd, ROW_PLAN_VERSION - 1);
    let current = replay_policy(&runtime.config, &runtime.input.cwd, ROW_PLAN_VERSION);
    assert_eq!(current, runtime.page_replays.policy);
    let make = |policy| {
        Registry::new(
            PageReplayMemo::cache_config(),
            Some(runtime.config.home.join("tmp").join("continuations")),
            policy,
        )
    };
    for (index, policy) in [legacy, previous, current.clone()].into_iter().enumerate() {
        let writer = make(policy.clone());
        let mut pending = Pending::new(access, None).unwrap();
        for work in [
            SavedWork::ToolResume { index: Some(0) },
            SavedWork::ResponsePage(Cursor {
                key: [1; 32],
                digest: [2; 32],
                length: 600,
                offset: 2,
            }),
        ] {
            let token = pending
                .insert(
                    ToolId::LocalFetch,
                    json!({"queries":[{"path":"must-not-be-read"}]}),
                    work,
                )
                .unwrap();
            writer.retain(pending, &context).unwrap();
            writer.clear_memory();
            let reopened = make(policy.clone())
                .open(ToolId::LocalFetch, &token, access)
                .expect("same version reopens from disk");
            pending = reopened.pending();
            if policy == current {
                assert!(
                    runtime
                        .cursors
                        .open(ToolId::LocalFetch, &token, access)
                        .is_some()
                );
            } else {
                let result = runtime
                    .execute(
                        runtime.admit(format!("old-plan-{index}-{token}")).unwrap(),
                        "localFetch".into(),
                        json!({"cursor": token}),
                        super::super::Channel::Mcp,
                    )
                    .await;
                let Err(error) = result else {
                    panic!("changed plan must fail before tool execution");
                };
                assert_eq!(error.code, ErrorCode::ResponseSnapshotExpired);
                assert!(runtime.github_services.get().is_none());
            }
        }
    }
    runtime.close().await;
}
