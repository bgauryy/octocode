use crate::runtime::tool_output::ToolOutputOf;
use octocode_contract_types::ErrorCode;
use octocode_runtime_base::id::ToolId;
use octocode_test_support::json_fixture;
use octocode_tools_shared::source_capture::page::RERUN_HINT;
use octocode_tools_shared::source_capture::page::reader_failure;
use serde_json::json;

/// A missing file's capture failure carries the reader's own leads and
/// hints, not a re-run of a read that cannot succeed.
#[test]
fn a_final_reader_failure_keeps_its_own_leads_and_hints() {
    let lead = json_fixture(
        r#"{
            "kind": "related",
            "tool": "structureSearch",
            "input": {"queries": [{"operation": "tree", "path": "src"}]}
        }"#,
    );
    let data = json!({
        "path": "src/exector.rs",
        "errorCode": "notFound",
        "error": "Path does not exist: src/exector.rs",
        "hints": ["Check the spelling."],
        "related": [lead.clone()],
    });
    let failure = reader_failure(ToolId::LocalFetch.output(), &data);
    assert_eq!(failure.code, ErrorCode::NotFound);
    assert_eq!(failure.hints, vec!["Check the spelling.".to_owned()]);
    assert_eq!(failure.leads.len(), 1);
    assert_eq!(failure.leads[0].1, lead);
    // A refusal with no recovery of its own says the code's fix, never
    // a re-run that fails the same way.
    let bare = reader_failure(
        ToolId::LocalFetch.output(),
        &json_fixture(r#"{"errorCode": "outsideAllowedRoots", "error": "no"}"#),
    );
    assert_eq!(bare.hints.len(), 1, "{:?}", bare.hints);
    assert_ne!(bare.hints[0], RERUN_HINT);
    // A failure a re-run can change keeps the re-run beside its hints.
    let transient = reader_failure(
        ToolId::LocalFetch.output(),
        &json_fixture(
            r#"{
                "errorCode": "timeout",
                "error": "io",
                "hints": ["Disk busy."]
            }"#,
        ),
    );
    assert_eq!(
        transient.hints,
        vec!["Disk busy.".to_owned(), RERUN_HINT.to_owned()]
    );
    let undeclared = reader_failure(
        ToolId::LocalFetch.output(),
        &json_fixture(r#"{"errorCode": "madeUp", "error": "x"}"#),
    );
    assert_eq!(undeclared.code, ErrorCode::ClassificationContextFailed);
}
