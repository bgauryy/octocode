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
        let mut pending = Pending::new(access, None);
        for work in [
            SavedWork::ToolResume { index: Some(0) },
            SavedWork::ResponsePage {
                cursor: Cursor {
                    key: [1; 32],
                    digest: [2; 32],
                    length: 600,
                    offset: 2,
                },
                index: None,
            },
        ] {
            let token = pending.insert(
                ToolId::LocalFetch,
                json!({"queries":[{"path":"must-not-be-read"}]}),
                work,
            );
            writer.retain(pending, &context).unwrap();
            writer.clear_memory();
            make(policy.clone())
                .open(ToolId::LocalFetch, &token, access)
                .expect("same version reopens from disk");
            pending = Pending::new(access, None);
            if policy == current {
                assert!(
                    runtime
                        .cursors
                        .open(ToolId::LocalFetch, &token, access)
                        .is_ok()
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
#[test]
fn expired_response_guidance_uses_the_retained_page_lifetime() {
    let lifetime = format!("{} seconds", super::RETENTION_IDLE.as_secs());
    for persistent in [true, false] {
        for mcp in [true, false] {
            let scope = RefusalScope { persistent, mcp };
            let expired = super::snapshot_expired(Refusal::NotRetained, scope);
            assert!(
                expired.message.contains(&lifetime),
                "Recovery guidance must agree with saved-page retention: {}",
                expired.message
            );
            for refusal in [
                Refusal::Malformed,
                Refusal::NotRetained,
                Refusal::OtherTool,
                Refusal::OtherAccess,
                Refusal::OtherSurface,
                Refusal::IdentityChanged,
                Refusal::RequestChanged,
            ] {
                let error = super::snapshot_expired(refusal, scope);
                assert_eq!(error.code, ErrorCode::ResponseSnapshotExpired);
                assert!(error.message.contains("No source or provider was called"));
                assert!(error.message.contains("original request without cursor"));
            }
        }
    }
}

/// Retention is bounded by bytes, not by a count: a burst of small saved
/// responses or handles (one per call that pages or continues) never evicts
/// the walk an agent is still reading.
#[test]
fn a_burst_of_small_saved_work_never_evicts_an_active_walk() {
    let store = crate::cache::Store::<usize>::new(PageReplayMemo::cache_config(), None);
    let key = |index: usize| crate::cache::CacheKey {
        namespace: "burst".into(),
        scope: "policy".into(),
        resource: index.to_string(),
    };
    store.put(key(0), 0, 1024, RETENTION);
    for index in 1..=1024 {
        store.put(key(index), index, 1024, RETENTION);
    }
    assert!(
        store.get(&key(0)).is_some(),
        "the active walk survives the burst"
    );
}
