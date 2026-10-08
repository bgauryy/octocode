//! Public response stage: the one boundary between executed rows and the
//! envelope a caller receives. Execution (dispatch, reranking inference)
//! finishes first; this stage owns the order of output-row isolation,
//! continuation shaping, text rendering, paging, and final
//! contract validation, identically for CLI JSON and MCP.

use super::pager::{
    ResponseError, ResponseInput, ResponsePageOptions, ResponsePager, ResponsePagerConfig,
};
use crate::contracts::{self, ContractValidationError};
use crate::runtime::ToolOutcome;
use crate::runtime::{ExecutionContext, ExecutionError};
use crate::tools::id::ToolId;
use crate::tools::result::FailureKind;
use serde_json::Value;

pub(crate) struct StageInput {
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
    /// False when a replay would not reproduce the page: clasify, whose
    /// replay would re-run inference (it pages at the evidence level,
    /// `next.clasify`).
    pub allow_auto_paging: bool,
    pub source_digest: Option<String>,
}

/// Turn executed rows into the public envelope. An envelope-level contract
/// violation is returned as `Ok(Err(_))`; isolated row violations become
/// explicit error rows and mark the outcome failed.
pub(crate) fn finish(
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
    // A row with pages left says so first, with each remaining count.
    super::pages::disclose_remaining_pages(&mut structured, tool);
    // A batch with partial rows says so before its first row.
    super::pages::disclose_incomplete_rows(&mut structured);
    // Hosts show agents this JSON: source reads carry their line numbers in
    // `content` itself, and both text encodings render from it. Numbering
    // precedes validation so an unpaged response is exactly what was checked.
    crate::tools::numbered::number_read_rows(tool, &mut structured);
    // Validate the complete, sanitized rows before deriving text, error state,
    // or a pagination snapshot from them.
    match isolate_output_rows(tool.as_str(), &mut structured) {
        Ok(true) if failure.is_none() => failure = Some(FailureKind::Execution),
        Ok(_) => {}
        Err(error) => return Ok(Err(error)),
    }
    let all_failed = response_all_failed(&structured);
    // An explicitly paged text response hashes and windows the rendered
    // text, so transient telemetry must leave before rendering (row pages
    // keep per-call facts outside their snapshot; the pager handles both).
    if options.explicit()
        && !options.rows_scope()
        && let Some(envelope) = structured.as_object_mut()
    {
        super::pager::strip_transient_telemetry(envelope);
    }
    context.check()?;
    let render = options.render_text.unwrap_or(mcp)
        || failure.is_some()
        || options.response_length.is_some()
        || options.response_offset.is_some()
        || options.response_snapshot.is_some();
    let rendered_text =
        render.then(|| super::render::render_tool(tool, &structured, &response_query, text_format));
    context.check()?;
    if allow_auto_paging {
        options.auto_paginate(rendered_text.as_deref(), &structured, auto_page_chars);
    }
    super::continuations::compact_input(tool.as_str(), &mut response_query);
    // Row and envelope windows reshape the rows; a text page only adds its
    // window and pagination, and an unpaged envelope leaves unchanged.
    let reshaped = options.rows_scope() || options.structured_scope();
    let prepared = ResponsePager::new(ResponsePagerConfig::default())
        .prepare(
            ResponseInput {
                tool: tool.as_str().into(),
                query: response_query,
                structured,
                rendered_text,
                is_error: all_failed,
                options,
            },
            &context.cancellation,
        )
        .map_err(response_failure)?;
    let mut structured_content = prepared.structured_content;
    context.check()?;
    // Page shaping crosses the public contract as well; rows validated
    // above and left unpaged are not validated twice.
    let paged = reshaped
        || structured_content.get("responsePagination").is_some()
        || structured_content.get("responseWindow").is_some();
    if paged
        && let Err(error) =
            contracts::validate_output_in_place(tool.as_str(), &mut structured_content)
    {
        return Ok(Err(error));
    }
    Ok(Ok(ToolOutcome {
        structured_content,
        content: prepared.content,
        source_digest,
        failure,
        all_failed,
        tool,
    }))
}

fn response_failure(error: ResponseError) -> ExecutionError {
    match error {
        ResponseError::Cancelled => ExecutionError::Cancelled,
        ResponseError::StructuredContentMustBeObject | ResponseError::Unserializable => {
            ExecutionError::WorkerFailed
        }
    }
}

/// Repair malformed result rows before either response channel is rendered.
/// Envelope violations remain fatal so pagination never snapshots invalid data.
pub(crate) fn isolate_output_rows(
    tool: &str,
    structured: &mut Value,
) -> Result<bool, ContractValidationError> {
    let Err(error) = contracts::validate_output_in_place(tool, structured) else {
        return Ok(false);
    };
    let Some(patched) = contracts::isolate_row_violations(tool, structured, &error) else {
        return Err(error);
    };
    *structured = patched;
    Ok(true)
}

pub(crate) fn response_all_failed(structured: &Value) -> bool {
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
    use crate::response::pager::ResponsePageOptions;
    use serde_json::{Value, json};
    use std::time::{Duration, Instant};
    use tokio_util::sync::CancellationToken;

    fn context() -> ExecutionContext {
        ExecutionContext {
            cancellation: CancellationToken::new(),
            deadline: Instant::now() + Duration::from_secs(30),
            walk_threads: None,
            response_window: None,
            github_credential: None,
        }
    }

    /// The engine's continuation stage, then this stage.
    fn stage(tool: &str, mut structured: Value, mcp: bool) -> ToolOutcome {
        let id = ToolId::from_name(tool).expect("known tool");
        super::super::continuations::finalize(
            &mut structured,
            id,
            &super::super::continuations::Sources::Rows(&[]),
            &super::super::continuations::Scope::everything(),
        )
        .expect("valid continuations");
        finish(
            StageInput {
                tool: id,
                structured,
                response_query: json!({"path":"/tmp/a.txt","mainGoal": "test", "reasoning":"r"}),
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
                "query":{"queries":[{"path":"/tmp/a.txt","mainGoal": "test", "reasoning":"r","debug":false,"offset":1}]}}}}})
    }

    /// M9: a rendered response past the 8 MiB ceiling pages (rows, else
    /// envelope windows) with an executable continuation instead of failing.
    #[test]
    fn oversized_responses_page_by_rows_instead_of_failing() {
        let content = "x".repeat(9 * 1024 * 1024);
        let structured = json!({"results":[{"index":0,"data":{"path":"a.txt","content":content,"totalLines":1}}]});
        let outcome = finish(
            StageInput {
                tool: ToolId::LocalFetch,
                structured,
                response_query: json!({"path":"/tmp/a.txt"}),
                options: ResponsePageOptions::default(),
                mcp: true,
                failure: None,
                auto_page_chars: 0,
                text_format: super::super::render::TextFormat::Yaml,
                allow_auto_paging: false,
                source_digest: None,
            },
            &context(),
        )
        .expect("execution")
        .expect("contract-valid page");
        let page = &outcome.structured_content;
        // One 9 MiB string row cannot split into row pages: it windows.
        assert_eq!(page["responsePagination"]["scope"], "structuredContent");
        assert_eq!(
            page["responsePagination"]["next"]["query"]["responseScope"],
            "structured"
        );
        let text: usize = outcome.content.iter().map(|c| c.text.len()).sum();
        assert!(text <= 9 * 1024 * 1024, "page text is bounded: {text}");
    }

    /// A page that continues keeps every other lead its row carries: the
    /// regex match-limit warning names next.textSearch beside next.continue.
    #[test]
    fn a_continuing_row_keeps_its_search_lead() {
        let mut row = fetch_row(0);
        row["data"]["next"]["textSearch"] = json!({"tool":"localSearch",
            "query":{"queries":[{"path":"/tmp/a.txt","matchString":"hit"}]}});
        for mcp in [false, true] {
            let outcome = stage("localFetch", json!({"results":[row.clone()]}), mcp);
            let data = &outcome.structured_content["results"][0]["data"];
            let leads = [&data["next"]["textSearch"], &data["hints"]["textSearch"]];
            assert!(
                leads.iter().any(|lead| lead["tool"] == "localSearch"),
                "mcp={mcp}: {data}"
            );
        }
    }

    #[test]
    fn pager_failures_keep_their_kind() {
        assert_eq!(
            response_failure(ResponseError::Cancelled),
            ExecutionError::Cancelled
        );
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
        let next = &cli.structured_content["results"][0]["data"]["next"]["continue"]["query"]["queries"]
            [0];
        assert!(next.get("debug").is_none(), "{next}");
        assert!(cli.content.is_empty() && !mcp.content.is_empty());
        assert!(!cli.all_failed);
    }

    /// Hosts show agents structuredContent, so source reads carry their
    /// line numbers there on both surfaces; text renders the same numbers.
    #[test]
    fn source_reads_carry_line_numbers_in_structured_content() {
        let row = json!({"index":0,"data":{"path":"a.txt","content":"one\ntwo\n","totalLines":9,
            "sourceLineRanges":[{"line":4,"endLine":5}]}});
        for mcp in [false, true] {
            let outcome = stage("localFetch", json!({"results":[row.clone()]}), mcp);
            let data = &outcome.structured_content["results"][0]["data"];
            assert_eq!(data["content"], "4\tone\n5\ttwo\n", "{data}");
            assert!(data.get("sourceLineRanges").is_none(), "{data}");
            if mcp {
                let text = serde_json::to_string(&outcome.content).expect("text");
                assert!(text.contains(r"4\tone\n5\ttwo"), "{text}");
            }
        }
        let gh = json!({"results":[{"index":0,"data":{"owner":"o","repo":"r",
            "path":"a.py","content":"x\n","totalLines":3,"commitSha":"abc",
             "sourceLineRanges":[{"line":2,"endLine":2}]}}]});
        let outcome = stage("ghGetFileContent", gh, false);
        assert_eq!(
            outcome.structured_content["results"][0]["data"]["content"],
            "2\tx\n"
        );
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

    /// B1: an explicitly paged text response hashes the rendered text; a
    /// GitHub error row's request id must not change that snapshot.
    #[test]
    fn text_pages_ignore_provider_request_ids() {
        let snapshot = |request_id: &str| {
            let rows = json!({"results":[
                {"index":0,"data":{"path":"a.txt","content":"one\n".repeat(200),"totalLines":200}},
                {"index":1,"status":"error","data":{"error":"Not found","errorCode":"notFound",
                    "httpStatus":404,"requestId":request_id,
                    "rateLimit":{"remaining":4999,"resetEpochSeconds":1_700_000_000}}}]});
            let outcome = finish(
                StageInput {
                    tool: ToolId::GhGetFileContent,
                    structured: rows,
                    response_query: json!({"queries":[{"owner":"o","repo":"r","path":"a.txt","mainGoal":"g","reasoning":"r"}]}),
                    options: ResponsePageOptions {
                        response_length: Some(300),
                        ..Default::default()
                    },
                    mcp: true,
                    failure: None,
                    auto_page_chars: 20_000,
                    text_format: super::super::render::TextFormat::Yaml,
                    allow_auto_paging: true,
                    source_digest: None,
                },
                &context(),
            )
            .expect("stage runs")
            .expect("valid envelope");
            outcome.structured_content["responsePagination"]["snapshot"].clone()
        };
        let first = snapshot("360F:376635:800998:A22B33:6ABD2D5E");
        assert!(first.is_string(), "{first}");
        assert_eq!(first, snapshot("47F4:376635:800A3C:A22BF9:6ABD2D60"));
    }

    #[test]
    fn clasify_receipts_compact_nested_queries_to_replay_equivalent_fields() {
        let receipt = json!({"queries":[{"id":"q","resources":[{"id":"r","coverage":"complete",
            "pages":[{"answers":{"a":{"yesno":0.5}}}]}],
            "next":{"clasify":{"queries":[{"id":"q","mainGoal": "test", "reasoning":"r",
                "resources":[{"id":"r","tool":"localFetch",
                    "query":{"path":"/tmp/a.txt","mainGoal": "test", "reasoning":"r","debug":false}}],
                "questions":[{"id":"a","type":"yesno","ask":"Does it?"}]}]}}}]});
        let mut structured = receipt.clone();
        super::super::continuations::finalize(
            &mut structured,
            ToolId::Clasify,
            &super::super::continuations::Sources::Matrices(&json!({"queries":[]})),
            &super::super::continuations::Scope::everything(),
        )
        .expect("valid continuations");
        let outcome = finish(
            StageInput {
                tool: ToolId::Clasify,
                structured,
                response_query: json!({"queries":[]}),
                options: ResponsePageOptions::default(),
                mcp: false,
                failure: None,
                auto_page_chars: 20_000,
                text_format: super::super::render::TextFormat::Yaml,
                allow_auto_paging: false,
                source_digest: None,
            },
            &context(),
        )
        .expect("stage runs")
        .expect("valid envelope");
        // Only the default `debug:false` goes; the replay is unchanged.
        let mut expected = receipt["queries"].clone();
        expected[0]["next"]["clasify"]["queries"][0]["resources"][0]["query"]
            .as_object_mut()
            .unwrap()
            .remove("debug");
        assert_eq!(outcome.structured_content["queries"], expected);
        assert!(
            outcome.content.is_empty(),
            "non-MCP receipts are not rendered"
        );
    }

    #[test]
    fn mcp_clasify_receipts_render_in_the_configured_text_format() {
        let receipt = json!({"queries":[{"id":"q","resources":[{"id":"r","coverage":"complete",
            "pages":[{"answers":{"a":{"yesno":0.5}}}]}]}]});
        let render = |format| {
            finish(
                StageInput {
                    tool: ToolId::Clasify,
                    structured: receipt.clone(),
                    response_query: json!({"queries":[]}),
                    options: ResponsePageOptions::default(),
                    mcp: true,
                    failure: None,
                    auto_page_chars: 20_000,
                    text_format: format,
                    allow_auto_paging: false,
                    source_digest: None,
                },
                &context(),
            )
            .expect("stage runs")
            .expect("valid envelope")
        };
        let yaml = render(super::super::render::TextFormat::Yaml);
        assert_eq!(yaml.content.len(), 1);
        let text = &yaml.content[0].text;
        assert!(text.contains("id: q"), "{text}");
        assert!(!text.trim_start().starts_with('{'), "{text}");
        assert_eq!(yaml.structured_content["queries"], receipt["queries"]);
        let json_text = render(super::super::render::TextFormat::Json);
        let parsed: Value = serde_json::from_str(&json_text.content[0].text).expect("json text");
        assert_eq!(parsed["queries"], receipt["queries"]);
    }
}
