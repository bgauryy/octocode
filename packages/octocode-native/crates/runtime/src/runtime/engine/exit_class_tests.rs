//! A runtime error exits as its error code's class, as a row failing with
//! that code does (`octocode_runtime_base::response::exit`).
use super::RuntimeError;
use octocode_runtime_base::id::error_codes::{self, ALL, ErrorClass};
use octocode_runtime_base::response::exit::ExitClass;
use octocode_runtime_base::test_support::tool_outcome;
use serde_json::json;

/// The contract's `errorCodes` table decides every exit; one code,
/// one class, whichever tool or runtime layer reports it.
#[test]
fn every_declared_code_maps_to_one_exit_class() {
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
        let expected = ExitClass::Failed(error_codes::class(code));
        let row = json!({"results":[{"status":"error","data":{"errorCode":code}}]});
        assert_eq!(
            tool_outcome(row, Some(error_codes::class(code)), true).exit_class(),
            expected,
            "{code}"
        );
        let runtime = RuntimeError::new(code, "m");
        assert_eq!(runtime.exit_class(), expected, "runtime {code}");
        let mixed_clasify = json!({"results":[
            {"index":0,"data":{"answers":{"q":{"type":"noul","noul":0.9}}}},
            {"index":1,"status":"error","data":{"errorCode":code,"error":"failed"}}
        ]});
        assert_eq!(
            tool_outcome(mixed_clasify, None, false).exit_class(),
            expected,
            "mixed clasify {code}"
        );
    }
    // No suffix or prefix matching: an undeclared spelling has no class.
    for undeclared in ["ast.language.required", "x.input.invalid", "lsp.timeout"] {
        assert_eq!(error_codes::parse(undeclared), None);
        assert_eq!(
            octocode_runtime_base::id::error_class(undeclared),
            ErrorClass::Unservable
        );
    }
}
