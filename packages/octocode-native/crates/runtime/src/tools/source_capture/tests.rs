use super::*;

/// A too-large selection names the long-context limit only to a row
/// under it, and always how to judge the selection in parts.
#[test]
fn a_too_large_selection_names_long_context_and_judging_in_parts() {
    let short = crate::id::clasify_policy::MAX_STATE_CHARS;
    let long = crate::id::clasify_policy::MAX_LONG_CONTEXT_STATE_CHARS;
    let error = too_large(short, "The selected source", IN_PARTS_HINT);
    assert_eq!(error.code, ErrorCode::ClassificationContextTooLarge);
    assert!(
        error.hints[0].contains("longContext:true"),
        "{:?}",
        error.hints
    );
    assert!(error.hints[0].contains(&long.to_string()));
    assert_eq!(error.hints[1], IN_PARTS_HINT);
    assert!(IN_PARTS_HINT.contains("operation:\"sections\""));
    let error = too_large(long, "The selected source", IN_PARTS_HINT);
    assert_eq!(error.hints, [IN_PARTS_HINT]);
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
