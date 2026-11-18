use super::*;

#[test]
fn invalid_progress_uses_the_error_without_a_redundant_hint() {
    assert_eq!(
        Output.error_hint(ErrorCode::InvalidPagination, &Value::Null),
        Some(octocode_runtime_base::response::output::NO_HINT)
    );
}
