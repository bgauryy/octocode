//! Public response stage: the one boundary between executed rows and the
//! envelope a caller receives. Execution (dispatch, reranking inference)
//! finishes first; this stage owns the order of output-row isolation,
//! continuation shaping, text rendering, paging, and final
//! contract validation, identically for CLI JSON and MCP.

use super::engine::{FailureKind, ToolOutcome};
use super::{ExecutionContext, ExecutionError};
use crate::contracts::{self, ContractValidationError};
use crate::response::{ResponseInput, ResponsePageOptions, ResponsePager, ResponsePagerConfig};
use crate::tools::id::ToolId;
use serde_json::Value;

pub(super) struct StageInput {
    pub tool: ToolId,
    /// Sanitized, reranked `{results}` envelope.
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
    pub source_digest: Option<String>,
}

/// Turn executed rows into the public envelope. An envelope-level contract
/// violation is returned as `Ok(Err(_))`; isolated row violations become
/// explicit error rows and mark the outcome failed.
pub(super) fn finish(
    input: StageInput,
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
        source_digest,
    } = input;
    super::continuations::mark_follow_ups(&mut structured);
    // Validate the complete, sanitized rows before deriving text, error state,
    // or a pagination snapshot from them.
    match isolate_output_rows(tool.as_str(), &mut structured) {
        Ok(true) if failure.is_none() => failure = Some(FailureKind::Execution),
        Ok(_) => {}
        Err(error) => return Ok(Err(error)),
    }
    let all_failed = response_all_failed(&structured);
    // Continuations replay through validation, which restores defaults;
    // emit only the fields that change the replay.
    super::continuations::compact_continuations(&mut structured);
    // An explicitly paged text response hashes and windows the rendered
    // text, so transient telemetry must leave before rendering (rows and
    // structured scopes are handled by the pager itself).
    if options.explicit()
        && let Some(envelope) = structured.as_object_mut()
    {
        crate::response::strip_transient_telemetry(envelope);
    }
    context.check()?;
    let render = options.render_text.unwrap_or(mcp)
        || failure.is_some()
        || options.response_char_length.is_some()
        || options.response_char_offset.is_some()
        || options.response_snapshot.is_some();
    let rendered_text =
        render.then(|| super::render::render_tool(tool, &structured, &response_query, text_format));
    context.check()?;
    if allow_auto_paging {
        options.auto_paginate(rendered_text.as_deref(), &structured, auto_page_chars);
    }
    super::continuations::compact_input(tool.as_str(), &mut response_query);
    seal(
        Sealed {
            tool: tool.as_str().into(),
            structured,
            response_query,
            rendered_text,
            options,
            failure,
            all_failed,
            source_digest,
        },
        context,
    )
}

/// Clasify receipts from [`super::clasify_batch::execute`]. They carry scoped
/// nested queries and page at the evidence level (`next.clasify`): no
/// continuation filling or compaction, no text rendering, and no replaying
/// auto-pagination, which would re-run inference.
pub(super) fn finish_receipts(
    receipts: super::clasify_batch::Receipts,
    response_query: Value,
    options: ResponsePageOptions,
    context: &ExecutionContext,
) -> Result<Result<ToolOutcome, ContractValidationError>, ExecutionError> {
    let tool = ToolId::Clasify.as_str();
    let super::clasify_batch::Receipts {
        mut structured,
        source_digest,
        mut failure,
    } = receipts;
    // Page reads are follow-ups like any continuation; next.clasify keeps
    // its briefs (the marking pass leaves clasify continuations alone).
    super::continuations::mark_follow_ups(&mut structured);
    match isolate_output_rows(tool, &mut structured) {
        Ok(true) if failure.is_none() => failure = Some(FailureKind::Execution),
        Ok(_) => {}
        Err(error) => return Ok(Err(error)),
    }
    let all_failed = response_all_failed(&structured);
    context.check()?;
    seal(
        Sealed {
            tool: tool.into(),
            structured,
            response_query,
            rendered_text: None,
            options,
            failure,
            all_failed,
            source_digest,
        },
        context,
    )
}

struct Sealed {
    tool: String,
    structured: Value,
    response_query: Value,
    rendered_text: Option<String>,
    options: ResponsePageOptions,
    failure: Option<FailureKind>,
    all_failed: bool,
    source_digest: Option<String>,
}

/// Page the envelope and validate the page against the public contract.
fn seal(
    sealed: Sealed,
    context: &ExecutionContext,
) -> Result<Result<ToolOutcome, ContractValidationError>, ExecutionError> {
    let Sealed {
        tool,
        structured,
        response_query,
        rendered_text,
        options,
        failure,
        all_failed,
        source_digest,
    } = sealed;
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
    let structured_content = prepared.structured_content;
    context.check()?;
    // Page shaping crosses the public contract as well.
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
            walk_threads: None,
        }
    }

    fn stage(tool: &str, structured: Value, mcp: bool) -> ToolOutcome {
        finish(
            StageInput {
                tool: ToolId::from_name(tool).expect("known tool"),
                structured,
                response_query: json!({"path":"/tmp/a.txt","goal": "test", "reasoning":"r"}),
                options: ResponsePageOptions::default(),
                mcp,
                failure: None,
                auto_page_chars: 20_000,
                text_format: super::super::render::TextFormat::Yaml,
                allow_auto_paging: true,
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
                "query":{"path":"/tmp/a.txt","goal": "test", "reasoning":"r","debug":false,"offset":1}}}}})
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
                tool: ToolId::LocalFetch,
                structured: json!({"results":"not-an-array"}),
                response_query: json!({}),
                options: ResponsePageOptions::default(),
                mcp: true,
                failure: None,
                auto_page_chars: 20_000,
                text_format: super::super::render::TextFormat::Yaml,
                allow_auto_paging: true,
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
            "next":{"clasify":{"id":"q","goal": "test", "reasoning":"r","resources":[{"id":"r","context":{"tool":"localFetch",
                "query":{"path":"/tmp/a.txt","goal": "test", "reasoning":"r","debug":false}}}],
                "questions":[{"id":"a","type":"noul","instructions":"Does it?"}]}}}]});
        let outcome = finish_receipts(
            super::super::clasify_batch::Receipts {
                structured: receipt.clone(),
                source_digest: None,
                failure: None,
            },
            json!({"queries":[]}),
            ResponsePageOptions::default(),
            &context(),
        )
        .expect("stage runs")
        .expect("valid envelope");
        assert_eq!(outcome.structured_content["queries"], receipt["queries"]);
        assert!(outcome.content.is_empty(), "receipts are never rendered");
    }
}
