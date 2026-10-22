//! Exit classification: how a finished call ended, decided once from its
//! public response so every interface reports the same status.

use super::outcome::ToolOutcome;
use super::result::{FailureKind, failure_kind};
use super::rows::is_invalid_input_code;
use crate::id::error_codes::{self, ErrorClass};
use octocode_contract_types::ErrorCode;
use serde_json::Value;

/// How a call ended.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExitClass {
    /// Results, whether or not a page, a partial source read or a clasify walk
    /// remains: more to read is a continuation (`next.*`, `pagination.hasMore`),
    /// never a different outcome.
    Success,
    /// Every row ran and found nothing, even when it offers a recovery call.
    Empty,
    /// The call, or any row of it, rejected the caller's input.
    InvalidInput,
    /// At least one row failed.
    Failed(FailureKind),
}

impl ToolOutcome {
    #[must_use]
    pub fn exit_class(&self) -> ExitClass {
        let value = &self.structured_content;
        let rows: &[Value] = value["results"].as_array().map_or(&[], Vec::as_slice);
        if let Some(class) = clasify_failure(value, self.failure) {
            return class;
        }
        if self.all_failed {
            // A rejected row ends the call as rejected input, as in a batch
            // where other rows answered.
            if rows.iter().any(is_invalid_input_row) {
                return ExitClass::InvalidInput;
            }
            return match self.failure {
                Some(kind) => ExitClass::Failed(kind),
                // A row a later stage failed (a provider page whose
                // classification failed) states its code; the same code
                // ends the call the same way on every tool. A refusal with
                // no row (a gate or admission-time validation) is the
                // caller's request, never success.
                None => failed_rows(rows).map_or(ExitClass::InvalidInput, ExitClass::Failed),
            };
        }
        // Batch precedence, worst row first: a rejected row (2), a failed
        // row (its shared failure, else execution; a retryable failure is
        // still a failure), an empty batch (1; a page past the end reads
        // nothing either), success (0). An answering row
        // never hides a failed one.
        if rows.iter().any(is_invalid_input_row) {
            ExitClass::InvalidInput
        } else if let Some(kind) = failed_rows(rows) {
            ExitClass::Failed(kind)
        } else if !rows.is_empty()
            && rows
                .iter()
                .all(|row| matches!(row["status"].as_str(), Some("empty" | "outOfRange")))
        {
            ExitClass::Empty
        } else {
            ExitClass::Success
        }
    }
}

impl ExitClass {
    /// How a call that failed with `code` ended: the contract's class of the
    /// code (`errorCodes`), whichever interface or command reports it.
    #[must_use]
    pub fn of(code: ErrorCode) -> Self {
        match error_codes::class(code) {
            ErrorClass::InvalidInput => Self::InvalidInput,
            _ => Self::Failed(failure_kind(code)),
        }
    }
}

/// Classification rows use their own error envelope. Apply the same worst-row
/// precedence to mixed and all-failed batches: input rejection first, then a
/// shared failure kind, otherwise execution failure.
fn clasify_failure(value: &Value, failure: Option<FailureKind>) -> Option<ExitClass> {
    let queries = value["queries"].as_array().filter(|q| !q.is_empty())?;
    let codes: Vec<Option<ErrorCode>> = queries.iter().filter_map(error_code).collect();
    if codes.is_empty() {
        return None;
    }
    Some(
        if codes
            .iter()
            .flatten()
            .any(|code| error_codes::class(*code) == ErrorClass::InvalidInput)
        {
            ExitClass::InvalidInput
        } else if let Some(failure) = failure {
            ExitClass::Failed(failure)
        } else {
            ExitClass::Failed(
                shared_failure(
                    codes
                        .into_iter()
                        .map(|code| code.map_or(FailureKind::Execution, failure_kind)),
                )
                .unwrap_or(FailureKind::Execution),
            )
        },
    )
}

/// A clasify row's `error.errorCode`: `Some(None)` for an undeclared one,
/// which the output contract already rejects (an execution failure).
fn error_code(value: &Value) -> Option<Option<ErrorCode>> {
    value
        .pointer("/error/errorCode")
        .and_then(Value::as_str)
        .map(error_codes::parse)
}

/// An error row whose `errorCode` rejects the caller's input.
fn is_invalid_input_row(row: &Value) -> bool {
    row["status"] == "error"
        && row
            .pointer("/data/errorCode")
            .and_then(Value::as_str)
            .is_some_and(is_invalid_input_code)
}

/// The failure of a batch's error rows: their shared failure, else an
/// execution failure.
fn failed_rows(rows: &[Value]) -> Option<FailureKind> {
    shared_failure(
        rows.iter()
            .filter(|row| row["status"] == "error")
            .map(|row| {
                row.pointer("/data/errorCode")
                    .and_then(Value::as_str)
                    .and_then(error_codes::parse)
                    .map_or(FailureKind::Execution, failure_kind)
            }),
    )
}

fn shared_failure(mut kinds: impl Iterator<Item = FailureKind>) -> Option<FailureKind> {
    let first = kinds.next()?;
    Some(if kinds.all(|kind| kind == first) {
        first
    } else {
        FailureKind::Execution
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::RuntimeError;
    use serde_json::json;

    fn all_rows_invalid_input(value: &Value) -> bool {
        value["results"]
            .as_array()
            .is_some_and(|rows| !rows.is_empty() && rows.iter().all(is_invalid_input_row))
    }

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
        let row = json!({"results":[{"status":"error","data":{"errorCode":"classificationProviderError"}}]});
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

    fn outcome(
        structured_content: Value,
        failure: Option<FailureKind>,
        all_failed: bool,
    ) -> ToolOutcome {
        ToolOutcome {
            structured_content,
            content: Vec::new(),
            failure,
            all_failed,
        }
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

    #[test]
    fn outcomes_classify_by_their_rows() {
        let row = |status: &str, code: &str| json!({"status":status,"data":{"errorCode":code}});
        let retry_row = |code: &str| {
            json!({"status":"error","data":{"errorCode":code,"retryable":true,
                "next":{"retry":{"tool":"ghSearchRepo","query":{"queries":[{}]}}}}})
        };
        let page_row = json!({"status":"success","data":{"next":{"nextPage":{
            "tool":"localSearch","query":{"queries":[{}]}}}}});
        let stale_row = json!({"results":[{"status":"error","data":{"errorCode":"staleSnapshot",
            "next":{"restart":{"tool":"localSearch","query":{}}}}}]});
        let cases = [
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
                json!({"results":[page_row.clone(), row("error", "pathNotFound")]}),
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
            // A remaining page, a partial read or a pending clasify walk
            // is success; the continuation is the page's `next.*`.
            (
                json!({"results":[{"data":{}}, page_row.clone()]}),
                None,
                false,
                ExitClass::Success,
            ),
            (
                json!({"results":[page_row.clone()]}),
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
                json!({"queries":[{"id":"b","answers":{"q":{"type":"noul","noul":0.4}},
                    "usage":{"input_tokens":1,"output_tokens":1}}]}),
                None,
                false,
                ExitClass::Success,
            ),
            // A stale snapshot is an error row: exit 5, alone or beside
            // answering rows, even when a healthy page is offered.
            (
                stale_row.clone(),
                Some(FailureKind::Execution),
                true,
                ExitClass::Failed(FailureKind::Execution),
            ),
            (
                json!({"results":[page_row.clone(), stale_row["results"][0].clone()]}),
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
        ];
        for (value, failure, all_failed, expected) in cases {
            assert_eq!(
                outcome(value.clone(), failure, all_failed).exit_class(),
                expected,
                "{value}"
            );
        }
    }

    /// Error codes are typed end to end: production source names a contract
    /// `errorCode` only as the generated `ErrorCode` enum. Every constructor,
    /// row builder and code-returning function takes or returns `ErrorCode`,
    /// so the compiler already refuses a string there; this scan covers what
    /// it cannot see: JSON bodies (`"errorCode": "x"`,
    /// `["errorCode"] = json!("x")`), a row's `errorCode` read and compared
    /// with a literal, a `code` compared or matched against one, a `code:`
    /// field set to one, and the body of a `fn ..code..() -> &'static str`.
    #[test]
    fn no_error_code_is_spelled_as_a_string() {
        let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("crates dir");
        let mut scanned = 0;
        let mut spelled = Vec::new();
        for krate in ["runtime/src", "cli/src", "runtime-napi/src"] {
            let mut pending = vec![crates.join(krate)];
            while let Some(dir) = pending.pop() {
                for entry in std::fs::read_dir(&dir).expect("read crate dir").flatten() {
                    let path = entry.path();
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if path.is_dir() {
                        if !["tests", "test_support", "fixtures"].contains(&name.as_str()) {
                            pending.push(path);
                        }
                        continue;
                    }
                    if !name.ends_with(".rs") || name == "tests.rs" || name.ends_with("_tests.rs") {
                        continue;
                    }
                    let source = std::fs::read_to_string(&path).expect("read source");
                    scanned += 1;
                    for (line, form) in spelled_codes(&source) {
                        spelled.push(format!("{}:{line}: {form}", path.display()));
                    }
                }
            }
        }
        assert!(scanned > 300, "the scan reads production source: {scanned}");
        assert!(
            spelled.is_empty(),
            "spell these contract errorCodes as ErrorCode::…:\n{}",
            spelled.join("\n")
        );
    }

    /// The scan catches each form it guards, and leaves the typed spellings,
    /// other vocabularies (`category`, engine diagnostics) and test modules.
    #[test]
    fn the_spelled_code_scan_finds_each_string_form() {
        let source = r#"
fn a() -> Value { json!({"error": m, "errorCode": "invalidInput"}) }
fn b(row: &mut Value) { row["errorCode"] = json!("notFound"); }
fn c(data: &Value) -> bool { data["errorCode"] == "rateLimited" }
fn d(row: &Value) -> bool { row.pointer("/data/errorCode").and_then(Value::as_str) == Some("staleSnapshot") }
fn e(code: &str) -> bool { code == "timeout" }
fn f(error: &E) -> bool { matches!(error.code.as_str(), "cancelled" | "closed") }
fn g() -> Failure { Failure { code: "requestFailed".into(), message } }
fn local_code(kind: Kind) -> &'static str {
    match kind {
        Kind::A => "fileReadFailed",
    }
}
fn typed(data: &Value) -> Value { json!({"errorCode": ErrorCode::InvalidInput, "category": "noLocations"}) }
fn other(category: &str, diagnostic: &D) -> bool { category == "noDiagnostics" || diagnostic.code == "structural.compile.failed" }
#[cfg(test)]
mod tests {
    fn pinned() -> Value { json!({"errorCode": "invalidInput"}) }
}
"#;
        let found: Vec<usize> = spelled_codes(source)
            .into_iter()
            .map(|(line, _)| line)
            .collect();
        assert_eq!(
            found,
            [2, 3, 4, 5, 6, 7, 8, 11],
            "{:?}",
            spelled_codes(source)
        );
    }

    /// `(line, text)` of each contract `errorCode` spelled as a string in
    /// `source`'s production part (a trailing `#[cfg(test)]` module is not).
    fn spelled_codes(source: &str) -> Vec<(usize, String)> {
        use crate::id::error_codes::parse;
        let test_module = regex::Regex::new(r"\n#\[cfg\(test\)\]\s*\nmod ").expect("pattern");
        let source = test_module
            .find(source)
            .map_or(source, |test| &source[..test.start()]);
        // The value of an `errorCode` JSON key is a code whatever it says.
        let json_value = regex::Regex::new(concat!(
            r#""errorCode"\s*:\s*"([^"]*)""#,
            r#"|\["errorCode"\]\s*=\s*json!\(\s*"([^"]*)""#,
        ))
        .expect("pattern");
        // A literal compared with a code: only a declared spelling is one.
        let compared = regex::Regex::new(concat!(
            r#"errorCode"\]?\)?[^;\n]*?[!=]=\s*(?:Some\()?"([A-Za-z]+)""#,
            r#"|\b(?:code|error_code)\s*[!=]=\s*(?:Some\()?"([A-Za-z]+)""#,
            r#"|\bcode:\s*"([A-Za-z]+)""#,
            r#"|matches!\([^,]*\bcode\b[^,]*,\s*"([A-Za-z]+)""#,
        ))
        .expect("pattern");
        let code_fn =
            regex::Regex::new(r"fn \w*code\w*\([^)]*\)\s*->\s*(?:Option<)?&'static str\s*\{")
                .expect("pattern");
        let literal = regex::Regex::new(r#""([A-Za-z]+)""#).expect("pattern");
        let line_of = |offset: usize| source[..offset].matches('\n').count() + 1;
        let text_of = |offset: usize| {
            let start = source[..offset].rfind('\n').map_or(0, |at| at + 1);
            let end = source[offset..]
                .find('\n')
                .map_or(source.len(), |at| offset + at);
            source[start..end].trim().to_owned()
        };
        let mut found = std::collections::BTreeMap::new();
        for captures in json_value.captures_iter(source) {
            let at = captures.get(0).expect("match").start();
            found.entry(line_of(at)).or_insert_with(|| text_of(at));
        }
        for captures in compared.captures_iter(source) {
            let code = captures.iter().skip(1).flatten().next().expect("group");
            if parse(code.as_str()).is_some() {
                found
                    .entry(line_of(code.start()))
                    .or_insert_with(|| text_of(code.start()));
            }
        }
        for head in code_fn.find_iter(source) {
            let mut depth = 1;
            let body = &source[head.end()..];
            let end = body
                .char_indices()
                .find(|(_, c)| {
                    depth += match c {
                        '{' => 1,
                        '}' => -1,
                        _ => 0,
                    };
                    depth == 0
                })
                .map_or(body.len(), |(end, _)| end);
            for code in literal
                .captures_iter(&body[..end])
                .filter_map(|c| c.get(1))
                .filter(|code| parse(code.as_str()).is_some())
            {
                let at = head.end() + code.start();
                found.entry(line_of(at)).or_insert_with(|| text_of(at));
            }
        }
        found.into_iter().collect()
    }
}
