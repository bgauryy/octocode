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
