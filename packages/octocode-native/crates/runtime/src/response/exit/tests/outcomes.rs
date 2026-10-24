use super::*;

fn row(status: &str, code: &str) -> Value {
    json!({"index":0,"status":status,"data":{"errorCode":code}})
}

fn retry_row(code: &str) -> Value {
    json!({"index":0,"status":"error","data":{"errorCode":code,"retryable":true,
            "next":{"retry":{"kind":"continue","tool":"ghSearchRepo","input":{"cursor":"issued-retry"}}}}})
}

fn page_row() -> Value {
    json!({"index":0,"data":{"next":{"nextPage":{
            "kind":"continue","tool":"localSearch","input":{"cursor":"issued-page"}}}}})
}

fn stale_row() -> Value {
    json!({"results":[{"index":0,"status":"error","data":{"errorCode":"staleSnapshot",
            "related":[{"kind":"related","tool":"localSearch","input":{"queries":[{"path":".","matchString":["needle"]}]}}]}}]})
}

/// Each `(response, failure, all rows failed, expected class)`.
fn assert_classes<const N: usize>(cases: [(Value, Option<FailureKind>, bool, ExitClass); N]) {
    for (value, failure, all_failed, expected) in cases {
        assert_eq!(
            outcome(value.clone(), failure, all_failed).exit_class(),
            expected,
            "{value}"
        );
    }
}

/// One row, or rows that agree, set the class.
#[test]
fn outcomes_classify_by_their_rows() {
    assert_classes([
        (
            json!({"results":[{"data":{}}]}),
            None,
            false,
            ExitClass::Success,
        ),
        (
            json!({"results":[row("empty", "")]}),
            None,
            false,
            ExitClass::Empty,
        ),
        (
            json!({"results":[{"data":{}}, row("error", "invalidInput")]}),
            None,
            false,
            ExitClass::InvalidInput,
        ),
        (
            json!({"results":[{"data":{}}],"responsePagination":{"hasMore":true}}),
            None,
            false,
            ExitClass::Success,
        ),
        (
            json!({"results":[row("error", "invalidPattern")]}),
            Some(FailureKind::Execution),
            true,
            ExitClass::InvalidInput,
        ),
        (
            json!({"results":[row("error", "pathNotFound")]}),
            Some(FailureKind::NotFound),
            true,
            ExitClass::Failed(FailureKind::NotFound),
        ),
        (
            json!({"results":[row("error", "fileAccessFailed")]}),
            Some(FailureKind::Execution),
            true,
            ExitClass::Failed(FailureKind::Execution),
        ),
        (json!({"results":[]}), None, true, ExitClass::InvalidInput),
    ]);
}

/// A batch with a failed or rejected row ends like its worst one.
#[test]
fn a_batch_ends_like_its_worst_row() {
    assert_classes([
        // No row succeeded, so the batch failed like its error rows.
        (
            json!({"results":[row("empty", ""), row("error", "pathNotFound")]}),
            None,
            false,
            ExitClass::Failed(FailureKind::NotFound),
        ),
        (
            json!({"results":[row("empty", ""), row("error", "pathNotFound"),
                    row("error", "fileAccessFailed")]}),
            None,
            false,
            ExitClass::Failed(FailureKind::Execution),
        ),
        // A failed row is never hidden by an answering one, whichever
        // order the batch lists them in.
        (
            json!({"results":[{"data":{}}, row("error", "pathNotFound")]}),
            None,
            false,
            ExitClass::Failed(FailureKind::NotFound),
        ),
        (
            json!({"results":[row("error", "pathNotFound"), {"data":{}}]}),
            None,
            false,
            ExitClass::Failed(FailureKind::NotFound),
        ),
        (
            json!({"results":[page_row(), row("error", "pathNotFound")]}),
            None,
            false,
            ExitClass::Failed(FailureKind::NotFound),
        ),
        (
            json!({"results":[{"data":{}}, row("error", "pathNotFound"),
                    row("error", "fileAccessFailed")]}),
            None,
            false,
            ExitClass::Failed(FailureKind::Execution),
        ),
        // A rejected row outranks every failed row (the documented 2).
        (
            json!({"results":[row("error", "pathNotFound"), row("error", "invalidInput")]}),
            None,
            false,
            ExitClass::InvalidInput,
        ),
        (
            json!({"results":[row("error", "invalidInput"), row("error", "pathNotFound")]}),
            None,
            false,
            ExitClass::InvalidInput,
        ),
    ]);
}

/// More to read, or a clasify answer, is success.
#[test]
fn a_continuation_or_an_answer_is_success() {
    assert_classes([
        // A remaining page, a partial read or a pending clasify walk
        // is success; the continuation is the page's `next.*`.
        (
            json!({"results":[{"data":{}}, page_row()]}),
            None,
            false,
            ExitClass::Success,
        ),
        (
            json!({"results":[page_row()]}),
            None,
            false,
            ExitClass::Success,
        ),
        (
            json!({"results":[{"status":"success","data":{"isPartial":true,"content":"x"}}]}),
            None,
            false,
            ExitClass::Success,
        ),
        (
            json!({"results":[{"index":0,"data":{"answers":{"q":{"type":"noul","noul":0.4}},
                    "usage":{"input_tokens":1,"output_tokens":1}}}]}),
            None,
            false,
            ExitClass::Success,
        ),
    ]);
}

/// A stale snapshot or a retryable failure still fails the batch.
#[test]
fn stale_and_retryable_rows_are_failures() {
    assert_classes([
        // A stale snapshot is an error row: exit 5, alone or beside
        // answering rows, even when a healthy page is offered.
        (
            stale_row(),
            Some(FailureKind::Execution),
            true,
            ExitClass::Failed(FailureKind::Execution),
        ),
        (
            json!({"results":[page_row(), stale_row()["results"][0].clone()]}),
            None,
            false,
            ExitClass::Failed(FailureKind::Execution),
        ),
        // A retryable failure (`next.retry`) is still a failure: it keeps
        // its own exit whether the other rows answered or found nothing.
        (
            json!({"results":[{"data":{}}, retry_row("rateLimited")]}),
            None,
            false,
            ExitClass::Failed(FailureKind::RateLimited),
        ),
        (
            json!({"results":[row("empty", ""), retry_row("server")]}),
            None,
            false,
            ExitClass::Failed(failure_kind(ErrorCode::Server)),
        ),
        (
            json!({"results":[retry_row("rateLimited")]}),
            Some(FailureKind::RateLimited),
            true,
            ExitClass::Failed(FailureKind::RateLimited),
        ),
    ]);
}
