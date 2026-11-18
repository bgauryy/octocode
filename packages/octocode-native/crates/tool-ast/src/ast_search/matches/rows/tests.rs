use super::*;

#[test]
fn a_capture_is_inside_a_span_by_position_not_by_text() {
    let call = ((1, 0), (1, 10)); // `add(1, 1)`
    assert!(within(call, ((1, 4), (1, 5))));
    assert!(within(call, ((1, 7), (1, 8))), "equal text, other place");
    assert!(within(call, call));
    assert!(!within(call, ((1, 4), (1, 11))), "ends past the span");
    assert!(!within(call, ((2, 0), (2, 1))), "a later line");
    assert!(within(((1, 5), (3, 2)), ((2, 0), (2, 40))), "inner lines");
    assert!(!within(((1, 5), (3, 2)), ((1, 4), (1, 6))), "starts before");
}

fn diagnostic(code: &str) -> StructuralDiagnostic {
    StructuralDiagnostic {
        code: code.to_owned(),
        severity: "error".to_owned(),
        stage: "search".to_owned(),
        message: "failed".to_owned(),
        path: None,
        recovery: None,
    }
}

#[test]
fn diagnostic_errors_carry_only_declared_error_codes() {
    for (engine, wire) in [
        (octocode_engine::structural::RULE_INVALID, "invalidPattern"),
        ("structural.file.unreadable", "fileReadFailed"),
        (octocode_engine::structural::MATCH_DEADLINE, "timeout"),
        ("structural.unknown.failure", "executionFailed"),
    ] {
        assert_eq!(
            octocode_runtime_base::id::error_codes::as_str(
                diagnostic_error(&diagnostic(engine)).code
            ),
            wire,
            "{engine}"
        );
    }
}
