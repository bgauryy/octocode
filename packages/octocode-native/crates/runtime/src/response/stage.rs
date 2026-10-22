//! Public response stage: the one boundary between executed rows and the
//! envelope a caller receives. Execution (dispatch, selection inference)
//! finishes first; this stage owns the order of output-row isolation,
//! continuation shaping, text rendering, paging, and final
//! contract validation, identically for CLI JSON and MCP.

use super::pager::{
    PreparedResponse, ResponseError, ResponseInput, ResponsePageOptions, ResponsePager,
    ResponsePagerConfig, SavedPages,
};
use crate::contracts::{self, ContractValidationError};
use crate::execution::{ExecutionContext, ExecutionError};
use crate::id::ToolId;
use crate::response::outcome::ToolOutcome;
use crate::response::result::FailureKind;
use serde_json::Value;

pub(crate) struct StageInput {
    pub tool: ToolId,
    /// Sanitized `{results}` envelope after evidence selection.
    pub structured: Value,
    /// Normalized input; the pager builds `responsePagination.next` from it.
    pub(crate) response_query: Value,
    pub options: ResponsePageOptions,
    pub mcp: bool,
    pub failure: Option<FailureKind>,
    pub auto_page_chars: usize,
    /// Encoding of the rendered text channel (`output.format`).
    pub(crate) text_format: super::render::TextFormat,
    /// False only when saved-result storage failed and the complete result
    /// must be returned without an unusable display continuation.
    pub(crate) allow_auto_paging: bool,
}

/// The page-independent half of the stage: selection receipts, line
/// numbers, output validation (row violations isolated, marking `failure`)
/// and continuation shaping. Returns the remaining-notes the pager words.
/// A settled envelope is what a page replay stores, so its later pages are
/// neither validated nor shaped again.
pub(crate) fn settle(
    tool: ToolId,
    structured: &mut Value,
    response_query: &Value,
    failure: &mut Option<FailureKind>,
) -> Result<Vec<Vec<super::pages::Remaining>>, ContractValidationError> {
    crate::response::selection_errors::decorate(tool, structured, response_query);
    super::notices::place(structured, tool.output().rows_key());
    super::channels::publish_hints(structured, tool);
    // The notes stay with the pager for the pages a split row moves.
    let remaining = super::pages::take_remaining_notes(structured);
    // Hosts show agents this JSON: source reads carry their line numbers in
    // `content` itself, and both text encodings render from it. Numbering
    // precedes validation so an unpaged response is exactly what was checked.
    crate::response::numbered::number_read_rows(tool, structured);
    // Validate the complete, sanitized rows before deriving text, error state,
    // or a pagination snapshot from them.
    if isolate_output_rows(tool.as_str(), structured)? && failure.is_none() {
        *failure = Some(FailureKind::Execution);
    }
    super::channels::lead_continuations(structured, tool);
    Ok(remaining)
}

/// The page-dependent half of the stage over a [`settle`]d envelope: text,
/// paging and the paged shape's contract check. `keep` (a first execution's
/// replay seed) receives the settled envelope, and `pages` the text it
/// drew, only when the response windows, so a response that fits one page
/// is never copied.
pub(crate) fn respond(
    input: StageInput,
    remaining: Vec<Vec<super::pages::Remaining>>,
    pages: Option<&SavedPages>,
    keep: Option<&mut Option<Value>>,
    context: &ExecutionContext,
) -> Result<Result<ToolOutcome, ContractValidationError>, ExecutionError> {
    let StageInput {
        tool,
        mut structured,
        mut response_query,
        mut options,
        mcp,
        failure,
        auto_page_chars,
        text_format,
        allow_auto_paging,
    } = input;
    let all_failed = response_all_failed(&structured);
    let mut keep = keep;
    let mut kept = |structured: &Value| {
        if let Some(slot) = keep.take() {
            *slot = Some(structured.clone());
        }
    };
    // A scoped window pages as asked; a length alone sizes whole-row pages.
    let scoped = options.scoped();
    if scoped {
        kept(&structured);
    }
    // A text or envelope window hashes and windows serialized bytes, so
    // transient telemetry must leave before rendering (row pages keep
    // per-call facts outside their snapshot; the pager handles both).
    let stripped = scoped && !options.rows_scope();
    if stripped && let Some(envelope) = structured.as_object_mut() {
        super::pager::strip_transient_telemetry(envelope);
    }
    context.check()?;
    // Row and envelope windows carry their own JSON: only a text response
    // draws the rendered text.
    let render = options.render_text.unwrap_or(mcp) || failure.is_some() || scoped;
    let draw = |windowed: bool| {
        (render && !windowed).then(|| {
            let draw =
                || super::render::render_tool(tool, &structured, &response_query, text_format);
            match pages {
                // Only a scoped response windows, and keeps its text.
                Some(pages) if scoped => pages.text(stripped, draw),
                _ => std::sync::Arc::new(draw()),
            }
        })
    };
    let rendered_text = if allow_auto_paging && !scoped {
        let rendered = options.auto_paginate(&structured, auto_page_chars, || draw(false));
        if options.scoped() {
            kept(&structured);
        }
        rendered
    } else {
        draw(options.rows_scope() || options.structured_scope())
    };
    context.check()?;
    super::continuations::compact_input(tool.as_str(), &mut response_query);
    // Row and envelope windows reshape the rows; a text page only adds its
    // window and pagination, and an unpaged envelope leaves unchanged.
    let reshaped = options.rows_scope() || options.structured_scope();
    let pager_config = if allow_auto_paging {
        ResponsePagerConfig::default()
    } else {
        ResponsePagerConfig {
            max_rendered_bytes: usize::MAX,
        }
    };
    let prepared = ResponsePager::new(pager_config)
        .prepare(
            ResponseInput {
                tool: tool.as_str().into(),
                query: response_query,
                structured,
                rendered_text,
                is_error: all_failed,
                options,
                remaining,
            },
            &context.cancellation,
        )
        .map_err(response_failure)?;
    context.check()?;
    Ok(finish(tool, prepared, reshaped, failure, all_failed))
}

/// A later page of a saved response: cut from `pages`, the memo built from
/// its settled `envelope` on the first read, so the envelope is neither
/// copied nor re-planned per page. `input.structured` is unused.
pub(crate) fn respond_saved(
    input: StageInput,
    envelope: &Value,
    remaining: &[Vec<super::pages::Remaining>],
    pages: &SavedPages,
    context: &ExecutionContext,
) -> Result<Result<ToolOutcome, ContractValidationError>, ExecutionError> {
    let StageInput {
        tool,
        response_query,
        options,
        failure,
        text_format,
        ..
    } = input;
    let all_failed = response_all_failed(envelope);
    context.check()?;
    let mut query = response_query.clone();
    super::continuations::compact_input(tool.as_str(), &mut query);
    let reshaped = options.rows_scope() || options.structured_scope();
    let prepared = ResponsePager::new(ResponsePagerConfig::default())
        .prepare_saved(
            &ResponseInput {
                tool: tool.as_str().into(),
                query,
                structured: Value::Null,
                rendered_text: None,
                is_error: all_failed,
                options,
                remaining: remaining.to_vec(),
            },
            envelope,
            pages,
            |structured| super::render::render_tool(tool, structured, &response_query, text_format),
            &context.cancellation,
        )
        .map_err(response_failure)?;
    context.check()?;
    Ok(finish(tool, prepared, reshaped, failure, all_failed))
}

/// The paged shape's contract check, then the outcome. Page shaping crosses
/// the public contract as well; rows validated before paging and left
/// unpaged are not validated twice.
fn finish(
    tool: ToolId,
    prepared: PreparedResponse,
    reshaped: bool,
    failure: Option<FailureKind>,
    all_failed: bool,
) -> Result<ToolOutcome, ContractValidationError> {
    let mut structured_content = prepared.structured_content;
    let paged = reshaped
        || structured_content.get("responsePagination").is_some()
        || structured_content.get("responseWindow").is_some();
    if paged {
        contracts::validate_output_in_place(tool.as_str(), &mut structured_content)?;
    }
    Ok(ToolOutcome {
        structured_content,
        content: prepared.content,
        failure,
        all_failed,
    })
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
            // A clasify row failed when it carries an error and no answers.
            !queries.is_empty()
                && queries
                    .iter()
                    .all(|query| query.get("error").is_some() && query.get("answers").is_none())
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::response::pager::ResponsePageOptions;
    use serde_json::{Value, json};
    use std::time::{Duration, Instant};
    use tokio_util::sync::CancellationToken;

    /// Both halves of the stage over one execution, with no replay text or
    /// seed: an envelope-level contract violation is returned as
    /// `Ok(Err(_))`; isolated row violations become explicit error rows and
    /// mark the outcome failed.
    fn finish(
        mut input: StageInput,
        context: &ExecutionContext,
    ) -> Result<Result<ToolOutcome, ContractValidationError>, ExecutionError> {
        let remaining = match settle(
            input.tool,
            &mut input.structured,
            &input.response_query,
            &mut input.failure,
        ) {
            Ok(remaining) => remaining,
            Err(error) => return Ok(Err(error)),
        };
        respond(input, remaining, None, None, context)
    }

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
    fn stage(tool: &str, structured: Value, mcp: bool) -> ToolOutcome {
        stage_from(tool, structured, mcp, None)
    }

    /// [`stage`] with the source query of row 0.
    fn stage_from(
        tool: &str,
        mut structured: Value,
        mcp: bool,
        source: Option<&Value>,
    ) -> ToolOutcome {
        let id = ToolId::from_name(tool).expect("known tool");
        super::super::continuations::finalize(
            &mut structured,
            id,
            &super::super::continuations::Sources::Rows(&[source]),
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
            },
            &context(),
        )
        .expect("stage runs")
        .expect("valid envelope")
    }

    fn fetch_row(index: usize) -> Value {
        json!({"index":index,"data":{"path":"a.txt","content":"one\n","totalLines":1,
            "next":{"continueWindow":{"tool":"localFetch","confidence":"exact",
                "query":{"queries":[{"path":"/tmp/a.txt","mainGoal": "test", "reasoning":"r","offset":1}]}}}}})
    }

    /// The envelope is sized before any text is drawn: one past the
    /// automatic page pages by rows without rendering, and only a windowed
    /// response copies its envelope (and keeps its text) for later pages.
    #[test]
    fn automatic_row_pages_skip_the_text_render_and_only_windows_keep_a_copy() {
        let respond_with = |content: String, scope: Option<&str>| {
            let structured = json!({"results":[{"index":0,"data":{"path":"a.txt","content":content,"totalLines":1}}]});
            let pages = SavedPages::default();
            let mut kept = None;
            let outcome = respond(
                StageInput {
                    tool: ToolId::LocalFetch,
                    structured,
                    response_query: json!({"path":"/tmp/a.txt"}),
                    options: ResponsePageOptions {
                        response_length: scope.map(|_| 1_000),
                        response_scope: scope.map(Into::into),
                        ..ResponsePageOptions::default()
                    },
                    mcp: true,
                    failure: None,
                    auto_page_chars: 1_000,
                    text_format: super::super::render::TextFormat::Yaml,
                    allow_auto_paging: true,
                },
                Vec::new(),
                Some(&pages),
                Some(&mut kept),
                &context(),
            )
            .expect("stage runs")
            .expect("valid envelope");
            (outcome, pages.has_text(), kept.is_some())
        };
        let (outcome, drawn, kept) = respond_with("x".repeat(5_000), None);
        assert_eq!(
            outcome.structured_content["responsePagination"]["scope"],
            "rows"
        );
        assert_eq!(
            outcome.content[0].text,
            serde_json::to_string(&outcome.structured_content).expect("json"),
            "row pages carry their own JSON"
        );
        assert!(kept && !drawn, "a row-paged response keeps its envelope");
        let (outcome, drawn, kept) = respond_with("small".into(), None);
        assert!(
            outcome
                .structured_content
                .get("responsePagination")
                .is_none()
        );
        assert!(
            !outcome.content[0].text.starts_with('{') && !kept && !drawn,
            "a one-page response is rendered, never copied"
        );
        let (outcome, drawn, kept) = respond_with("x".repeat(5_000), Some("text"));
        assert_eq!(
            outcome.structured_content["responsePagination"]["scope"],
            "content.text"
        );
        assert!(kept && drawn, "a text window keeps its envelope and text");
    }

    /// An explicit `responseLength` alone sizes whole-row pages: both
    /// channels carry rows, never an emptied `results` beside a text
    /// `responseWindow`, and the continuation keeps the scope it pages by.
    /// Only `responseScope:"text"` windows the rendered text. A response
    /// that fits the length is one unpaged response, without a page header.
    #[test]
    fn an_explicit_length_pages_rows_and_only_the_text_scope_windows_text() {
        let rows: Vec<Value> = (0..4)
            .map(|index| {
                json!({"index":index,"data":{"path":"a.txt","content":"x".repeat(2_000),
                    "totalLines":1}})
            })
            .collect();
        let respond_with = |length: usize, scope: Option<&str>| {
            finish(
                StageInput {
                    tool: ToolId::LocalFetch,
                    structured: json!({ "results": rows }),
                    response_query: json!({ "queries": vec![json!({"path":"/tmp/a.txt"}); 4] }),
                    options: ResponsePageOptions {
                        response_length: Some(length),
                        response_scope: scope.map(Into::into),
                        ..ResponsePageOptions::default()
                    },
                    mcp: true,
                    failure: None,
                    auto_page_chars: 20_000,
                    text_format: super::super::render::TextFormat::Yaml,
                    allow_auto_paging: true,
                },
                &context(),
            )
            .expect("stage runs")
            .expect("valid envelope")
        };
        let paged = respond_with(3_000, None).structured_content;
        assert!(
            paged["results"]
                .as_array()
                .is_some_and(|rows| !rows.is_empty()),
            "{paged}"
        );
        assert!(paged.get("responseWindow").is_none(), "{paged}");
        assert_eq!(paged["responsePagination"]["scope"], "rows", "{paged}");
        let next = &paged["responsePagination"]["next"]["query"];
        assert_eq!(next["responseScope"], "rows", "{paged}");
        assert_eq!(next["responseLength"], 3_000, "{paged}");

        let text = respond_with(3_000, Some("text")).structured_content;
        assert!(text["responseWindow"].is_string(), "{text}");
        assert_eq!(text["results"], json!([]), "{text}");
        assert_eq!(
            text["responsePagination"]["next"]["query"]["responseScope"], "text",
            "{text}"
        );

        let whole = respond_with(50_000, None);
        assert!(
            whole.structured_content.get("responsePagination").is_none(),
            "{}",
            whole.structured_content
        );
        assert_eq!(
            whole.structured_content["results"].as_array().map(Vec::len),
            Some(4)
        );
        assert!(
            whole
                .content
                .iter()
                .all(|item| !item.text.contains("# Response page")),
            "one unpaged response"
        );
    }

    /// A rendered response past the 8 MiB ceiling pages (rows, else
    /// envelope windows) with an executable continuation instead of failing.
    #[test]
    fn oversized_responses_page_by_rows_instead_of_failing() {
        let content = "x".repeat(9 * 1024 * 1024);
        let structured = json!({"results":[{"index":0,"data":{"path":"a.txt","content":content,"totalLines":1}}]});
        for allow_auto_paging in [true, false] {
            let outcome = finish(
                StageInput {
                    tool: ToolId::LocalFetch,
                    structured: structured.clone(),
                    response_query: json!({"path":"/tmp/a.txt"}),
                    options: ResponsePageOptions::default(),
                    mcp: true,
                    failure: None,
                    auto_page_chars: 0,
                    text_format: super::super::render::TextFormat::Yaml,
                    allow_auto_paging,
                },
                &context(),
            )
            .expect("execution")
            .expect("contract-valid response");
            let page = &outcome.structured_content;
            if allow_auto_paging {
                // One 9 MiB string row cannot split into row pages: it windows.
                assert_eq!(page["responsePagination"]["scope"], "structuredContent");
                assert_eq!(
                    page["responsePagination"]["next"]["query"]["responseScope"],
                    "structured"
                );
                let text: usize = outcome.content.iter().map(|c| c.text.len()).sum();
                assert!(text <= 9 * 1024 * 1024, "page text is bounded: {text}");
            } else {
                assert!(page.get("responsePagination").is_none());
                assert!(
                    page["results"][0]["data"]["content"]
                        .as_str()
                        .is_some_and(|returned| returned == content),
                    "unretained responses keep the complete source content"
                );
            }
        }
    }

    /// A page that continues keeps every other lead its row carries when
    /// the row asked for `hints`; by default a successful row's optional
    /// search lead goes and its page stays.
    fn suggested_input(data: &Value, action: &str, tool: &str) -> Value {
        let prefix = format!("{action}: {tool} ");
        let text = data["hints"]
            .as_array()
            .expect("string guidance")
            .iter()
            .filter_map(Value::as_str)
            .find_map(|text| text.strip_prefix(&prefix))
            .expect("suggested tool call");
        let input: Value = serde_json::from_str(text).expect("complete JSON input");
        crate::contracts::prepare_many_and_validate(tool, input.clone())
            .expect("suggestion validates");
        input
    }

    #[test]
    fn a_continuing_row_keeps_its_search_lead() {
        let mut row = fetch_row(0);
        row["data"]["next"]["findText"] = json!({"tool":"localSearch",
            "query":{"queries":[{"path":"/tmp/a.txt","matchString":"hit"}]}});
        let hinted = json!({"path":"/tmp/a.txt","extras":["hints"]});
        for mcp in [false, true] {
            let outcome = stage_from(
                "localFetch",
                json!({"results":[row.clone()]}),
                mcp,
                Some(&hinted),
            );
            let data = &outcome.structured_content["results"][0]["data"];
            let input = suggested_input(data, "findText", "localSearch");
            assert_eq!(input["queries"][0]["matchString"], "hit");
            assert!(data["next"].get("findText").is_none());
            assert_eq!(
                data["next"]["continueWindow"]["tool"], "localFetch",
                "{data}"
            );
            let outcome = stage("localFetch", json!({"results":[row.clone()]}), mcp);
            let data = &outcome.structured_content["results"][0]["data"];
            assert!(data.get("hints").is_none(), "mcp={mcp}: {data}");
            assert_eq!(
                data["next"]["continueWindow"]["tool"], "localFetch",
                "{data}"
            );
        }
    }

    fn lsp_partial_row(paged: bool) -> Value {
        let rows: Vec<Value> = (1..=5)
            .map(|n| json!({"path":format!("/tmp/f{n}.ts"),"matchString":"\\bname\\b"}))
            .collect();
        let mut data = json!({"path":"a.ts","kind":"callers","files":[],
            "isPartial":true,"partialReasons":["importerScanFailed"],
            "next":{"findText":{"tool":"localSearch","query":{"queries":rows}}}});
        if paged {
            data["next"]["nextImporterPage"] = json!({"tool":"lspSearch","query":{"queries":[
                {"operation":"callers","path":"/tmp/a.ts","symbolName":"name","line":1,"page":2}]}});
        }
        json!({"results":[{"index":0,"data":data}]})
    }

    /// A partial row with no page has no other route to what it withholds:
    /// its lead stays in the default output with every row.
    #[test]
    fn a_partial_row_without_a_page_keeps_its_lead_whole() {
        for mcp in [false, true] {
            let outcome = stage("lspSearch", lsp_partial_row(false), mcp);
            let data = &outcome.structured_content["results"][0]["data"];
            let input = suggested_input(data, "findText", "localSearch");
            let rows = input["queries"]
                .as_array()
                .unwrap_or_else(|| panic!("mcp={mcp}: lead stays: {data}"));
            let paths: Vec<_> = rows.iter().filter_map(|row| row["path"].as_str()).collect();
            assert_eq!(
                paths,
                [
                    "/tmp/f1.ts",
                    "/tmp/f2.ts",
                    "/tmp/f3.ts",
                    "/tmp/f4.ts",
                    "/tmp/f5.ts"
                ],
                "mcp={mcp}: {data}"
            );
        }
    }

    /// A partial row whose `next` pages on reaches the rest by that page,
    /// so its optional lead is opt-in like any successful row's.
    #[test]
    fn a_partial_row_with_a_page_drops_its_optional_lead_by_default() {
        for mcp in [false, true] {
            let outcome = stage("lspSearch", lsp_partial_row(true), mcp);
            let data = &outcome.structured_content["results"][0]["data"];
            assert_eq!(
                data["next"]["nextImporterPage"]["tool"], "lspSearch",
                "mcp={mcp}: {data}"
            );
            assert!(data.get("hints").is_none(), "mcp={mcp}: {data}");
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
    fn clasify_all_failed_needs_every_row_to_fail() {
        let failed = json!({"id":"a","error":{"errorCode":"pathNotFound","error":"gone"}});
        let answered = json!({"id":"b","answers":{"q":{"type":"noul","noul":0.9}}});
        assert!(response_all_failed(&json!({"queries":[failed.clone()]})));
        assert!(response_all_failed(
            &json!({"queries":[failed.clone(), failed.clone()]})
        ));
        assert!(!response_all_failed(
            &json!({"queries":[failed, answered.clone()]})
        ));
        assert!(!response_all_failed(&json!({"queries":[answered]})));
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
        let next = &cli.structured_content["results"][0]["data"]["next"]["continueWindow"]["query"]
            ["queries"][0];
        assert_eq!(
            next,
            &json!({"path":"/tmp/a.txt","offset":1}),
            "no default is injected into a continuation"
        );
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

    /// A classified row's two unfiltered routes (the unknown evidence only,
    /// and the whole view) reach different scopes: after channel shaping
    /// they are still two separately callable pages with their rows intact.
    #[test]
    fn the_unknown_and_whole_unfiltered_routes_stay_two_calls() {
        let call = |tool: &str, row: Value| json!({"tool":tool,"query":{"queries":[row]}});
        let cases = [
            (
                "localSearch",
                json!({"path":"/tmp/src","matchString":"fn","include":["/a.rs"]}),
                json!({"path":"/tmp/src","matchString":"fn","snapshot":"s1"}),
                json!({"files":[{"path":"a.rs","matches":[{"line":1,"value":"fn a()"}]}],
                    "matchCount":1,"matchedLineCount":1,"filesScanned":2}),
            ),
            (
                "localFetch",
                json!({"path":"/tmp/a.txt","ranges":["9-20","30-31"]}),
                json!({"path":"/tmp/a.txt","fullContent":true}),
                json!({"path":"a.txt","content":"one\n","totalLines":40}),
            ),
        ];
        for (tool, unknown, whole, mut data) in cases {
            data["next"] = json!({"unfilteredUnknown": call(tool, unknown.clone()),
                "unfiltered": call(tool, whole.clone())});
            for mcp in [false, true] {
                let outcome = stage(
                    tool,
                    json!({"results":[{"index":0,"data":data.clone()}]}),
                    mcp,
                );
                let next = &outcome.structured_content["results"][0]["data"]["next"];
                assert_eq!(
                    next["unfilteredUnknown"]["query"]["queries"],
                    json!([unknown]),
                    "{tool} mcp={mcp}: {next}"
                );
                assert_eq!(
                    next["unfiltered"]["query"]["queries"],
                    json!([whole]),
                    "{tool} mcp={mcp}: {next}"
                );
            }
        }
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
            },
            &context(),
        )
        .expect("stage runs");
        assert!(result.is_err());
    }

    /// An explicitly paged text response hashes the rendered text; a
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
    fn clasify_receipts_keep_their_read_lead_executable() {
        let receipt = json!({"queries":[{"id":"q","model":"jev-1.13.0",
            "answers":{"a":{"type":"noul","noul":0.5}},
            "usage":{"input_tokens":3,"output_tokens":1},
            "hints":{"read":{"tool":"localFetch","query":{"queries":[
                {"path":"/tmp/a","ranges":["1-2"]}]}}}}]});
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
            },
            &context(),
        )
        .expect("stage runs")
        .expect("valid envelope");
        // The read of exactly what was judged survives the stage unchanged.
        let returned = &outcome.structured_content["queries"][0];
        assert_eq!(
            suggested_input(returned, "read", "localFetch"),
            receipt["queries"][0]["hints"]["read"]["query"]
        );
        for key in ["id", "model", "answers", "usage"] {
            assert_eq!(returned[key], receipt["queries"][0][key]);
        }
        assert!(
            outcome.content.is_empty(),
            "non-MCP receipts are not rendered"
        );
    }

    #[test]
    fn mcp_clasify_receipts_render_in_the_configured_text_format() {
        let receipt = json!({"queries":[{"id":"q","model":"jev-1.13.0",
            "answers":{"a":{"type":"noul","noul":0.5}},
            "usage":{"input_tokens":3,"output_tokens":1}}]});
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
