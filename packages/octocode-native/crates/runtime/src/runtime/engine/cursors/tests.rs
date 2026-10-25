use super::*;
use serde_json::json;

fn context() -> ExecutionContext {
    ExecutionContext {
        cancellation: tokio_util::sync::CancellationToken::new(),
        deadline: Instant::now() + Duration::from_secs(5),
        walk_threads: None,
        response_window: None,
        github_credential: None,
    }
}

fn insert_page(pending: &mut Pending, page: usize) -> String {
    pending
        .insert(
            ToolId::LocalSearch,
            json!({"queries":[{"path":"src","page":page}]}),
            SavedWork::ToolResume { index: Some(0) },
        )
        .expect("randomness")
}

#[test]
fn a_busy_store_honors_cancellation_and_deadline_without_publishing() {
    let disk = tempfile::tempdir().unwrap();
    let registry = Registry::new(
        CacheConfig::default(),
        Some(disk.path().into()),
        "policy".into(),
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
    expiring.deadline = Instant::now() + Duration::from_millis(20);
    for (context, expected) in [
        (cancelled, ExecutionError::Cancelled),
        (expiring, ExecutionError::Timeout),
    ] {
        let mut pending = Pending::new([9; 32], None).unwrap();
        let token = insert_page(&mut pending, 1);
        assert_eq!(registry.retain(pending, &context), Err(expected));
        assert!(
            registry
                .open(ToolId::LocalSearch, &token, [9; 32])
                .is_none()
        );
    }
}

#[test]
fn memory_publication_stops_while_another_writer_still_holds_the_lock() {
    let registry = registry("policy");
    let held = registry.mutation.lock().unwrap();
    let mut pending = Pending::new([4; 32], None).unwrap();
    let token = insert_page(&mut pending, 1);
    let mut context = context();
    context.deadline = Instant::now() + Duration::from_millis(20);
    let (sent, received) = std::sync::mpsc::channel();
    std::thread::scope(|scope| {
        scope.spawn(|| sent.send(registry.retain(pending, &context)).unwrap());
        let outcome = received.recv_timeout(Duration::from_secs(1));
        drop(held);
        assert_eq!(outcome.unwrap(), Err(ExecutionError::Timeout));
    });
    assert!(
        registry
            .open(ToolId::LocalSearch, &token, [4; 32])
            .is_none()
    );
}

#[test]
fn branched_resumes_preserve_every_published_handle() {
    let registry = registry("policy");
    let mut initial = Pending::new([1; 32], None).expect("randomness");
    let original = insert_page(&mut initial, 1);
    assert!(registry.retain(initial, &context()).is_ok());
    let opened = registry
        .open(ToolId::LocalSearch, &original, [1; 32])
        .unwrap();
    let mut first = opened.pending();
    let mut second = opened.pending();
    let left = insert_page(&mut first, 2);
    let right = insert_page(&mut second, 3);
    assert!(registry.retain(first, &context()).is_ok());
    assert!(registry.retain(second, &context()).is_ok());
    for token in [original, left, right] {
        assert!(
            registry
                .open(ToolId::LocalSearch, &token, [1; 32])
                .is_some(),
            "a published branch disappeared"
        );
    }
}

#[test]
fn independent_runtimes_merge_branches_and_refresh_cached_sets() {
    let disk = tempfile::tempdir().unwrap();
    let make = || {
        Registry::new(
            CacheConfig {
                max_entries: 8,
                max_bytes: 1024 * 1024,
                disk_bytes: 1024 * 1024,
            },
            Some(disk.path().into()),
            "policy".into(),
        )
    };
    let first = make();
    let second = make();
    let mut initial = Pending::new([2; 32], None).unwrap();
    let original = insert_page(&mut initial, 1);
    assert!(first.retain(initial, &context()).is_ok());
    let mut left = first
        .open(ToolId::LocalSearch, &original, [2; 32])
        .unwrap()
        .pending();
    let mut right = second
        .open(ToolId::LocalSearch, &original, [2; 32])
        .unwrap()
        .pending();
    let left_token = insert_page(&mut left, 2);
    let right_token = insert_page(&mut right, 3);
    assert!(first.retain(left, &context()).is_ok());
    assert!(second.retain(right, &context()).is_ok());
    for runtime in [&first, &second, &make()] {
        for token in [&original, &left_token, &right_token] {
            assert!(
                runtime.open(ToolId::LocalSearch, token, [2; 32]).is_some(),
                "another runtime's published branch disappeared"
            );
        }
    }
}

fn registry(policy: &str) -> Registry {
    Registry::new(
        CacheConfig {
            max_entries: 8,
            max_bytes: 1024 * 1024,
            disk_bytes: 1024 * 1024,
        },
        None,
        policy.into(),
    )
}

#[test]
fn issued_work_retains_the_complete_request_and_rejects_other_access_or_tool() {
    let registry = registry("contract-and-policy");
    let input =
        json!({"queries":[{"path":"src","matchString":["exact"],"page":2}],"responseChars":1024});
    let mut pending = Pending::new([7; 32], Some("github-user".into())).expect("randomness");
    let token = pending
        .insert(
            ToolId::LocalSearch,
            input.clone(),
            SavedWork::ToolResume { index: Some(0) },
        )
        .expect("randomness");
    assert!(
        registry
            .open(ToolId::LocalSearch, &token, [7; 32])
            .is_none()
    );
    assert!(registry.retain(pending, &context()).is_ok());
    let opened = registry
        .open(ToolId::LocalSearch, &token, [7; 32])
        .expect("issued handle");
    assert_eq!(opened.member().input, input);
    assert_eq!(opened.github_identity(), Some("github-user"));
    assert!(registry.open(ToolId::LocalFetch, &token, [7; 32]).is_none());
    assert!(
        registry
            .open(ToolId::LocalSearch, &token, [8; 32])
            .is_none()
    );
    assert!(
        registry
            .open(ToolId::LocalSearch, &format!("{token}0"), [7; 32])
            .is_none()
    );
    assert!(
        registry
            .open(ToolId::LocalSearch, "c1.eyJzdGF0ZSI6e319", [7; 32])
            .is_none()
    );
}

#[test]
fn a_set_retains_many_members_without_evicting_its_own_handles() {
    let registry = registry("policy");
    let mut pending = Pending::new([0; 32], None).expect("randomness");
    let tokens = (0..30)
        .map(|page| {
            pending
                .insert(
                    ToolId::LocalSearch,
                    json!({"queries":[{"path":"src","page":page}]}),
                    SavedWork::ToolResume { index: Some(0) },
                )
                .expect("randomness")
        })
        .collect::<Vec<_>>();
    assert!(registry.retain(pending, &context()).is_ok());
    for token in tokens {
        assert!(
            registry
                .open(ToolId::LocalSearch, &token, [0; 32])
                .is_some()
        );
    }
}
#[test]
fn disk_reopen_preserves_work_but_different_contract_or_policy_cannot_open_it() {
    let disk = tempfile::tempdir().expect("disk");
    let config = CacheConfig {
        max_entries: 8,
        max_bytes: 1024 * 1024,
        disk_bytes: 1024 * 1024,
    };
    let first = Registry::new(
        config.clone(),
        Some(disk.path().into()),
        "contract-a;policy-a".into(),
    );
    let input = json!({"queries":[{"path":"a.ts","ranges":[{"startLine":2,"endLine":7}],"readProgress":{"unit":"lines","offset":4},"snapshot":"source-pin"}],"responseChars":3000});
    let mut pending = Pending::new([3; 32], Some("account-a".into())).expect("randomness");
    let token = pending
        .insert(
            ToolId::LocalFetch,
            input.clone(),
            SavedWork::ToolResume { index: Some(0) },
        )
        .expect("randomness");
    assert!(first.retain(pending, &context()).is_ok());
    first.clear_memory();
    let second = Registry::new(
        config.clone(),
        Some(disk.path().into()),
        "contract-a;policy-a".into(),
    );
    let opened = second
        .open(ToolId::LocalFetch, &token, [3; 32])
        .expect("disk replay");
    assert_eq!(opened.member().input, input);
    assert_eq!(opened.github_identity(), Some("account-a"));
    let changed = Registry::new(
        config,
        Some(disk.path().into()),
        "contract-b;policy-a".into(),
    );
    assert!(changed.open(ToolId::LocalFetch, &token, [3; 32]).is_none());
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
            .is_none()
    );
}

#[test]
fn unavailable_storage_never_claims_an_issued_handle() {
    let registry = Registry::new(
        CacheConfig {
            max_entries: 0,
            max_bytes: 32,
            disk_bytes: 0,
        },
        None,
        "policy".into(),
    );
    let mut pending = Pending::new([0; 32], None).expect("randomness");
    let token = pending
        .insert(
            ToolId::LocalSearch,
            json!({"queries":[{"path":"src","matchString":["run"]}]}),
            SavedWork::ToolResume { index: Some(0) },
        )
        .expect("randomness");
    assert!(!registry.retain(pending, &context()).is_ok());
    assert!(
        registry
            .open(ToolId::LocalSearch, &token, [0; 32])
            .is_none()
    );
}

#[test]
fn extending_saved_response_pages_retains_earlier_source_work() {
    let registry = registry("policy");
    let mut pending = Pending::new([1; 32], None).expect("randomness");
    let first = pending
        .insert(
            ToolId::LocalSearch,
            json!({"queries":[{"path":"src","matchString":["run"],"page":2}]}),
            SavedWork::ToolResume { index: Some(0) },
        )
        .expect("randomness");
    assert!(registry.retain(pending, &context()).is_ok());
    let opened = registry
        .open(ToolId::LocalSearch, &first, [1; 32])
        .expect("first");
    let mut pending = opened.pending();
    let next = pending
        .insert(
            ToolId::LocalFetch,
            json!({"queries":[{"path":"src/a.ts"}]}),
            SavedWork::ToolResume { index: Some(0) },
        )
        .expect("randomness");
    assert!(registry.retain(pending, &context()).is_ok());
    assert!(
        registry
            .open(ToolId::LocalSearch, &first, [1; 32])
            .is_some()
    );
    assert!(registry.open(ToolId::LocalFetch, &next, [1; 32]).is_some());
}
