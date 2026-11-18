use super::*;

/// What this request saw resolve is its own fact: a continuation that
/// resumes it re-checks a repository that may have gone since.
#[test]
fn a_resolved_repository_does_not_travel_in_continuation_evidence() {
    let request = RequestContext::with_timeout(Duration::from_secs(5), 1024);
    let url = Url::parse("https://api.github.com/repos/a/b/pulls/1").expect("url");
    request.record_answered(&url);
    assert!(request.repository_resolves("A", "b"));
    let resumed = RequestContext::with_timeout(Duration::from_secs(5), 1024)
        .resuming(Arc::new(request.evidence()));
    assert!(!resumed.repository_resolves("a", "b"));
}
