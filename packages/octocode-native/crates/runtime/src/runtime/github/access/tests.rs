use super::*;
use octocode_github::ProviderAnswer;
use std::sync::Arc;

fn evidence(body: &str) -> ProviderEvidence {
    ProviderEvidence {
        answers: vec![ProviderAnswer {
            request: "GET /file".into(),
            status: 200,
            headers: Vec::new(),
            body: body.into(),
            next: None,
            not_found: None,
        }],
        notes: Default::default(),
    }
}

/// Only a call that observed exactly what it resumed reuses that digest:
/// any new answer is evidence of its own.
#[test]
fn only_an_unchanged_observation_keeps_the_resumed_digest() {
    assert_eq!(ResponseAccess::default().retained_digest(), None);
    let resumed = || ResponseAccess::resuming("digest".into(), Arc::new(evidence("body")));
    assert_eq!(resumed().retained_digest(), None, "nothing observed yet");
    let same = resumed();
    *same.observed.lock().unwrap() = evidence("body");
    assert_eq!(same.retained_digest(), Some("digest"));
    let changed = resumed();
    *changed.observed.lock().unwrap() = evidence("another body");
    assert_eq!(changed.retained_digest(), None);
}

/// A saved answer observed before the call is an older observation; one
/// a row of the call observed after it began is fresh.
#[test]
fn only_an_answer_older_than_the_call_predates_it() {
    let access = ResponseAccess::default();
    assert!(access.predates(access.started_ms - 1_000));
    assert!(!access.predates(access.started_ms + 1));
}
