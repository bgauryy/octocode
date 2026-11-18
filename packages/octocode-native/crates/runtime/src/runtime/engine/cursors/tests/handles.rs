use super::*;
use octocode_test_support::json_fixture;

/// The same continuation (tool, canonical input, saved work) issued to the
/// same access identity has one handle, whichever output issues it and in
/// whatever key order its input is spelled; anything else that differs
/// issues another. Retaining it again keeps it reachable.
#[test]
fn an_identical_continuation_has_one_handle() {
    let registry = registry("policy");
    let issue = |access: u8, tool, input: Value, index| {
        let mut pending = registry.pending([access; 32], None);
        let token = pending.insert(tool, input, SavedWork::ToolResume { index: Some(index) });
        assert!(registry.retain(pending, &context()).is_ok());
        token
    };
    let lead = || {
        json_fixture(
            r#"{
                "queries": [{"path": "src", "importerPage": 2, "snapshot": "s"}]
            }"#,
        )
    };
    let first = issue(1, ToolId::LspSearch, lead(), 0);
    let reordered = json_fixture(
        r#"{
            "queries": [{"snapshot": "s", "importerPage": 2, "path": "src"}]
        }"#,
    );
    assert_eq!(issue(1, ToolId::LspSearch, reordered, 0), first);
    assert_eq!(issue(1, ToolId::LspSearch, lead(), 0), first);
    for other in [
        issue(2, ToolId::LspSearch, lead(), 0),
        issue(1, ToolId::LocalSearch, lead(), 0),
        issue(1, ToolId::LspSearch, lead(), 1),
        issue(
            1,
            ToolId::LspSearch,
            json_fixture(
                r#"{
                    "queries": [{"path": "src", "importerPage": 3, "snapshot": "s"}]
                }"#,
            ),
            0,
        ),
    ] {
        assert_ne!(other, first);
    }
    let opened = registry
        .open(ToolId::LspSearch, &first, [1; 32])
        .expect("retained");
    assert_eq!(opened.member().input, lead());
    assert!(registry.open(ToolId::LspSearch, &first, [2; 32]).is_err());
    assert_eq!(
        first.len(),
        octocode_runtime_base::response::cursor::issued::placeholder().len()
    );
}

/// Reissuing the same work under a new policy must never resurrect an old
/// token: the new configuration, workspace, contract or planner can change
/// the evidence represented by that continuation.
#[test]
fn reissuing_work_under_another_policy_does_not_resurrect_old_tokens() {
    let disk = tempfile::tempdir().unwrap();
    let make = |policy: &str| {
        Registry::new(
            CacheConfig::derived(),
            Some(disk.path().into()),
            policy.into(),
        )
    };
    let (old, current) = (make("old-policy"), make("current-policy"));
    let access = [1; 32];
    let input = json_fixture(r#"{"queries": [{"path": "src"}]}"#);
    for work in [
        SavedWork::ToolResume { index: Some(0) },
        SavedWork::Rows {
            indices: vec![0, 1],
        },
        SavedWork::ResponsePage {
            shape: super::super::PageShape::default(),
            cursor: Cursor {
                key: [2; 32],
                digest: [3; 32],
                length: 600,
                offset: 1,
            },
            index: None,
            rows: None,
        },
    ] {
        let mut old_pending = old.pending(access, None);
        let old_token = old_pending.insert(ToolId::LocalFetch, input.clone(), work.clone());
        old.retain(old_pending, &context()).unwrap();
        old.clear_memory();
        assert!(old.open(ToolId::LocalFetch, &old_token, access).is_ok());
        assert!(matches!(
            current.open(ToolId::LocalFetch, &old_token, access),
            Err(Refusal::NotRetained)
        ));
        let mut pending = current.pending(access, None);
        let new_token = pending.insert(ToolId::LocalFetch, input.clone(), work);
        current.retain(pending, &context()).unwrap();
        current.clear_memory();
        assert!(current.open(ToolId::LocalFetch, &new_token, access).is_ok());
        assert!(
            matches!(
                current.open(ToolId::LocalFetch, &old_token, access),
                Err(Refusal::NotRetained)
            ),
            "new policy resurrected old token {old_token}",
        );
        assert_ne!(old_token, new_token);
    }
}

/// This wire fixture was issued before handles included the replay policy,
/// for access [1; 32], localFetch({queries:[{path:"src"}]}), ToolResume(0).
#[test]
fn policy_free_handles_stay_expired_after_identical_work_is_reissued() {
    let old_token = "cursor2.-GTMq4TBZprr_BG8mbWo3gYNqvcjihLiANcr7kGTweaA";
    let registry = registry("current-policy");
    let mut pending = registry.pending([1; 32], None);
    let new_token = pending.insert(
        ToolId::LocalFetch,
        json_fixture(r#"{"queries": [{"path": "src"}]}"#),
        SavedWork::ToolResume { index: Some(0) },
    );
    registry.retain(pending, &context()).unwrap();
    assert!(
        registry
            .open(ToolId::LocalFetch, &new_token, [1; 32])
            .is_ok()
    );
    assert!(matches!(
        registry.open(ToolId::LocalFetch, old_token, [1; 32]),
        Err(Refusal::NotRetained)
    ));
}
