use super::*;
use octocode_engine::text::analysis::SourceError;

/// A source-correspondence deadline is a timeout the caller can widen
/// or narrow around; only a cancellation is `cancelled`.
#[test]
fn a_source_deadline_is_a_timeout_not_a_cancellation() {
    assert_eq!(
        ToolError::source(SourceError::Deadline).code,
        ErrorCode::Timeout
    );
    assert_eq!(
        ToolError::source(SourceError::Cancelled).code,
        ErrorCode::Cancelled
    );
}
