//! Public response stage: the one boundary between executed rows and the
//! envelope a caller receives. Execution (dispatch, reranking inference)
//! finishes first; this stage owns the order of output-row isolation,
//! continuation shaping, text rendering, paging, cursor stamping, and final
//! contract validation, identically for CLI JSON and MCP.

use super::engine::{FailureKind, ToolOutcome};
use super::{ExecutionContext, ExecutionError};
use crate::contracts::{self, ContractValidationError};
use crate::response::{ResponseInput, ResponsePageOptions, ResponsePager, ResponsePagerConfig};
use serde_json::Value;

pub(super) struct StageInput<'a> {
    pub tool: String,
    /// Sanitized, reranked envelope: `{results}` or clasify `{queries}`.
    pub structured: Value,
    /// Normalized input; the pager builds `responsePagination.next` from it.
    pub response_query: Value,
    pub options: ResponsePageOptions,
    pub mcp: bool,
    pub failure: Option<FailureKind>,
    pub auto_page_chars: usize,
    /// Encoding of the rendered text channel (`output.format`).
    pub text_format: super::render::TextFormat,
    /// False when a replay would not reproduce the page (reranked output).
    pub allow_auto_paging: bool,
    pub cursor_scope: &'a str,
    pub source_digests: &'a [Option<String>],
    pub source_digest: Option<String>,
}

/// Turn executed rows into the public envelope. An envelope-level contract
/// violation is returned as `Ok(Err(_))`; isolated row violations become
/// explicit error rows and mark the outcome failed.
pub(super) fn finish(
    input: StageInput<'_>,
    context: &ExecutionContext,
) -> Result<Result<ToolOutcome, ContractValidationError>, ExecutionError> {
    let StageInput {
        tool,
        mut structured,
        mut response_query,
        mut options,
        mcp,
        mut failure,
        auto_page_chars,
        text_format,
        allow_auto_paging,
        cursor_scope,
        source_digests,
        source_digest,
    } = input;
    // Validate the complete, sanitized rows before deriving text, error state,
    // or a pagination snapshot from them.
    let repaired = match isolate_output_rows(&tool, &mut structured) {
        Ok(repaired) => repaired,
        Err(error) => return Ok(Err(error)),
    };
    if repaired && failure.is_none() {
        failure = Some(FailureKind::Execution);
    }
    let all_failed = response_all_failed(&structured);
    // clasify receipts carry scoped nested queries and pages at the evidence
    // level (next.clasify): no continuation compaction, cursors, or replaying
    // auto-pagination, which would re-run inference.
    let is_clasify = tool == "clasify";
    if !is_clasify {
        // Continuations replay through validation, which restores defaults;
        // emit only the fields that change the replay.
        super::continuations::compact_continuations(&mut structured);
    }
    context.check()?;
    let render = !is_clasify
        && (options.render_text.unwrap_or(mcp)
            || failure.is_some()
            || options.response_char_length.is_some()
            || options.response_char_offset.is_some()
            || options.response_snapshot.is_some());
    let rendered_text = render
        .then(|| super::render::render_tool(&tool, &structured, &response_query, text_format));
    context.check()?;
    if !is_clasify && allow_auto_paging {
        options.auto_paginate(rendered_text.as_deref(), &structured, auto_page_chars);
    }
    if !is_clasify {
        super::continuations::compact_input(&tool, &mut response_query);
    }
    let prepared = ResponsePager::new(ResponsePagerConfig::default())
        .prepare(
            ResponseInput {
                tool: tool.clone(),
                query: response_query,
                structured,
                rendered_text,
                is_error: all_failed,
                options,
            },
            &std::sync::atomic::AtomicBool::new(context.cancellation.is_cancelled()),
        )
        .map_err(|_| ExecutionError::WorkerFailed)?;
    let mut structured_content = prepared.structured_content;
    // Cursors cover responsePagination.next too, so they are stamped last;
    // clasify receipts keep their own tool scopes.
    if !is_clasify {
        inject_cursors(&mut structured_content, cursor_scope, source_digests);
    }
    context.check()?;
    // Cursor insertion and page shaping cross the public contract as well.
    if let Err(error) = contracts::validate_output(&tool, &structured_content) {
        return Ok(Err(error));
    }
    Ok(Ok(ToolOutcome {
        structured_content,
        content: prepared.content,
        source_digest,
        failure,
        all_failed,
    }))
}

/// Repair malformed result rows before either response channel is rendered.
/// Envelope violations remain fatal so pagination never snapshots invalid data.
pub(super) fn isolate_output_rows(
    tool: &str,
    structured: &mut Value,
) -> Result<bool, ContractValidationError> {
    let Err(error) = contracts::validate_output(tool, structured) else {
        return Ok(false);
    };
    let Some(patched) = contracts::isolate_row_violations(tool, structured, &error) else {
        return Err(error);
    };
    *structured = patched;
    Ok(true)
}

pub(super) fn response_all_failed(structured: &Value) -> bool {
    if let Some(rows) = structured.get("results").and_then(Value::as_array) {
        return rows.iter().all(|row| row["status"] == "error");
    }
    structured
        .get("queries")
        .and_then(Value::as_array)
        .is_some_and(|queries| {
            !queries.is_empty()
                && queries.iter().all(|query| {
                    query["status"] == "error"
                        || query
                            .get("resources")
                            .and_then(Value::as_array)
                            .is_some_and(|resources| {
                                !resources.is_empty()
                                    && resources
                                        .iter()
                                        .all(|resource| resource["coverage"] == "error")
                            })
                })
        })
}

pub(super) fn inject_cursors(value: &mut Value, scope: &str, source_digests: &[Option<String>]) {
    inject_cursors_inner(value, scope, false, source_digests, None);
}

fn inject_cursors_inner<'a>(
    value: &mut Value,
    scope: &str,
    inside_next: bool,
    source_digests: &'a [Option<String>],
    row_source_digest: Option<&'a str>,
) {
    match value {
        Value::Array(arr) => {
            for child in arr.iter_mut() {
                inject_cursors_inner(child, scope, inside_next, source_digests, row_source_digest);
            }
        }
        Value::Object(map) => {
            let row_source_digest = map
                .get("index")
                .and_then(Value::as_u64)
                .and_then(|index| source_digests.get(index as usize))
                .and_then(Option::as_deref)
                .or(row_source_digest);
            if let (true, Some(tool_str), Some(query_val)) = (
                inside_next && !map.contains_key("cursor"),
                map.get("tool").and_then(Value::as_str),
                map.get("query").filter(|v| v.is_object()),
            ) {
                // A cursor re-encodes the whole query (~1 KB), and replaying
                // `query` ignores it, so emit one only where it adds a check the
                // query lacks: localFetch source-change detection. localSearch
                // queries carry `snapshot`, which replay already verifies.
                // `{cursor}` resume stays accepted for older callers.
                let token = (tool_str == "localFetch")
                    .then_some(row_source_digest)
                    .flatten()
                    .and_then(|digest| {
                        super::cursor::ReadCursor::create(
                            tool_str,
                            query_val.clone(),
                            digest.to_owned(),
                            scope.to_owned(),
                        )
                        .ok()
                    });
                if let Some(token) = token {
                    map.insert("cursor".into(), Value::String(token));
                }
            }
            let keys: Vec<String> = map.keys().cloned().collect();
            for key in keys {
                if let Some(child) = map.get_mut(&key) {
                    inject_cursors_inner(
                        child,
                        scope,
                        inside_next || key == "next",
                        source_digests,
                        row_source_digest,
                    );
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::response::ResponsePageOptions;
    use serde_json::{Value, json};
    use std::time::{Duration, Instant};
    use tokio_util::sync::CancellationToken;

    fn context() -> ExecutionContext {
        ExecutionContext {
            cancellation: CancellationToken::new(),
            deadline: Instant::now() + Duration::from_secs(30),
            output_bytes: 1 << 20,
        }
    }

    fn stage(tool: &str, structured: Value, mcp: bool) -> ToolOutcome {
        finish(
            StageInput {
                tool: tool.into(),
                structured,
                response_query: json!({"path":"/tmp/a.txt","reasoning":"r"}),
                options: ResponsePageOptions::default(),
                mcp,
                failure: None,
                auto_page_chars: 20_000,
                text_format: super::super::render::TextFormat::Yaml,
                allow_auto_paging: true,
                cursor_scope: "scope",
                source_digests: &[None, None],
                source_digest: None,
            },
            &context(),
        )
        .expect("stage runs")
        .expect("valid envelope")
    }

    fn fetch_row(index: usize) -> Value {
        json!({"index":index,"data":{"path":"a.txt","content":"one\n","totalLines":1,
            "next":{"continue":{"tool":"localFetch","confidence":"exact",
                "query":{"path":"/tmp/a.txt","reasoning":"r","debug":false,"offset":1}}}}})
    }

    #[test]
    fn semantic_error_flag_uses_resource_coverage_without_hiding_partial_success() {
        for (coverage, expected) in [
            (vec!["error"], true),
            (vec!["error", "error"], true),
            (vec!["error", "partial"], false),
            (vec!["error", "complete"], false),
            (vec![], false),
        ] {
            let resources = coverage
                .into_iter()
                .map(|coverage| json!({"coverage":coverage}))
                .collect::<Vec<_>>();
            assert_eq!(
                response_all_failed(&json!({"queries":[{"resources":resources}]})),
                expected
            );
        }
        assert!(!response_all_failed(&json!({"queries":[]})));
    }

    #[test]
    fn cli_and_mcp_carry_the_same_rows_and_compact_continuations() {
        let rows = json!({"results":[fetch_row(0)]});
        let cli = stage("localFetch", rows.clone(), false);
        let mcp = stage("localFetch", rows, true);
        assert_eq!(
            cli.structured_content["results"],
            mcp.structured_content["results"]
        );
        let next = &cli.structured_content["results"][0]["data"]["next"]["continue"]["query"];
        assert!(next.get("debug").is_none(), "{next}");
        assert!(cli.content.is_empty() && !mcp.content.is_empty());
        assert!(!cli.all_failed);
    }

    #[test]
    fn rejected_batch_rows_stay_explicit_errors_on_both_channels() {
        let rejected = json!({"index":1,"status":"error","data":{"error":"Check the query fields.","errorCode":"invalidInput"}});
        let rows = json!({"results":[fetch_row(0), rejected.clone()]});
        for mcp in [false, true] {
            let outcome = stage("localFetch", rows.clone(), mcp);
            assert_eq!(outcome.structured_content["results"][1]["status"], "error");
            assert!(!outcome.all_failed);
        }
    }

    #[test]
    fn a_malformed_row_becomes_an_error_row_and_marks_failure() {
        let malformed = json!({"index":0,"data":{"path":7}});
        let outcome = stage("localFetch", json!({"results":[malformed]}), true);
        assert_eq!(outcome.structured_content["results"][0]["status"], "error");
        assert!(outcome.failure.is_some());
        assert!(outcome.all_failed);
    }

    #[test]
    fn an_invalid_envelope_is_a_contract_error_not_a_repair() {
        let result = finish(
            StageInput {
                tool: "localFetch".into(),
                structured: json!({"results":"not-an-array"}),
                response_query: json!({}),
                options: ResponsePageOptions::default(),
                mcp: true,
                failure: None,
                auto_page_chars: 20_000,
                text_format: super::super::render::TextFormat::Yaml,
                allow_auto_paging: true,
                cursor_scope: "scope",
                source_digests: &[],
                source_digest: None,
            },
            &context(),
        )
        .expect("stage runs");
        assert!(result.is_err());
    }

    #[test]
    fn clasify_receipts_keep_their_nested_queries_whole() {
        let receipt = json!({"queries":[{"queryId":"q","resources":[{"resourceId":"r","coverage":"complete",
            "pages":[{"answers":{"a":{"noul":0.5}}}]}],
            "next":{"clasify":{"id":"q","reasoning":"r","resources":[{"id":"r","context":{"tool":"localFetch",
                "query":{"path":"/tmp/a.txt","reasoning":"r","debug":false}}}],
                "questions":[{"id":"a","type":"noul","instructions":"Does it?"}]}}}]});
        let outcome = stage("clasify", receipt.clone(), true);
        assert_eq!(outcome.structured_content["queries"], receipt["queries"]);
    }
}
