//! Exit classification: how a finished call ended, decided once from its
//! public response so every interface reports the same status.

use super::engine::{RuntimeError, ToolOutcome};
use crate::response::pages::{has_remaining_page, is_partial};
use crate::response::rows::{is_invalid_input_code, is_not_found_code};
use crate::tools::id::ToolId;
use crate::tools::result::FailureKind;
use serde_json::Value;

/// How a call ended.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExitClass {
    /// Results with nothing left to read.
    Success,
    /// Every row ran and found nothing, even when it offers a recovery call.
    Empty,
    /// The call, or any row of it, rejected the caller's input.
    InvalidInput,
    /// Results with more to read: a remaining page, a partial source read,
    /// or `next.clasify`.
    Incomplete,
    /// Nothing succeeded.
    Failed(FailureKind),
}

impl ToolOutcome {
    #[must_use]
    pub fn exit_class(&self) -> ExitClass {
        let value = &self.structured_content;
        if self.all_failed {
            return clasify_failure(value, self.failure).unwrap_or(match self.failure {
                Some(FailureKind::Execution) if all_rows_invalid_input(value) => {
                    ExitClass::InvalidInput
                }
                Some(kind) => ExitClass::Failed(kind),
                // A refusal with no failure kind (a gate or admission-time
                // validation) is the caller's request, never success.
                None => ExitClass::InvalidInput,
            });
        }
        let rows: &[Value] = value["results"].as_array().map_or(&[], Vec::as_slice);
        if rows.iter().any(is_invalid_input_row) {
            ExitClass::InvalidInput
        } else if !rows.is_empty() && rows.iter().all(|row| row["status"] == "empty") {
            ExitClass::Empty
        } else if rows.iter().any(|row| has_row_continuation(self.tool, row))
            || has_clasify_continuation(value)
            || value.pointer("/responsePagination/hasMore") == Some(&Value::Bool(true))
        {
            ExitClass::Incomplete
        } else {
            ExitClass::Success
        }
    }
}

impl RuntimeError {
    #[must_use]
    pub fn exit_class(&self) -> ExitClass {
        if self.code == "invalidInput" {
            ExitClass::InvalidInput
        } else {
            ExitClass::Failed(FailureKind::Execution)
        }
    }
}

/// A missing local path, registry package, or unresolved LSP anchor is
/// not-found (like a GitHub 404); every other domain error is an execution
/// failure.
pub(crate) fn failure_kind(code: &str) -> FailureKind {
    if matches!(code, "notFound" | "versionNotFound" | "anchorUnresolved")
        || is_not_found_code(code)
    {
        FailureKind::NotFound
    } else {
        FailureKind::Execution
    }
}

/// A clasify call in which every resource errored: the caller's request when
/// every error rejects it; when every delegated read failed alike, that read's
/// failure; else missing sources, throttling, or an execution/provider failure.
fn clasify_failure(value: &Value, failure: Option<FailureKind>) -> Option<ExitClass> {
    let queries = value["queries"].as_array().filter(|q| !q.is_empty())?;
    let mut codes = Vec::new();
    for query in queries {
        codes.extend(error_code(query));
        for resource in query["resources"].as_array().into_iter().flatten() {
            // Compact output states a single page's `error`/`answers` on the
            // resource, and hoists one error shared by every page there while
            // the pages keep their reads: the resource always counts.
            for page in
                std::iter::once(resource).chain(resource["pages"].as_array().into_iter().flatten())
            {
                codes.extend(error_code(page));
                codes.extend(
                    page["answers"]
                        .as_object()
                        .into_iter()
                        .flat_map(|answers| answers.values())
                        .filter_map(error_code),
                );
            }
        }
    }
    if codes.is_empty() {
        return None;
    }
    let all = |test: fn(&str) -> bool| codes.iter().all(|code| test(code));
    Some(if all(is_clasify_caller_code) {
        ExitClass::InvalidInput
    } else if let Some(failure) = failure {
        ExitClass::Failed(failure)
    } else if all(is_not_found_code) {
        ExitClass::Failed(FailureKind::NotFound)
    } else if all(|code| matches!(code, "classificationRateLimited" | "rateLimited")) {
        ExitClass::Failed(FailureKind::RateLimited)
    } else {
        ExitClass::Failed(FailureKind::Execution)
    })
}

fn error_code(value: &Value) -> Option<&str> {
    value.pointer("/error/errorCode").and_then(Value::as_str)
}

/// clasify error codes that reject the caller's request rather than report a
/// failed read or provider call.
fn is_clasify_caller_code(code: &str) -> bool {
    is_invalid_input_code(code)
        || matches!(
            code,
            "invalidClassificationContext"
                | "invalidClassificationRequest"
                | "classificationLocateUnsupported"
                | "classificationExpandedCellsExceeded"
                | "outsideAllowedRoots"
                | "pathValidationFailed"
        )
}

/// An error row whose `errorCode` rejects the caller's input.
fn is_invalid_input_row(row: &Value) -> bool {
    row["status"] == "error"
        && row
            .pointer("/data/errorCode")
            .and_then(Value::as_str)
            .is_some_and(is_invalid_input_code)
}

fn all_rows_invalid_input(value: &Value) -> bool {
    value["results"]
        .as_array()
        .is_some_and(|rows| !rows.is_empty() && rows.iter().all(is_invalid_input_row))
}

/// clasify returns `queries[].next.clasify`, a complete query rather than a
/// `{tool, query}` row continuation.
fn has_clasify_continuation(value: &Value) -> bool {
    value["queries"].as_array().is_some_and(|queries| {
        queries.iter().any(|query| {
            query
                .get("next")
                .and_then(|next| next.get(ToolId::Clasify.as_str()))
                .is_some_and(Value::is_object)
        })
    })
}

/// A row with more of its result remaining: a page that leaves more to read
/// or a partial source read. A `complete:true` row has nothing left to page;
/// any `next.*` it carries (e.g. astSearch `expandCaptures`) is a drill-down.
fn has_row_continuation(tool: ToolId, row: &Value) -> bool {
    let data = &row["data"];
    let complete = data["complete"] == Value::Bool(true);
    (!complete && has_remaining_page(tool, data))
        || (is_partial(data)
            && data["content"]
                .as_str()
                .is_some_and(|text| !text.is_empty()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn missing_local_path_is_not_found_other_errors_are_execution() {
        assert_eq!(failure_kind("pathNotFound"), FailureKind::NotFound);
        assert_eq!(failure_kind("notFound"), FailureKind::NotFound);
        assert_eq!(failure_kind("versionNotFound"), FailureKind::NotFound);
        assert_eq!(failure_kind("fileAccessFailed"), FailureKind::Execution);
    }

    #[test]
    fn clasify_whose_every_read_failed_exits_like_that_read() {
        let not_found = Some(ExitClass::Failed(FailureKind::NotFound));
        let execution = Some(ExitClass::Failed(FailureKind::Execution));
        let failed = |code: &str| {
            json!({"queries":[{"id":"q","resources":[{"id":"x","coverage":"error",
                "pages":[{"error":{"errorCode":code,"error":"m"}}]}]}]})
        };
        // localFetch reports a missing file as pathNotFound + NotFound.
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
        // Compact output: a single failed page is stated on the resource.
        let compact = |code: &str| {
            json!({"queries":[{"id":"q","resources":[{"id":"x","coverage":"error",
                "answers":{"q":{"error":{"errorCode":code,"error":"m"}}}}]}]})
        };
        assert_eq!(
            clasify_failure(&compact("classificationProviderError"), None),
            execution
        );
        assert_eq!(clasify_failure(&compact("pathNotFound"), None), not_found);
        assert_eq!(
            clasify_failure(&failed("rateLimited"), Some(FailureKind::RateLimited)),
            Some(ExitClass::Failed(FailureKind::RateLimited))
        );
        assert_eq!(
            clasify_failure(&failed("rateLimited"), None),
            Some(ExitClass::Failed(FailureKind::RateLimited))
        );
        assert_eq!(
            clasify_failure(
                &failed("classificationLocateUnsupported"),
                Some(FailureKind::NotFound)
            ),
            Some(ExitClass::InvalidInput),
            "a rejected request stays a caller error"
        );
        assert_eq!(clasify_failure(&json!({"queries":[]}), None), None);
        // N13: compact output hoists one shared page error to the resource
        // while pages keep their `next.read`; the resource error still counts.
        let hoisted = json!({"queries":[{"id":"q","resources":[{"id":"s",
            "error":{"errorCode":"classificationQuotaExhausted","error":"m"},
            "pages":[{"lines":[1,9],"next":{"read":{"tool":"localFetch",
                "query":{"queries":[{"path":"a.rs"}]}}}}]}]}]});
        assert_eq!(clasify_failure(&hoisted, None), execution);
    }

    #[test]
    fn optional_drill_downs_are_not_remaining_pages() {
        let call = json!({"tool":"ghGetHistoryItem","query":{"queries":[{"number":1}]}});
        let menu = json!({"data":{"next":{"readBody":call,"readPullRequest":call,"verifyReferences":call}}});
        assert!(!has_row_continuation(ToolId::LocalSearch, &menu));
        for name in ["nextPage", "continue", "expandScan", "retry"] {
            let row = json!({"data":{"next":{name:call}}});
            assert!(has_row_continuation(ToolId::LocalSearch, &row), "{name}");
        }
        let nested = json!({"data":{"nestedEvidence":{"next":{"nextPage":call}}}});
        assert!(has_row_continuation(ToolId::LocalSearch, &nested));
    }

    #[test]
    fn complete_rows_with_only_drill_downs_are_not_partial() {
        let call = json!({"tool":"astSearch","query":{"captureText":true}});
        let complete = json!({"data":{"complete":true,"next":{"expandCaptures":call}}});
        assert!(!has_row_continuation(ToolId::LocalSearch, &complete));
        let open = json!({"data":{"complete":false,"next":{"expandCaptures":call}}});
        assert!(has_row_continuation(ToolId::LocalSearch, &open));
    }

    /// LF4/P8: a directory given to a file read (or a file to a listing) is
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
    fn clasify_remaining_coverage_is_incomplete() {
        let pending = json!({"queries":[
            {"id":"a","results":[]},
            {"id":"b","results":[],"next":{"clasify":{"queries":[{"id":"b","resources":[]}]}}}
        ]});
        assert!(has_clasify_continuation(&pending));
        assert!(!has_clasify_continuation(
            &json!({"queries":[{"id":"a","results":[]}]})
        ));
    }

    #[test]
    fn nested_executable_continuation_is_incomplete() {
        let row = json!({"data":{"files":[{"path":"src/lib.rs","isPartial":true,
            "next":{"continue":{"tool":"ghGetFileContent",
                "query":{"owner":"a","repo":"b","path":"src/lib.rs","offset":64}}}}]}});
        assert!(has_row_continuation(ToolId::LocalSearch, &row));
    }

    #[test]
    fn informational_nested_partial_without_executable_next_stays_success() {
        let row = json!({"data":{"pages":[{"isPartial":true,"coverage":"partial"}]}});
        assert!(!has_row_continuation(ToolId::LocalSearch, &row));
    }

    fn outcome(
        structured_content: Value,
        failure: Option<FailureKind>,
        all_failed: bool,
    ) -> ToolOutcome {
        ToolOutcome {
            structured_content,
            content: Vec::new(),
            source_digest: None,
            failure,
            all_failed,
            tool: ToolId::LocalSearch,
        }
    }

    #[test]
    fn outcomes_classify_by_their_rows() {
        let row = |status: &str, code: &str| json!({"status":status,"data":{"errorCode":code}});
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
                ExitClass::Incomplete,
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
        ];
        for (value, failure, all_failed, expected) in cases {
            assert_eq!(
                outcome(value.clone(), failure, all_failed).exit_class(),
                expected,
                "{value}"
            );
        }
    }
}
