use super::*;
use octocode_test_support::json_fixture;
use serde_json::json;

fn context() -> ExecutionContext {
    ExecutionContext {
        cancellation: octocode_io::cancellation::Cancellation::new(),
        deadline: Some(Instant::now() + Duration::from_secs(5)),
        response_window: None,
        github_credential: None,
    }
}

fn insert_page(pending: &mut Pending, page: usize) -> String {
    pending.insert(
        ToolId::LocalSearch,
        json!({"queries":[{"path":"src","page":page}]}),
        SavedWork::ToolResume { index: Some(0) },
    )
}

#[test]
fn unusable_persistent_storage_names_the_recovery_without_publishing_a_handle() {
    let directory = tempfile::tempdir().unwrap();
    let blocked = directory.path().join("not-a-directory");
    std::fs::write(&blocked, "occupied").unwrap();
    let registry = Registry::scoped(CacheConfig::derived(), Some(blocked), String::from("policy").into());
    let mut pending = registry.pending([9; 32], None);
    let token = insert_page(&mut pending, 1);
    let error = registry
        .retain(pending, &context())
        .expect_err("storage failure");
    assert!(
        error
            .to_string()
            .contains("Persistent continuation storage"),
        "{error}"
    );
    assert!(error.to_string().contains("OCTOCODE_HOME"), "{error}");
    assert!(registry.open(ToolId::LocalSearch, &token, [9; 32]).is_err());
}

#[test]
fn page_planning_reserves_the_issued_handle_width() {
    let mut pending = registry("policy").pending([3; 32], None);
    let issued = insert_page(&mut pending, 1);
    assert_eq!(
        issued.len(),
        52,
        "two 128-bit IDs use compact opaque encoding"
    );
    let response = octocode_runtime_base::response::pager::prepare(
        octocode_runtime_base::response::pager::ResponseInput {
            tool: octocode_runtime_base::id::ToolId::LocalSearch,
            structured: json!({"results":[{"index":0,"data":{"files":(0..30)
                .map(|line|json!({"path":format!("file-{line}.rs"),"matches":[{"line":line+1,"value":"evidence"}]}))
                .collect::<Vec<_>>()}}]}),
            options: octocode_runtime_base::response::page_model::ResponsePageOptions {
                response_chars: Some(700),
                paging: true,
                ..Default::default()
            },
        },
        &octocode_io::cancellation::Cancellation::new(),
    )
    .expect("page");
    let planned = response.value["responsePagination"]["next"]["input"]["cursor"]
        .as_str()
        .expect("continued page");
    assert_eq!(
        planned.len(),
        issued.len(),
        "reserve actual wire width, not schema admission maximum"
    );
}

#[test]
fn a_busy_store_honors_cancellation_and_deadline_without_publishing() {
    let disk = tempfile::tempdir().unwrap();
    let registry = Registry::scoped(
        CacheConfig::derived(),
        Some(disk.path().into()),
        String::from("policy").into(),
    );
    let _held = octocode_io::lock::lock_within(
        octocode_io::lock::open_lock_file(&disk.path().join("continuations.lock")).unwrap(),
        Duration::ZERO,
        octocode_io::lock::LOCK_POLL,
    )
    .unwrap();
    let cancelled = context();
    cancelled.cancellation.cancel();
    let mut expiring = context();
    expiring.deadline = Some(Instant::now() + Duration::from_millis(20));
    for (context, expected) in [
        (cancelled, ExecutionError::Cancelled),
        (expiring, ExecutionError::Timeout),
    ] {
        let mut pending = registry.pending([9; 32], None);
        let token = insert_page(&mut pending, 1);
        assert_eq!(registry.retain(pending, &context), Err(expected));
        assert!(registry.open(ToolId::LocalSearch, &token, [9; 32]).is_err());
    }
}

#[test]
fn memory_publication_stops_while_another_writer_still_holds_the_lock() {
    let registry = registry("policy");
    let held = registry.mutation_lock(&context()).unwrap();
    let mut pending = registry.pending([4; 32], None);
    let token = insert_page(&mut pending, 1);
    let mut context = context();
    context.deadline = Some(Instant::now() + Duration::from_millis(20));
    let (sent, received) = std::sync::mpsc::channel();
    std::thread::scope(|scope| {
        scope.spawn(|| sent.send(registry.retain(pending, &context)).unwrap());
        let outcome = received.recv_timeout(Duration::from_secs(1));
        drop(held);
        assert_eq!(outcome.unwrap(), Err(ExecutionError::Timeout));
    });
    assert!(registry.open(ToolId::LocalSearch, &token, [4; 32]).is_err());
}

#[test]
fn branched_resumes_preserve_every_published_handle() {
    let registry = registry("policy");
    let mut initial = registry.pending([1; 32], None);
    let original = insert_page(&mut initial, 1);
    assert!(registry.retain(initial, &context()).is_ok());
    assert!(
        registry
            .open(ToolId::LocalSearch, &original, [1; 32])
            .is_ok()
    );
    let mut first = registry.pending([1; 32], None);
    let mut second = registry.pending([1; 32], None);
    let left = insert_page(&mut first, 2);
    let right = insert_page(&mut second, 3);
    assert!(registry.retain(first, &context()).is_ok());
    assert!(registry.retain(second, &context()).is_ok());
    for token in [original, left, right] {
        assert!(
            registry.open(ToolId::LocalSearch, &token, [1; 32]).is_ok(),
            "a published branch disappeared"
        );
    }
}

#[test]
fn independent_runtimes_merge_branches_and_refresh_cached_sets() {
    let disk = tempfile::tempdir().unwrap();
    let make = || {
        Registry::scoped(
            CacheConfig {
                disk_bytes: 1024 * 1024,
            },
            Some(disk.path().into()),
            String::from("policy").into(),
        )
    };
    let first = make();
    let second = make();
    let mut initial = first.pending([2; 32], None);
    let original = insert_page(&mut initial, 1);
    assert!(first.retain(initial, &context()).is_ok());
    let mut left = first
        .open(ToolId::LocalSearch, &original, [2; 32])
        .map(|_| first.pending([2; 32], None))
        .unwrap();
    let mut right = second
        .open(ToolId::LocalSearch, &original, [2; 32])
        .map(|_| second.pending([2; 32], None))
        .unwrap();
    let left_token = insert_page(&mut left, 2);
    let right_token = insert_page(&mut right, 3);
    assert!(first.retain(left, &context()).is_ok());
    assert!(second.retain(right, &context()).is_ok());
    for runtime in [&first, &second, &make()] {
        for token in [&original, &left_token, &right_token] {
            assert!(
                runtime.open(ToolId::LocalSearch, token, [2; 32]).is_ok(),
                "another runtime's published branch disappeared"
            );
        }
    }
}

fn registry(policy: &str) -> Registry {
    Registry::scoped(
        CacheConfig {
            disk_bytes: 1024 * 1024,
        },
        None,
        policy.to_owned().into(),
    )
}

#[test]
fn issued_work_retains_the_complete_request_and_rejects_other_access_or_tool() {
    let registry = registry("contract-and-policy");
    let input = json_fixture(
        r#"{
            "queries": [{"path": "src", "matchString": ["exact"], "page": 2}],
            "responseChars": 1024
        }"#,
    );
    let mut pending = registry.pending([7; 32], Some("github-user".into()));
    let token = pending.insert(
        ToolId::LocalSearch,
        input.clone(),
        SavedWork::ToolResume { index: Some(0) },
    );
    let refusal = |tool, token: &str, access| registry.open(tool, token, access).err();
    assert_eq!(
        refusal(ToolId::LocalSearch, &token, [7; 32]),
        Some(Refusal::NotRetained)
    );
    assert!(registry.retain(pending, &context()).is_ok());
    let opened = registry
        .open(ToolId::LocalSearch, &token, [7; 32])
        .expect("issued handle");
    assert_eq!(opened.member().input, input);
    assert_eq!(opened.github_identity(), Some("github-user"));
    assert_eq!(
        refusal(ToolId::LocalFetch, &token, [7; 32]),
        Some(Refusal::OtherTool)
    );
    assert_eq!(
        refusal(ToolId::LocalSearch, &token, [8; 32]),
        Some(Refusal::OtherAccess)
    );
    assert_eq!(
        refusal(ToolId::LocalSearch, &format!("{token}0"), [7; 32]),
        Some(Refusal::Malformed)
    );
    assert_eq!(
        refusal(ToolId::LocalSearch, "c1.eyJzdGF0ZSI6e319", [7; 32]),
        Some(Refusal::Malformed)
    );
}

/// One output's handles are each retained under the runtime's cache
/// configuration (no entry count: bytes alone bound retention), so issuing
/// many never evicts one of its own.
#[test]
fn an_output_retains_many_handles_without_evicting_its_own() {
    let registry = Registry::scoped(
        octocode_runtime_base::cache::CacheConfig::derived(),
        None,
        String::from("policy").into(),
    );
    let mut pending = registry.pending([0; 32], None);
    let tokens = (0..30)
        .map(|page| {
            pending.insert(
                ToolId::LocalSearch,
                json!({"queries":[{"path":"src","page":page}]}),
                SavedWork::ToolResume { index: Some(0) },
            )
        })
        .collect::<Vec<_>>();
    assert!(registry.retain(pending, &context()).is_ok());
    for token in tokens {
        assert!(registry.open(ToolId::LocalSearch, &token, [0; 32]).is_ok());
    }
}
#[test]
fn disk_reopen_preserves_work_but_different_contract_or_policy_cannot_open_it() {
    let disk = tempfile::tempdir().expect("disk");
    let config = CacheConfig {
        disk_bytes: 1024 * 1024,
    };
    let first = Registry::scoped(
        config.clone(),
        Some(disk.path().into()),
        String::from("contract-a;policy-a").into(),
    );
    let input = json_fixture(
        r#"{
            "queries": [
                {
                    "path": "a.ts",
                    "ranges": [{"startLine": 2, "endLine": 7}],
                    "readProgress": {"unit": "lines", "offset": 4},
                    "snapshot": "source-pin"
                }
            ],
            "responseChars": 3000
        }"#,
    );
    let mut pending = first.pending([3; 32], Some("account-a".into()));
    let token = pending.insert(
        ToolId::LocalFetch,
        input.clone(),
        SavedWork::ToolResume { index: Some(0) },
    );
    assert!(first.retain(pending, &context()).is_ok());
    first.clear_memory();
    let second = Registry::scoped(
        config.clone(),
        Some(disk.path().into()),
        String::from("contract-a;policy-a").into(),
    );
    let opened = second
        .open(ToolId::LocalFetch, &token, [3; 32])
        .expect("disk replay");
    assert_eq!(opened.member().input, input);
    assert_eq!(opened.github_identity(), Some("account-a"));
    let changed = Registry::scoped(
        config,
        Some(disk.path().into()),
        String::from("contract-b;policy-a").into(),
    );
    assert!(changed.open(ToolId::LocalFetch, &token, [3; 32]).is_err());
    let mut altered = token.clone().into_bytes();
    let at = altered.len() - 1;
    altered[at] = if altered[at] == b'0' { b'1' } else { b'0' };
    assert!(
        second
            .open(
                ToolId::LocalFetch,
                std::str::from_utf8(&altered).expect("ascii"),
                [3; 32]
            )
            .is_err()
    );
}

#[test]
fn unavailable_storage_never_claims_an_issued_handle() {
    let storage = tempfile::tempdir().unwrap();
    let registry = Registry::scoped(
        CacheConfig { disk_bytes: 0 },
        Some(storage.path().into()),
        String::from("policy").into(),
    );
    let mut pending = registry.pending([0; 32], None);
    let token = pending.insert(
        ToolId::LocalSearch,
        json_fixture(r#"{"queries": [{"path": "src", "matchString": ["run"]}]}"#),
        SavedWork::ToolResume { index: Some(0) },
    );
    assert!(!registry.retain(pending, &context()).is_ok());
    assert!(registry.open(ToolId::LocalSearch, &token, [0; 32]).is_err());
}

#[test]
fn extending_saved_response_pages_retains_earlier_source_work() {
    let registry = registry("policy");
    let mut pending = registry.pending([1; 32], None);
    let first = pending.insert(
        ToolId::LocalSearch,
        json_fixture(
            r#"{
                "queries": [{"path": "src", "matchString": ["run"], "page": 2}]
            }"#,
        ),
        SavedWork::ToolResume { index: Some(0) },
    );
    assert!(registry.retain(pending, &context()).is_ok());
    registry
        .open(ToolId::LocalSearch, &first, [1; 32])
        .expect("first");
    let mut pending = registry.pending([1; 32], None);
    let next = pending.insert(
        ToolId::LocalFetch,
        json_fixture(r#"{"queries": [{"path": "src/a.ts"}]}"#),
        SavedWork::ToolResume { index: Some(0) },
    );
    assert!(registry.retain(pending, &context()).is_ok());
    assert!(registry.open(ToolId::LocalSearch, &first, [1; 32]).is_ok());
    assert!(registry.open(ToolId::LocalFetch, &next, [1; 32]).is_ok());
}

#[cfg(unix)]
mod evidence_bodies;
mod handles;
mod turns;

/// A write the store's filesystem refuses (a full or read-only disk) is a
/// storage failure the caller can act on, never "the tool worker failed",
/// and no handle is published.
#[cfg(unix)]
#[test]
fn a_cursor_write_the_filesystem_refuses_is_a_storage_failure() {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let registry = Registry::scoped(
        CacheConfig::derived(),
        Some(directory.path().into()),
        String::from("policy").into(),
    );
    // One write makes the store's files, so the refusal below is the write's own.
    let mut first = registry.pending([9; 32], None);
    insert_page(&mut first, 1);
    registry.retain(first, &context()).expect("writable store");
    std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o555)).unwrap();
    let writable = std::fs::File::create(directory.path().join(".probe")).is_ok();
    let mut pending = registry.pending([9; 32], None);
    let token = insert_page(&mut pending, 2);
    let outcome = registry.retain(pending, &context());
    std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    if writable {
        return; // a superuser writes anywhere
    }
    let error = outcome.expect_err("the write was refused");
    assert_eq!(
        error,
        ExecutionError::StorageUnavailable(
            octocode_runtime_base::execution::StorageFault::NotWritable
        )
    );
    let served = crate::runtime::engine::runtime_execution_error(error);
    assert_eq!(
        served.code,
        octocode_contract_types::ErrorCode::ExecutionFailed
    );
    assert!(
        served.message.contains("Persistent continuation storage")
            && !served.message.contains("tool worker failed"),
        "{}",
        served.message
    );
    assert!(
        served.message.contains("not writable"),
        "the cause is named: {}",
        served.message
    );
    // The served call tells the caller to fix the environment, not to report
    // a tool failure.
    let rejected = crate::runtime::error::tool_error_result("localSearch", &served);
    let hints = rejected.envelope["hints"].to_string();
    assert!(
        hints.contains("OCTOCODE_HOME") && !hints.contains("tool failure"),
        "{}",
        rejected.envelope
    );
    assert!(registry.open(ToolId::LocalSearch, &token, [9; 32]).is_err());
}

/// A full disk and a quota are one fault and a refused permission another, so
/// the message names the one the filesystem reported.
#[test]
fn a_storage_failure_keeps_the_kind_the_filesystem_gave() {
    use octocode_runtime_base::execution::StorageFault;
    use std::io::ErrorKind;
    for (kind, fault, said) in [
        (ErrorKind::StorageFull, StorageFault::Full, "full"),
        (ErrorKind::QuotaExceeded, StorageFault::Full, "full"),
        (
            ErrorKind::PermissionDenied,
            StorageFault::NotWritable,
            "not writable",
        ),
        (
            ErrorKind::ReadOnlyFilesystem,
            StorageFault::NotWritable,
            "not writable",
        ),
    ] {
        assert_eq!(StorageFault::from(kind), fault);
        let served = crate::runtime::engine::runtime_execution_error(
            ExecutionError::StorageUnavailable(fault),
        );
        assert!(
            served.message.contains(said),
            "{kind:?}: {}",
            served.message
        );
    }
}

/// A saved continuation names a file by its shown name: a file named after a
/// credential is never stored with the credential in it.
#[test]
fn a_saved_continuation_stores_no_raw_credential_from_a_path() {
    let name = format!(
        "deploy-ghp_{}.txt",
        "a1B2c3D4e5".repeat(4).get(..36).unwrap()
    );
    let registry = registry("policy");
    let mut pending = registry.pending([5; 32], None);
    let token = pending.insert(
        ToolId::LocalSearch,
        json!({"queries":[{"path":format!("src/{name}"),"paths":[name.clone()],"page":2}]}),
        SavedWork::ToolResume { index: Some(0) },
    );
    registry.retain(pending, &context()).unwrap();
    let opened = registry.open(ToolId::LocalSearch, &token, [5; 32]).unwrap();
    let stored = opened.member().input.to_string();
    assert!(!stored.contains("ghp_a1B2"), "{stored}");
    assert!(
        stored.contains("REDACTED") && stored.contains("deploy-"),
        "{stored}"
    );
}
