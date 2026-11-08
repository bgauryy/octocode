use super::run_batch;
use crate::execution::ExecutionError;

#[test]
fn rows_come_back_in_query_order_at_any_width() {
    for width in [1, 3] {
        let rows = run_batch(width, &[1, 2, 3], |query| Ok(query * 10)).expect("batch");
        assert_eq!(rows, [10, 20, 30], "width {width}");
    }
}

#[test]
fn a_failed_row_fails_the_batch() {
    for width in [1, 2] {
        let failed = run_batch(width, &[1, 2], |query| {
            if *query == 2 {
                Err(ExecutionError::WorkerFailed)
            } else {
                Ok(*query)
            }
        });
        assert!(
            matches!(failed, Err(ExecutionError::WorkerFailed)),
            "width {width}"
        );
    }
}
