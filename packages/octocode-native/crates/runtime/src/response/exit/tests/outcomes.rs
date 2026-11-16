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
fn assert_classes<const N: usize>(cases: [(Value, Option<ErrorClass>, bool, ExitClass); N]) {
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
            ExitClass::Failed(ErrorClass::CorrectInput),
        ),
        (
            json!({"results":[{"data":{}}],"responsePagination":{"hasMore":true}}),
            None,
            false,
            ExitClass::Success,
        ),
        (
            json!({"results":[row("error", "invalidPattern")]}),
            Some(ErrorClass::Unservable),
            true,
            ExitClass::Failed(ErrorClass::CorrectInput),
        ),
        (
            json!({"results":[row("error", "notFound")]}),
            Some(ErrorClass::CorrectInput),
            true,
            ExitClass::Failed(ErrorClass::CorrectInput),
        ),
        (
            json!({"results":[row("error", "fileAccessFailed")]}),
            Some(ErrorClass::Unservable),
            true,
            ExitClass::Failed(ErrorClass::Unservable),
        ),
        (
            json!({"results":[]}),
            None,
            true,
            ExitClass::Failed(ErrorClass::CorrectInput),
        ),
    ]);
}

/// A batch with a failed or rejected row ends like its worst one.
#[test]
fn a_batch_ends_like_its_worst_row() {
    assert_classes([
        // No row succeeded, so the batch failed like its error rows.
        (
            json!({"results":[row("empty", ""), row("error", "notFound")]}),
            None,
            false,
            ExitClass::Failed(ErrorClass::CorrectInput),
        ),
        (
            json!({"results":[row("empty", ""), row("error", "notFound"),
                    row("error", "fileAccessFailed")]}),
            None,
            false,
            ExitClass::Failed(ErrorClass::CorrectInput),
        ),
        // A failed row is never hidden by an answering one, whichever
        // order the batch lists them in.
        (
            json!({"results":[{"data":{}}, row("error", "notFound")]}),
            None,
            false,
            ExitClass::Failed(ErrorClass::CorrectInput),
        ),
        (
            json!({"results":[row("error", "notFound"), {"data":{}}]}),
            None,
            false,
            ExitClass::Failed(ErrorClass::CorrectInput),
        ),
        (
            json!({"results":[page_row(), row("error", "notFound")]}),
            None,
            false,
            ExitClass::Failed(ErrorClass::CorrectInput),
        ),
        (
            json!({"results":[{"data":{}}, row("error", "notFound"),
                    row("error", "fileAccessFailed")]}),
            None,
            false,
            ExitClass::Failed(ErrorClass::CorrectInput),
        ),
        // A correctInput row outranks every failed row (the documented 2).
        (
            json!({"results":[row("error", "notFound"), row("error", "invalidInput")]}),
            None,
            false,
            ExitClass::Failed(ErrorClass::CorrectInput),
        ),
        (
            json!({"results":[row("error", "invalidInput"), row("error", "notFound")]}),
            None,
            false,
            ExitClass::Failed(ErrorClass::CorrectInput),
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
        // A stale snapshot is an error row: exit 6, alone or beside
        // answering rows, even when a healthy page is offered.
        (
            stale_row(),
            Some(ErrorClass::Unservable),
            true,
            ExitClass::Failed(ErrorClass::Restart),
        ),
        (
            json!({"results":[page_row(), stale_row()["results"][0].clone()]}),
            None,
            false,
            ExitClass::Failed(ErrorClass::Restart),
        ),
        // A retryable failure (`next.retry`) is still a failure: it keeps
        // its own exit whether the other rows answered or found nothing.
        (
            json!({"results":[{"data":{}}, retry_row("rateLimited")]}),
            None,
            false,
            ExitClass::Failed(ErrorClass::Transient),
        ),
        (
            json!({"results":[row("empty", ""), retry_row("server")]}),
            None,
            false,
            ExitClass::Failed(crate::id::error_codes::class(ErrorCode::Server)),
        ),
        (
            json!({"results":[retry_row("rateLimited")]}),
            Some(ErrorClass::Transient),
            true,
            ExitClass::Failed(ErrorClass::Transient),
        ),
    ]);
}
