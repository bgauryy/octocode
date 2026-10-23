use super::*;
use crate::response::cursor::PageScope;
use crate::response::page_model::ResponsePageOptions;
use serde_json::{Value, json};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;
mod failures;
mod leads;
mod rows;

/// Both halves of the stage over one execution of `query`, with no
/// replay seed: an envelope-level contract violation is returned as
/// `Ok(Err(_))`; isolated row violations become explicit error rows and
/// mark the outcome failed.
fn finish(
    mut input: StageInput,
    query: &Value,
    context: &ExecutionContext,
) -> Result<Result<ToolOutcome, ContractValidationError>, ExecutionError> {
    let remaining = match settle(input.tool, &mut input.structured, query, &mut input.failure) {
        Ok(remaining) => remaining,
        Err(error) => return Ok(Err(error)),
    };
    respond(input, remaining, None, context)
}

/// The stage input of `structured` for `tool` with `options`.
fn input(tool: ToolId, structured: Value, options: ResponsePageOptions) -> StageInput {
    StageInput {
        tool,
        structured: Sanitized::sealed(structured),
        options,
        failure: None,
        auto_page_chars: 20_000,
        allow_auto_paging: true,
    }
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
fn stage(tool: &str, structured: Value) -> ToolOutcome {
    stage_from(tool, structured, None)
}

/// [`stage`] with the source query of row 0.
fn stage_from(tool: &str, mut structured: Value, source: Option<&Value>) -> ToolOutcome {
    let id = ToolId::from_name(tool).expect("known tool");
    super::super::continuations::finalize(
        &mut structured,
        id,
        &super::super::continuations::Sources::Rows(&[source]),
        &super::super::continuations::Scope::everything(),
    )
    .expect("valid continuations");
    finish(
        input(id, structured, ResponsePageOptions::default()),
        &json!({"path":"/tmp/a.txt","mainGoal": "test", "reasoning":"r"}),
        &context(),
    )
    .expect("stage runs")
    .expect("valid envelope")
}

fn fetch_row(index: usize) -> Value {
    json!({"index":index,"data":{"path":"a.txt","content":"one\n","totalLines":1,
            "next":{"continueWindow":{"tool":"localFetch",
                "query":{"queries":[{"path":"/tmp/a.txt","mainGoal": "test", "reasoning":"r","offset":1}]}}}}})
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
    crate::contracts::prepare_many_and_validate(tool, input.clone()).expect("suggestion validates");
    input
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
