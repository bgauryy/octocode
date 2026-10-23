use super::*;
use serde_json::json;

/// A missing file's capture failure carries the reader's own leads and
/// hints, not a re-run of a read that cannot succeed.
#[test]
fn a_final_reader_failure_keeps_its_own_leads_and_hints() {
    let lead = |tool: &str| json!({"tool": tool, "query": {"queries": [{"path": "src"}]}});
    let data = json!({
        "path": "src/exector.rs",
        "errorCode": "pathNotFound",
        "error": "Path does not exist: src/exector.rs",
        "hints": ["Check the spelling."],
        "next": {"viewTree": lead("structureSearch"), "findFile": lead("structureSearch")},
    });
    let failure = reader_failure(ToolId::LocalFetch, &data);
    assert_eq!(failure.code, ErrorCode::PathNotFound);
    assert_eq!(failure.hints, vec!["Check the spelling.".to_owned()]);
    let mut names: Vec<&str> = failure
        .leads
        .iter()
        .map(|(name, _)| name.as_str())
        .collect();
    names.sort_unstable();
    assert_eq!(names, ["findFile", "viewTree"]);
    // A refusal with no recovery of its own says the code's fix, never
    // a re-run that fails the same way.
    let bare = reader_failure(
        ToolId::LocalFetch,
        &json!({"errorCode": "outsideAllowedRoots", "error": "no"}),
    );
    assert_eq!(bare.hints.len(), 1, "{:?}", bare.hints);
    assert_ne!(bare.hints[0], RERUN_HINT);
    // A failure a re-run can change keeps the re-run beside its hints.
    let transient = reader_failure(
        ToolId::LocalFetch,
        &json!({
            "errorCode": "fileReadFailed", "error": "io", "hints": {"text": ["Disk busy."]}
        }),
    );
    assert_eq!(
        transient.hints,
        vec!["Disk busy.".to_owned(), RERUN_HINT.to_owned()]
    );
    let undeclared = reader_failure(
        ToolId::LocalFetch,
        &json!({"errorCode": "madeUp", "error": "x"}),
    );
    assert_eq!(undeclared.code, ErrorCode::ClassificationContextFailed);
}
