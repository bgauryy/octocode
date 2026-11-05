use super::*;

/// A selection over one capture's memory says how to judge it in parts.
#[test]
fn a_too_large_selection_says_how_to_judge_it_in_parts() {
    let error = too_large(1_000, "The selected source", IN_PARTS_HINT);
    assert_eq!(error.code, ErrorCode::ClassificationContextTooLarge);
    assert_eq!(error.hints, [IN_PARTS_HINT]);
    assert!(IN_PARTS_HINT.contains("operation:\"sections\""));
    assert!(error.message.starts_with("The selected source exceeds"));
}

fn one_char_pages(count: usize) -> impl FnMut(&Value) -> Result<page::Page, ClassificationError> {
    move |row: &Value| {
        let at = row["at"].as_u64().unwrap_or(0);
        Ok(page::Page {
            content: "x".to_owned(),
            pin: Some("pin".to_owned()),
            next: (at + 1 < count as u64).then(|| json!({ "at": at + 1 })),
        })
    }
}

/// A selection takes as many reader pages as it has: only the state limit
/// ends it, never a page count.
#[test]
fn a_selection_of_many_small_pages_is_captured_whole() {
    let (text, pin) =
        collect_pages(json!({"at": 0}), one_char_pages(500), 1_000, false).expect("captured");
    assert_eq!(text.len(), 500);
    assert_eq!(pin.as_deref(), Some("pin"));
    let error = collect_pages(json!({"at": 0}), one_char_pages(500), 499, false)
        .expect_err("over the state limit");
    assert_eq!(error.code, ErrorCode::ClassificationContextTooLarge);
}

/// A reader that offers a page it already served fails as a defect rather
/// than looping or claiming the selection is too large.
#[test]
fn a_reader_that_repeats_a_page_fails_without_looping() {
    let repeat = |_: &Value| {
        Ok(page::Page {
            content: "x".to_owned(),
            pin: None,
            next: Some(json!({"at": 0})),
        })
    };
    let error =
        collect_pages(json!({"at": 0}), repeat, 1_000_000, false).expect_err("repeated page");
    assert_eq!(error.code, ErrorCode::ClassificationContextFailed);
}

/// The GitHub tools that can send a row to a classifier, each passing the name
/// rule. A GitHub tool that gains `classification` has to pass it too.
#[test]
fn only_the_covered_github_tools_take_classification() {
    let taking = ToolId::ALL
        .iter()
        .copied()
        .filter(|tool| {
            tool.is_github() && crate::contracts::query_declares(*tool, "classification")
        })
        .collect::<Vec<_>>();
    assert_eq!(taking, [ToolId::GhSearchCode, ToolId::GhGetFileContent]);
}

#[test]
fn a_withheld_github_source_is_the_local_refusal_with_its_direct_read() {
    let query = json!({"owner":"o","repo":"r","path":"a/.ssh/config","ref":"main"});
    let error = withheld_github_source(&query).expect("withheld");
    assert_eq!(error.code, ErrorCode::PathPolicyDenied);
    let (name, read) = &error.leads[0];
    assert_eq!(name, "read");
    assert_eq!(read["tool"], "ghGetFileContent");
    assert!(withheld_github_source(&json!({"owner":"o","repo":"r","path":"src/a.rs"})).is_none());
}
