use super::types::ScannedFile;
use super::*;
use octocode_contract_types::{AstSearchQueryMatchPattern, ErrorCode};

/// A rescan cut by the match deadline (zero matches, status truncated)
/// is a retryable timeout for the same page, not a stale snapshot; a
/// complete rescan that differs stays stale.
#[test]
fn a_deadline_cut_rescan_is_a_retryable_timeout() {
    let query: AstSearchQueryMatchPattern = serde_json::from_value(json!({
        "operation":"match","path":"/repo/a.min.js","pattern":"f($A)",
        "matchPage":9,"snapshot":"earlier"
    }))
    .expect("typed row");
    let q = MatchQuery::Pattern(&query);
    let file = |diagnostics, status: &str| ScannedFile {
        path: "a.min.js".to_owned(),
        parser: String::new(),
        matches: FileMatches::Held(vec![]),
        diagnostics,
        status: status.to_owned(),
        recovered: false,
        source: None,
    };
    let cut = Scan {
        files: vec![file(
            vec![StructuralDiagnostic {
                code: "structural.match.deadline".to_owned(),
                severity: "warning".to_owned(),
                stage: "match".to_owned(),
                message: "Structural matching exceeded its execution deadline".to_owned(),
                path: None,
                recovery: None,
            }],
            "truncated",
        )],
        ..Scan::default()
    };
    let error = interrupted_rescan(q, &cut).expect("a deadline cut is a timeout");
    assert_eq!(error.code, ErrorCode::Timeout);
    let retry = &error.next.expect("retry")["retry"]["query"]["queries"][0];
    assert_eq!(retry["matchPage"], 9);
    assert_eq!(retry["snapshot"], "earlier");
    let changed = Scan {
        files: vec![file(vec![], "ok")],
        ..Scan::default()
    };
    assert!(interrupted_rescan(q, &changed).is_none());
}
