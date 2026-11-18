use super::*;
use octocode_test_support::json_fixture;

fn callers() -> LspSearchQuery {
    serde_json::from_value(json_fixture(
        r#"{
                "operation": "callers",
                "path": "/repo/a.ts",
                "symbolName": "f",
                "line": 1
            }"#,
    ))
    .expect("lsp query")
}

#[test]
fn every_page_is_its_own_call_whatever_its_revision() {
    let query = callers();
    let unbounded = response_scope(&query, "content".into(), SystemTime::now(), None);
    let fingerprinted = response_scope(&query, "content".into(), SystemTime::now(), Some("fp"));
    assert!(unbounded.call.is_some(), "a page past the walk bounds");
    assert!(fingerprinted.call.is_some());
    assert_ne!(unbounded.call, fingerprinted.call, "pages are not one call");
}

/// A scoped revision names the instant it observed its scope at, with
/// or without a fingerprint, which a continuation page observes again;
/// an unscoped revision names none.
#[test]
fn a_scoped_revision_carries_its_fingerprint_instant() {
    let observed = UNIX_EPOCH + std::time::Duration::from_nanos(1_700_000_000_123_456_789);
    let scoped = response_scope(&callers(), "content".into(), observed, Some("fp"));
    assert_eq!(observed_at(&scoped.generation), Some(observed));
    let later = response_scope(
        &callers(),
        "content".into(),
        observed + std::time::Duration::from_secs(1),
        Some("fp"),
    );
    assert_ne!(later.generation, scoped.generation);
    let unfingerprinted = response_scope(&callers(), "content".into(), observed, None);
    assert_eq!(observed_at(&unfingerprinted.generation), Some(observed));
    assert_ne!(unfingerprinted.generation, scoped.generation);
    let hover = serde_json::from_value(json_fixture(
        r#"{
                "operation": "hover",
                "path": "/repo/a.ts",
                "symbolName": "f",
                "line": 1
            }"#,
    ))
    .expect("lsp query");
    let unscoped = response_scope(&hover, "content".into(), observed, Some("fp"));
    assert_eq!(unscoped.generation, "content");
    assert_eq!(observed_at(&unscoped.generation), None);
}

#[test]
fn fresh_pages_own_independent_provider_evidence() {
    let observed = SystemTime::now();
    let first = response_scope(&callers(), "content".into(), observed, Some("fp"));
    let next = response_scope(&callers(), "content".into(), observed, Some("fp"));
    assert_eq!(first.generation, next.generation);
    assert!(!std::sync::Arc::ptr_eq(
        first.answers.as_ref().expect("answers"),
        next.answers.as_ref().expect("answers")
    ));
}
