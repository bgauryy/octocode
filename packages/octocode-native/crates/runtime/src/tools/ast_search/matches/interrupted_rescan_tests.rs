use super::types::{Scan, ScannedFile};
use super::*;
use octocode_contract_types::{AstSearchQueryMatch0, ErrorCode};
use octocode_engine::structural::{FileMatches, StructuralDiagnostic};
use serde_json::json;

/// A rescan cut by the match deadline (zero matches, status truncated)
/// is a retryable timeout for the same page, not a stale snapshot; a
/// complete rescan that differs stays stale.
#[test]
fn a_deadline_cut_rescan_is_a_retryable_timeout() {
    let query: AstSearchQueryMatch0 = serde_json::from_value(json!({"operation":"match","path":"/repo/a.min.js","rule":{"pattern":"f($A)"},"matchPage":9,"snapshot":"earlier"}))
    .expect("typed row");
    let q = MatchQuery::Content(&query);
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
    let calls = error.calls.expect("retry");
    assert_eq!(calls["retry"]["kind"], "continue");
    let retry = &calls["retry"]["input"]["queries"][0];
    assert_eq!(retry["matchPage"], 9);
    assert_eq!(retry["snapshot"], "earlier");
    let changed = Scan {
        files: vec![file(vec![], "ok")],
        ..Scan::default()
    };
    assert!(interrupted_rescan(q, &changed).is_none());
}
