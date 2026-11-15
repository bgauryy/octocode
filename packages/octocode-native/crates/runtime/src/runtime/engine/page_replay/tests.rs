use super::*;
use crate::cache::CONTINUATION_RETENTION;
use crate::execution::ExecutionContext;
use crate::id::ToolId;
use crate::runtime::engine::cursors::{Registry, SavedWork};
use octocode_io::cancellation::Cancellation;
use std::time::{Duration, Instant};

#[tokio::test]
async fn changed_plan_expires_disk_handles_before_execution_but_current_handles_reopen() {
    let home = tempfile::tempdir().unwrap();
    let runtime = super::super::construction_tests::runtime(
        home.path(),
        &[("OCTOCODE_STORAGE_MODE", "persistent")],
    );
    let access = page_replay_key("", &[], &[], None);
    let context = ExecutionContext {
        cancellation: Cancellation::new(),
        deadline: Some(Instant::now() + Duration::from_secs(30)),
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
    let previous = replay_policy(&runtime.config, &runtime.input.cwd, "another-build");
    let current = replay_policy(&runtime.config, &runtime.input.cwd, BUILD_IDENTITY);
    assert_eq!(current, runtime.page_replays.policy);
    let make = |policy| {
        Registry::new(
            crate::cache::CacheConfig::derived(),
            Some(runtime.config.home.join("tmp").join("continuations")),
            policy,
        )
    };
    let mut expired_tokens = Vec::new();
    for (index, policy) in [legacy, previous, current.clone()].into_iter().enumerate() {
        let writer = make(policy.clone());
        let mut pending = writer.pending(access, None);
        for work in [
            SavedWork::ToolResume { index: Some(0) },
            SavedWork::Rows {
                indices: vec![0, 1],
            },
            SavedWork::ResponsePage {
                shape: super::super::cursors::PageShape::default(),
                rows: None,
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
            pending = writer.pending(access, None);
            if policy == current {
                assert!(
                    runtime
                        .cursors
                        .open(ToolId::LocalFetch, &token, access)
                        .is_ok()
                );
            } else {
                expired_tokens.push(token.clone());
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
    // Current issuance must not make any previously refused token usable.
    for token in expired_tokens {
        let result = runtime
            .execute(
                runtime.admit(format!("reissued-plan-{token}")).unwrap(),
                "localFetch".into(),
                json!({"cursor": token}),
                super::super::Channel::Mcp,
            )
            .await;
        let Err(error) = result else {
            panic!("current issuance resurrected an old plan's handle");
        };
        assert_eq!(error.code, ErrorCode::ResponseSnapshotExpired);
        assert!(runtime.github_services.get().is_none());
    }
    runtime.close().await;
}
#[test]
fn expired_response_guidance_uses_the_retained_page_lifetime() {
    let lifetime = format!("{} seconds", super::CONTINUATION_IDLE.as_secs());
    for persistent in [true, false] {
        {
            let scope = RefusalScope {
                persistent,
                tool: ToolId::LocalFetch,
            };
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
                Refusal::IdentityChanged,
                Refusal::RequestChanged,
            ] {
                let error = super::snapshot_expired(refusal, scope);
                let expected = match refusal {
                    Refusal::Malformed | Refusal::OtherTool => ErrorCode::InvalidInput,
                    _ => ErrorCode::ResponseSnapshotExpired,
                };
                assert_eq!(error.code, expected, "{refusal:?}");
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
    let store = crate::cache::Store::<usize>::new(crate::cache::CacheConfig::derived(), None);
    let key = |index: usize| crate::cache::CacheKey {
        namespace: "burst".into(),
        scope: "policy".into(),
        resource: index.to_string(),
    };
    store.put_confirmed(key(0), 0, 1024, CONTINUATION_RETENTION);
    for index in 1..=1024 {
        store.put_confirmed(key(index), index, 1024, CONTINUATION_RETENTION);
    }
    assert!(
        store.get(&key(0)).is_some(),
        "the active walk survives the burst"
    );
}

/// A continuation that expires before the plan it continues is a dead end:
/// every tool's cursor, saved pages and stated expiry share one lifetime, and
/// a clasify cursor is never shorter than its plan.
#[test]
fn a_clasify_continuation_lives_as_long_as_its_plan() {
    use crate::tools::clasify::sections::PLAN_RETENTION;
    assert_eq!(retention_of(ToolId::Clasify.as_str()), PLAN_RETENTION);
    for tool in [ToolId::LocalFetch, ToolId::GhSearchCode] {
        assert_eq!(retention_of(tool.as_str()), CONTINUATION_RETENTION);
    }
    assert!(
        retention_of(ToolId::Clasify.as_str()).ttl()
            > retention_of(ToolId::LocalFetch.as_str()).ttl()
    );
    assert_ne!(
        page_expiry(ToolId::Clasify.as_str()),
        page_expiry(ToolId::LocalFetch.as_str())
    );
}

/// Every page states `now + lifetime` as its expiry, so saved work must
/// count its lifetime from its last read: a clasify page saved 31 minutes
/// ago and read a minute ago stated an expiry 29 minutes from now, and is
/// still served (CLS-C29).
#[test]
fn a_saved_clasify_page_lives_its_stated_expiry_from_its_last_read() {
    let dir = tempfile::tempdir().unwrap();
    let store = crate::cache::Store::<u32>::new(
        crate::cache::CacheConfig::derived(),
        Some(dir.path().to_path_buf()),
    );
    let key = crate::cache::CacheKey {
        namespace: "response-pages-v2".into(),
        scope: "policy".into(),
        resource: "page".into(),
    };
    let lifetime = retention_of(ToolId::Clasify.as_str());
    let ttl = lifetime.ttl().expect("a clasify page expires");
    assert!(store.put_confirmed(key.clone(), 7, 8, lifetime).1);
    let file = crate::cache::Disk::new(dir.path().to_path_buf()).file(&key);
    let bytes = std::fs::read(&file).unwrap();
    let split = bytes.iter().position(|byte| *byte == b'\n').unwrap();
    let mut header: serde_json::Value = serde_json::from_slice(&bytes[..split]).unwrap();
    let saved =
        crate::civil_date::unix_secs().unsigned_abs() - (ttl + Duration::from_secs(60)).as_secs();
    header["storedAtUnix"] = serde_json::json!(saved);
    let mut rewritten = serde_json::to_vec(&header).unwrap();
    rewritten.extend_from_slice(&bytes[split..]);
    std::fs::write(&file, rewritten).unwrap();
    let last_read = std::time::SystemTime::now() - Duration::from_secs(60);
    std::fs::File::options()
        .write(true)
        .open(&file)
        .unwrap()
        .set_modified(last_read)
        .unwrap();
    assert_eq!(
        store.get(&key).map(|hit| *hit.value),
        Some(7),
        "a page read a minute ago lives until the expiry that read stated"
    );
}

/// A refused cursor states the lifetime of the tool it was sent to: a
/// clasify cursor lives as long as its plan, any other its idle window.
#[test]
fn a_refusal_states_the_lifetime_of_its_tool() {
    for persistent in [true, false] {
        for tool in [ToolId::Clasify, ToolId::LocalFetch] {
            let seconds = retention_of(tool.as_str()).ttl().unwrap().as_secs();
            let error = snapshot_expired(Refusal::NotRetained, RefusalScope { persistent, tool });
            assert!(
                error
                    .message
                    .contains(&format!("{seconds} seconds after its last read")),
                "{tool:?}: {}",
                error.message
            );
        }
    }
}
