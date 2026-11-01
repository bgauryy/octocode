use super::run_batch;
use crate::execution::ExecutionError;

#[test]
fn finalize_sees_every_row_in_query_order_before_shaping() {
    for width in [1, 3] {
        let mut seen = Vec::new();
        let rows = run_batch(
            width,
            &[1, 2, 3],
            |query| Ok(query * 10),
            |rows: &mut [i32]| {
                seen = rows.to_vec();
                rows[0] += 1;
                Ok(())
            },
        )
        .expect("batch");
        assert_eq!(seen, [10, 20, 30], "width {width}");
        assert_eq!(
            rows,
            [11, 20, 30],
            "width {width}: the filled rows are shaped"
        );
    }
}

#[test]
fn a_failed_finalize_fails_the_batch_and_a_failed_row_skips_it() {
    let failed = run_batch(
        2,
        &[1, 2],
        |query| Ok(*query),
        |_: &mut [i32]| Err(ExecutionError::WorkerFailed),
    );
    assert!(matches!(failed, Err(ExecutionError::WorkerFailed)));
    let mut finalized = false;
    let row_failed = run_batch(
        2,
        &[1, 2],
        |query| {
            if *query == 2 {
                Err(ExecutionError::WorkerFailed)
            } else {
                Ok(*query)
            }
        },
        |_: &mut [i32]| {
            finalized = true;
            Ok(())
        },
    );
    assert!(row_failed.is_err());
    assert!(!finalized);
}
