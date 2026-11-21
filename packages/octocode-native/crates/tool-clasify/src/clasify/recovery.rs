use super::{Captured, Input, Row, Target};
use octocode_contract_types::ErrorCode;
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::id::{
    error_class,
    error_codes::{self, ErrorClass},
};
use octocode_tools_shared::source_capture;
use serde_json::{Value, json};

/// What cuts an image row the provider window rejects: each image question
/// re-reads its images.
const IMAGE_WINDOW_HINT: &str = "Each image question re-reads its images: ask fewer image questions per call, or crop the image.";

/// What a `source` whose path is a glob needs: it reads one file.
const GLOB_SOURCE_HINT: &str = "A source reads one file; a glob goes in resources, one resource per file, as the clasify call in related[] does.";

/// What a fresh read larger than one response needs
/// ([`Captured::read_pages`]): the runtime that runs it pages it, which a
/// command that exits after one call refuses.
const PAGED_READ_HINT: &str = "This source is larger than one response, so reading it needs paging: use storage.mode persistent, or one running MCP server.";

/// Says on `row` that the read `captured` offers pages, when it does.
pub(super) fn disclose_paged_read(row: &mut Value, captured: &Captured) {
    if captured.read_pages {
        octocode_runtime_base::response::paging::push_warnings(row, [PAGED_READ_HINT.to_owned()]);
    }
}

/// Recovery for a failed row. A captured source failed in classification:
/// its exact read is the way back to the evidence. A source that was never
/// captured failed in its reader: that error and its own hints say what
/// happened, nothing was classified, and the requested read is offered only
/// when repeating it can succeed (not for a missing path or a refusal). A
/// refused input (an `invalidInput`-class code) was never judged either: it
/// keeps only its own input-fix hints. A selection too large to judge offers
/// no read of itself: its hints say how to judge it. A plan's cursor over a
/// changed source judged nothing and failed nothing: its restart is all.
pub(super) fn add(row: &Row, captured: Option<&Captured>, failure: &mut Value, can_resume: bool) {
    let stale = octocode_runtime_base::id::error_code_of(failure) == Some(ErrorCode::StaleSnapshot);
    if matches!(row.input, Input::Models) || (stale && matches!(row.input, Input::Sections(_))) {
        return;
    }
    let acquisition = captured.is_none() && matches!(row.input, Input::Judge(Target::Source(_)));
    let too_large = matches!(
        octocode_runtime_base::id::error_code_of(failure),
        Some(ErrorCode::ClassificationContextTooLarge | ErrorCode::ClassificationStateTooLarge)
    );
    if too_large
        && let Input::Judge(Target::Source(source)) = &row.input
        && let Some(split) = sections_call(row, source, can_resume)
    {
        octocode_runtime_base::response::result::attach_related(failure, split);
    }
    // A localFetch source path with glob characters that names no file is a
    // glob: its files are read as resources.
    if acquisition
        && octocode_runtime_base::id::error_code_of(failure) == Some(ErrorCode::NotFound)
        && let Input::Judge(Target::Source(source)) = &row.input
        && source["tool"] == ToolId::LocalFetch.as_str()
        && source["query"]["path"]
            .as_str()
            .is_some_and(|path| path.contains(octocode_engine::search::glob::GLOB_META))
        && let Some(call) = rerun_resource(row, "source", source)
    {
        // The way to the files leads the reader's own recovery.
        match failure.get_mut("related").and_then(Value::as_array_mut) {
            Some(related) if !related.contains(&call) => related.insert(0, call),
            Some(_) => {}
            None => octocode_runtime_base::response::result::attach_related(failure, call),
        }
        octocode_runtime_base::response::result::prepend_hints(failure, [GLOB_SOURCE_HINT]);
    }
    let read = captured
        .map(|capture| capture.read.clone())
        .or_else(|| {
            let Input::Judge(Target::Source(source)) = &row.input else {
                return None;
            };
            let tool = ToolId::from_name(source["tool"].as_str()?)?;
            (matches!(tool, ToolId::LocalFetch | ToolId::GhGetFileContent) && rereadable(failure))
                .then(|| source_capture::read_lead(tool, &source["query"], None))
        })
        .filter(|_| !too_large);
    if let Some(read) = &read {
        octocode_runtime_base::response::result::attach_calls(failure, json!({"read":read}));
        if let Some(captured) = captured {
            disclose_paged_read(failure, captured);
        }
    }
    // A refused input judged nothing: its own hints say how to fix it, and
    // no judgment failed that another tool could stand in for.
    // The two size refusals are judged rows: their hints below name the split.
    let refused = failure["errorCode"].as_str().is_some_and(|code| {
        error_class(code) == ErrorClass::CorrectInput
            && !matches!(
                error_codes::parse(code),
                Some(
                    ErrorCode::ClassificationStateTooLarge
                        | ErrorCode::ClassificationContextTooLarge
                )
            )
    });
    if acquisition || refused {
        return;
    }
    let notice = octocode_runtime_base::contracts::classification_notice;
    let hint = notice(if read.is_some() {
        "failedWithRead"
    } else {
        "failed"
    });
    let mut tips = vec![hint];
    if row.assets.is_some() {
        tips.push(notice("imageInspection"));
        if octocode_runtime_base::id::error_code_of(failure)
            == Some(ErrorCode::ClassificationStateTooLarge)
        {
            tips.push(IMAGE_WINDOW_HINT);
        }
    }
    octocode_runtime_base::response::result::prepend_hints(failure, tips);
}

/// The same questions over `source` as a ready `sections` call: the way to
/// judge a source too large for one state. A box has no text to read, so it
/// stays out.
pub(super) fn sections_call(row: &Row, source: &Value, can_resume: bool) -> Option<Value> {
    let questions = row
        .keys
        .iter()
        .zip(&row.questions)
        .filter(|(_, question)| question["type"] != "box")
        .map(|(key, question)| (key.clone(), question.clone()))
        .collect::<serde_json::Map<_, _>>();
    sections_over(questions, row, vec![(None, source.clone())], can_resume)
}

/// `questions` over every one of `sources` as one ready `sections` call. A
/// source with a resource key keeps it when the input contract admits it as
/// a key, so the call names what the row shows; any other source (a row's
/// own `source`, or a key the contract refuses, such as a glob's
/// `<key>/<path>`) is keyed `source` when it is the only one, else
/// `source1`.. in order. A runtime that keeps nothing for a later call
/// (`can_resume` false) refuses a plan, so it offers none.
pub(super) fn sections_over(
    questions: serde_json::Map<String, Value>,
    row: &Row,
    sources: Vec<(Option<String>, Value)>,
    can_resume: bool,
) -> Option<Value> {
    if !can_resume || questions.is_empty() || sources.is_empty() {
        return None;
    }
    let admitted = |key: &str, source: &Value| {
        let mut probe = serde_json::Map::new();
        probe.insert(key.to_owned(), source.clone());
        octocode_runtime_base::contracts::validate_built_query(
            ToolId::Clasify.as_str(),
            json!({"operation":"sections","resources":probe,"questions":&questions}),
        )
        .is_ok()
    };
    let sources = sources
        .into_iter()
        .map(|(key, source)| (key.filter(|key| admitted(key, &source)), source))
        .collect::<Vec<_>>();
    let unkeyed = sources.iter().filter(|(key, _)| key.is_none()).count();
    let mut resources = serde_json::Map::new();
    let mut ordinal = 0;
    for (key, source) in sources {
        let wanted = key.unwrap_or_else(|| {
            ordinal += 1;
            if unkeyed > 1 {
                format!("source{ordinal}")
            } else {
                "source".to_owned()
            }
        });
        // Two rows may show one key for different sources: the later one
        // takes the first free `<key>-<n>`.
        let key = octocode_tools_shared::source_capture::glob::free_key(&wanted, |key| {
            resources.contains_key(key)
        });
        resources.insert(key, source);
    }
    let mut call = json!({"operation":"sections","resources":resources,"questions":questions});
    super::rows::carry_brief(row, &mut call);
    if let Some(model) = &row.model {
        call["model"] = json!(model);
    }
    Some(
        octocode_runtime_base::response::result::Continuation::related(ToolId::Clasify, call)
            .build(),
    )
}

/// `row` again over its one resource `key` (`source`): the way to the files
/// of a glob, one the call stopped before it expanded or one given as a
/// `source`, which reads one file. Offered only when the contract admits the
/// call.
pub(super) fn rerun_resource(row: &Row, key: &str, source: &Value) -> Option<Value> {
    let questions = row
        .keys
        .iter()
        .cloned()
        .zip(row.questions.iter().cloned())
        .collect::<serde_json::Map<_, _>>();
    let mut resources = serde_json::Map::new();
    resources.insert(key.to_owned(), source.clone());
    let mut call = json!({"operation":"evaluate","resources":resources,"questions":questions});
    super::rows::carry_brief(row, &mut call);
    if let Some(model) = &row.model {
        call["model"] = json!(model);
    }
    if let Some(long_context) = row.long_context {
        call["longContext"] = json!(long_context);
    }
    if let Some(assets) = &row.assets {
        call["assets"] = assets.clone();
    }
    octocode_runtime_base::contracts::validate_built_query(ToolId::Clasify.as_str(), call.clone())
        .ok()?;
    Some(
        octocode_runtime_base::response::result::Continuation::related(ToolId::Clasify, call)
            .build(),
    )
}

/// Whether the same read is worth offering again: not after a failure the
/// caller must correct or a refusal. An unservable read (a binary file) still
/// leads to the source, which the reader inspects directly.
pub(super) fn rereadable(failure: &Value) -> bool {
    failure["errorCode"].as_str().is_none_or(|code| {
        !matches!(
            error_class(code),
            ErrorClass::CorrectInput | ErrorClass::Refused
        )
    })
}

#[cfg(test)]
mod tests;
