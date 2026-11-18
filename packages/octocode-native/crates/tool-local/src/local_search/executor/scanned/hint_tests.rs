/// Call keys never reach the output (`attach_calls` lists a related
/// call in `related[]` without its key), so a hint naming one points
/// at nothing.
const INTERNAL_KEYS: [&str; 3] = ["readFile", "viewFileCount", "listScope"];

/// Each lead the hint names is one the refusal offers, and only those,
/// named by the tool its related[] call runs.
#[test]
fn a_directory_hint_names_only_offered_leads_by_tool() {
    for counted in [false, true] {
        for listed in [false, true] {
            let hint = super::directory_hint(counted, listed);
            assert_eq!(
                hint.contains("related[] localSearch call for per-file counts"),
                counted,
                "{hint}"
            );
            assert_eq!(
                hint.contains("structureSearch listing to split the scope"),
                listed,
                "{hint}"
            );
            assert!(
                hint.contains("narrow path, include or the pattern"),
                "{hint}"
            );
            for key in INTERNAL_KEYS {
                assert!(!hint.contains(key), "{hint}");
            }
        }
    }
}

/// A regex that compiles but hits the engine's matching limit in every
/// file is an execution failure, not an invalid pattern.
#[test]
fn a_matching_limit_in_every_file_is_an_execution_failure() {
    let stats = octocode_engine::types::TextSearchStats {
        error_count: Some(2),
        pattern_error_count: Some(2),
        first_error: Some("a.txt: PCRE2: error matching: match limit exceeded".into()),
        ..Default::default()
    };
    let error = super::unreadable_scope(&stats);
    assert_eq!(error.code, super::ErrorCode::ExecutionFailed, "{error:?}");
    assert!(error.message.contains("matching limit"), "{error:?}");
}

/// A one-file refusal names its localFetch lead by tool.
#[test]
fn a_one_file_hint_names_the_localfetch_call() {
    let hint = super::ONE_FILE_HINT;
    assert!(
        hint.contains("related[] localFetch call (it pages the file whole)"),
        "{hint}"
    );
    for key in INTERNAL_KEYS {
        assert!(!hint.contains(key), "{hint}");
    }
}
