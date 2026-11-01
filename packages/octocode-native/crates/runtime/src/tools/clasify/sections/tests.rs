use super::assess::{pending_cost, settle_unsupported};
use super::emit::{answered_cell, emit, terminal_saved};
use super::*;
use crate::cache::CacheConfig;
use crate::execution::SelectionLimits;
use std::time::{Duration, Instant};

fn test_context(timeout: Duration) -> ExecutionContext {
    ExecutionContext {
        cancellation: tokio_util::sync::CancellationToken::new(),
        deadline: Instant::now() + timeout,
        response_window: None,
        github_credential: None,
    }
}

fn session() -> Session {
    Session {
        id: "a".repeat(64),
        context: None,
        questions: json!({"a":{"type":"noul","instructions":"a"},"b":{"type":"noul","instructions":"b"}}),
        keys: vec!["a".into(), "b".into()],
        model: Some("jev-1.13.0".into()),
        resolved_model: None,
        long_context: false,
        resources: vec![Resource {
            key: "resource".into(),
            metadata: json!({"status":"ready"}),
            local_check: None,
            units: vec![Unit {
                metadata: json!({"id":"s0","kind":"text","target":{"bytes":[0,4],"lines":[1,1]},"context":[]}),
                state: json!({"target":{"text":"text"}}),
                chars: 26,
                cells: vec![Some(answered_cell(json!({"type":"noul","noul":0.9}))), None],
                emitted: false,
            }],
        }],
        github_identity: None,
        batches: Vec::new(),
        active: Vec::new(),
    }
}

#[test]
fn section_answer_projection_preserves_values_and_distributions_with_shared_type() {
    for (provider, expected) in [
        (
            json!({"type":"noul","noul":0.9}),
            json!({"status":"answered","answer":{"noul":0.9}}),
        ),
        (
            json!({"type":"choice","choice":"present","confidence":0.75,"probabilities":{"present":0.75,"absent":0.2,"unknown":0.05}}),
            json!({"status":"answered","answer":{"choice":"present","confidence":0.75,"probabilities":{"present":0.75,"absent":0.2,"unknown":0.05}}}),
        ),
        (
            json!({"type":"score","score":1,"confidence":0.8,"probabilities":{"0":0.2,"1":0.8},"legend":{"0":"absent","1":"present"}}),
            json!({"status":"answered","answer":{"score":1,"confidence":0.8,"probabilities":{"0":0.2,"1":0.8}}}),
        ),
    ] {
        assert_eq!(answered_cell(provider), expected);
    }
}

#[test]
fn partial_cells_are_held_until_the_complete_section_emits_exactly_once() {
    let mut session = session();
    let usage = ClassificationUsage::default();
    let first = emit(&mut session, &usage, None);
    assert_eq!(first["resources"]["resource"]["sections"], json!([]));
    assert_eq!(
        first["resources"]["resource"]["coverage"]["answered"],
        json!(1)
    );
    assert_eq!(
        first["resources"]["resource"]["coverage"]["unassessed"],
        json!(1)
    );
    session.resources[0].units[0].cells[1] = Some(
        json!({"status":"error","error":{"errorCode":"classificationProviderError","error":"refused"}}),
    );
    let second = emit(&mut session, &usage, None);
    assert_eq!(
        second["resources"]["resource"]["sections"]
            .as_array()
            .map(Vec::len),
        Some(1)
    );
    assert_eq!(
        second["resources"]["resource"]["sections"][0]["answers"]
            .as_object()
            .map(serde_json::Map::len),
        Some(2)
    );
    let third = emit(&mut session, &usage, None);
    assert_eq!(third["resources"]["resource"]["sections"], json!([]));
    assert_eq!(
        third["resources"]["resource"]["coverage"]["remainingSections"],
        json!(0)
    );
}

#[test]
fn terminal_outcome_preserves_answer_and_explains_every_unassessed_cell() {
    let mut session = session();
    let outcome = terminal(&mut session, "budget ended", ClassificationUsage::default());
    let section = &outcome.output["resources"]["resource"]["sections"][0];
    assert_eq!(section["answers"]["a"]["answer"]["noul"], json!(0.9));
    assert_eq!(
        section["answers"]["b"],
        json!({"status":"unassessed","reason":"budget ended"})
    );
    assert_eq!(
        outcome.output["resources"]["resource"]["coverage"]["terminalReason"],
        json!("budget ended")
    );
    assert!(outcome.output.get("next").is_none());
}

#[test]
fn assessment_locks_exclude_the_same_plan_but_not_an_independent_plan() {
    let directory = tempfile::tempdir().expect("lock directory");
    let first = Sessions::new(
        CacheConfig::default(),
        Some(directory.path().to_owned()),
        "policy".into(),
    );
    let second = Sessions::new(
        CacheConfig::default(),
        Some(directory.path().to_owned()),
        "policy".into(),
    );
    let key = first.key(&[1; 32], &"a".repeat(64));
    let other = first.key(&[1; 32], &"b".repeat(64));
    let held = first
        .transaction_lock(&key, &test_context(Duration::from_secs(1)))
        .expect("first transaction");
    assert!(
        second
            .transaction_lock(&key, &test_context(Duration::from_millis(20)))
            .is_err()
    );
    assert!(
        second
            .transaction_lock(&other, &test_context(Duration::from_millis(20)))
            .is_ok()
    );
    assert!(second.save(
        other.clone(),
        session(),
        &test_context(Duration::from_secs(1))
    ));
    assert!(
        second
            .current(&other, &test_context(Duration::from_secs(1)))
            .expect("read")
            .is_some()
    );
    drop(held);
    assert!(
        second
            .transaction_lock(&key, &test_context(Duration::from_millis(20)))
            .is_ok()
    );
}

/// A failed read's own leads follow its hints by name, as the hint
/// lines a failed row publishes.
#[test]
fn an_error_preserves_executable_recovery_and_prose_separately() {
    let lead = crate::response::result::Continuation::related(
        crate::id::ToolId::StructureSearch,
        json!({"operation":"tree","path":"src"}),
    )
    .build();
    let mut error = ClassificationError::with_hints(
        ErrorCode::PathNotFound,
        "Path does not exist: nope.rs",
        Vec::new(),
    );
    assert_eq!(
        error_value(&error),
        json!({"errorCode":"pathNotFound","error":"Path does not exist: nope.rs"})
    );
    error.leads.push(("viewTree".into(), lead.clone()));
    error.hints.push("Check the spelling.".into());
    assert_eq!(error_value(&error)["hints"], json!(["Check the spelling."]));
    assert_eq!(error_value(&error)["related"], json!([lead]));
}

#[test]
fn work_token_is_bounded_and_does_not_accept_arbitrary_plan_keys() {
    let id = "c".repeat(64);
    let value = token(&id, usize::MAX);
    assert!(
        value.len() <= 128,
        "work token exceeded the canonical bound"
    );
    assert_eq!(decode(&value), Some((id.as_str(), usize::MAX)));
    assert!(decode("sections-v1:../other:0").is_none());
    assert!(decode(&format!("sections-v1:{}:no", id)).is_none());
}

#[test]
fn permanently_unsupported_cells_are_settled_before_lifetime_preflight() {
    let mut session = session();
    let limits = SelectionLimits {
        max_candidates: 1,
        max_chars: 1,
        timeout: Duration::from_secs(1),
    };
    assert_eq!(settle_unsupported(&mut session, limits), 1);
    assert_eq!(pending_cost(&session), (0, 0));
    let output = emit(&mut session, &ClassificationUsage::default(), None);
    let answers = &output["resources"]["resource"]["sections"][0]["answers"];
    assert_eq!(answers["a"]["status"], "answered");
    assert_eq!(answers["b"]["status"], "unsupported");
    assert_eq!(
        answers["b"]["error"]["errorCode"],
        "classificationContextTooLarge"
    );
    assert!(output.get("error").is_none());
    assert!(output.get("next").is_none());
}

#[test]
fn admission_is_shared_and_never_skips_an_over_budget_unit_then_loops() {
    let budget = Budget::new(SelectionLimits {
        max_candidates: 2,
        max_chars: 9,
        timeout: Duration::from_secs(1),
    });
    budget.deadline(Instant::now() + Duration::from_secs(2));
    assert!(budget.admit(5));
    assert!(!budget.admit(5));
    assert!(budget.admit(4));
    assert!(!budget.admit(0));
}

#[test]
fn retained_work_is_partitioned_by_access_and_terminal_receipts_replay() {
    let sessions = Sessions::new(CacheConfig::default(), None, "policy".into());
    let runtime = Runtime {
        sessions: &sessions,
        access: [1; 32],
        can_resume: true,
    };
    let mut session = session();
    let output = terminal_saved(
        &mut session,
        "terminal",
        ClassificationUsage::default(),
        runtime,
        &test_context(Duration::from_secs(5)),
    )
    .output;
    let key = sessions.key(&runtime.access, &session.id);
    let retained = sessions.store.get(&key).expect("terminal batch retained");
    assert_eq!(retained.value.batches[0], output);
    assert!(
        sessions
            .store
            .get(&sessions.key(&[2; 32], &session.id))
            .is_none()
    );
    let other = Sessions::new(CacheConfig::default(), None, "other-policy".into());
    assert!(
        other
            .store
            .get(&other.key(&runtime.access, &session.id))
            .is_none()
    );
}
