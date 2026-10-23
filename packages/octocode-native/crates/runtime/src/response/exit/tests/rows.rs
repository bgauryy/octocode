use super::*;

#[test]
fn missing_local_path_is_not_found_other_errors_are_execution() {
    assert_eq!(failure_kind(ErrorCode::PathNotFound), FailureKind::NotFound);
    assert_eq!(failure_kind(ErrorCode::NotFound), FailureKind::NotFound);
    assert_eq!(
        failure_kind(ErrorCode::VersionNotFound),
        FailureKind::NotFound
    );
    assert_eq!(
        failure_kind(ErrorCode::FileAccessFailed),
        FailureKind::Execution
    );
}

#[test]
fn clasify_whose_every_row_failed_exits_like_its_failure() {
    let not_found = Some(ExitClass::Failed(FailureKind::NotFound));
    let execution = Some(ExitClass::Failed(FailureKind::Execution));
    let failed =
        |code: &str| json!({"queries":[{"id":"q","error":{"errorCode":code,"error":"m"}}]});
    // localFetch reports a missing source as pathNotFound + NotFound.
    let missing = failed("pathNotFound");
    assert_eq!(
        clasify_failure(&missing, Some(FailureKind::NotFound)),
        not_found
    );
    assert_eq!(clasify_failure(&missing, None), not_found);
    assert_eq!(
        clasify_failure(&failed("fileAccessFailed"), None),
        execution
    );
    assert_eq!(
        clasify_failure(&failed("classificationProviderError"), None),
        execution
    );
    assert_eq!(
        clasify_failure(&failed("rateLimited"), None),
        Some(ExitClass::Failed(FailureKind::RateLimited))
    );
    assert_eq!(
        clasify_failure(&failed("invalidInput"), Some(FailureKind::NotFound)),
        Some(ExitClass::InvalidInput),
        "a rejected request stays a caller error"
    );
    assert_eq!(clasify_failure(&json!({"queries":[]}), None), None);
    let answered = json!({"queries":[{"id":"q","answers":{}}]});
    assert_eq!(clasify_failure(&answered, None), None);
}

/// A row failed after execution (classification of a provider page)
/// carries no execution failure kind: its code decides, as on a row
/// that failed while executing.
#[test]
fn a_row_failed_after_execution_exits_by_its_code() {
    let row =
        json!({"results":[{"status":"error","data":{"errorCode":"classificationProviderError"}}]});
    assert_eq!(
        outcome(row, None, true).exit_class(),
        ExitClass::Failed(FailureKind::Execution)
    );
    let refused = json!({"results":[{"status":"error","data":{"errorCode":"invalidInput"}}]});
    assert_eq!(
        outcome(refused, None, true).exit_class(),
        ExitClass::InvalidInput
    );
    assert_eq!(
        outcome(json!({}), None, true).exit_class(),
        ExitClass::InvalidInput
    );
}

/// Every row failed and one rejected its input: rejected input (2), as
/// in a batch where other rows answered.
#[test]
fn a_failed_batch_with_a_rejected_row_is_invalid_input() {
    let rows = json!({"results":[
        {"status":"error","data":{"errorCode":"invalidPattern"}},
        {"status":"error","data":{"errorCode":"pathNotFound"}}
    ]});
    for failure in [
        Some(FailureKind::Execution),
        Some(FailureKind::NotFound),
        None,
    ] {
        assert_eq!(
            outcome(rows.clone(), failure, true).exit_class(),
            ExitClass::InvalidInput,
            "{failure:?}"
        );
    }
}

/// A directory given to a file read (or a file to a listing) is
/// the caller's mistake: exit 2, not an execution failure.
#[test]
fn not_a_file_and_not_a_directory_are_invalid_input() {
    for code in ["notAFile", "notADirectory"] {
        let row = json!({"results":[{"status":"error","data":{"errorCode":code}}]});
        assert!(all_rows_invalid_input(&row), "{code}");
        assert_eq!(
            outcome(row, Some(FailureKind::Execution), true).exit_class(),
            ExitClass::InvalidInput,
            "{code}"
        );
    }
}

/// The contract's `errorCodes` table decides every exit; one code,
/// one class, whichever tool or runtime layer reports it.
#[test]
fn every_declared_code_maps_to_one_exit_class() {
    use crate::id::error_codes::ALL;
    assert!(!ALL.is_empty());
    for &code in ALL {
        let spelling = error_codes::as_str(code);
        assert!(
            spelling.chars().all(|c| c.is_ascii_alphanumeric())
                && spelling.starts_with(|c: char| c.is_ascii_lowercase()),
            "{code} is not camelCase"
        );
        assert_eq!(
            spelling,
            code.to_string(),
            "the wire spelling is the enum's"
        );
        assert_eq!(error_codes::parse(spelling), Some(code));
        let expected = match error_codes::class(code) {
            ErrorClass::InvalidInput => ExitClass::InvalidInput,
            ErrorClass::NotFound => ExitClass::Failed(FailureKind::NotFound),
            ErrorClass::Authentication => ExitClass::Failed(FailureKind::Authentication),
            ErrorClass::Permission => ExitClass::Failed(FailureKind::Permission),
            ErrorClass::RateLimited => ExitClass::Failed(FailureKind::RateLimited),
            ErrorClass::Execution => ExitClass::Failed(FailureKind::Execution),
        };
        let row = json!({"results":[{"status":"error","data":{"errorCode":code}}]});
        assert_eq!(
            outcome(row, Some(failure_kind(code)), true).exit_class(),
            expected,
            "{code}"
        );
        let runtime = RuntimeError::new(code, "m");
        assert_eq!(runtime.exit_class(), expected, "runtime {code}");
        let mixed_clasify = json!({"queries":[
            {"id":"ok","answers":{"q":{"type":"noul","noul":0.9}}},
            {"id":"failed","error":{"errorCode":code,"error":"failed"}}
        ]});
        assert_eq!(
            outcome(mixed_clasify, None, false).exit_class(),
            expected,
            "mixed clasify {code}"
        );
    }
    // No suffix or prefix matching: an undeclared spelling has no class.
    for undeclared in ["ast.language.required", "x.input.invalid", "lsp.timeout"] {
        assert_eq!(error_codes::parse(undeclared), None);
        assert_eq!(crate::id::error_class(undeclared), ErrorClass::Execution);
        assert!(!is_invalid_input_code(undeclared), "{undeclared}");
    }
}

#[test]
fn rows_rejecting_caller_input_are_invalid_input() {
    let rows = json!({"results":[
        {"status":"error","data":{"errorCode":"invalidPattern"}},
        {"status":"error","data":{"errorCode":"invalidInput"}}
    ]});
    assert!(all_rows_invalid_input(&rows));
    let mixed = json!({"results":[
        {"status":"error","data":{"errorCode":"invalidPattern"}},
        {"status":"error","data":{"errorCode":"fileAccessFailed"}}
    ]});
    assert!(!all_rows_invalid_input(&mixed));
    assert!(!all_rows_invalid_input(&json!({"results":[]})));
}

#[test]
fn mixed_clasify_rows_follow_worst_row_precedence() {
    let good = json!({"id":"good", "answers":{"ok":{"type":"noul","noul":0.8}}});
    let bad = |code| json!({"id":"bad", "error":{"errorCode":code,"error":"failed"}});
    for (code, expected) in [
        ("notFound", ExitClass::Failed(FailureKind::NotFound)),
        ("rateLimited", ExitClass::Failed(FailureKind::RateLimited)),
        ("invalidInput", ExitClass::InvalidInput),
    ] {
        for rows in [vec![good.clone(), bad(code)], vec![bad(code), good.clone()]] {
            assert_eq!(
                outcome(json!({"queries":rows}), None, false).exit_class(),
                expected
            );
        }
    }
    assert_eq!(
        outcome(
            json!({"queries":[good,bad("notFound"),bad("invalidInput")]}),
            None,
            false
        )
        .exit_class(),
        ExitClass::InvalidInput
    );
}
