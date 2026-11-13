use super::{Captured, Input, Row, Target};
use crate::id::ToolId;
use crate::id::{error_class, error_codes::ErrorClass};
use crate::tools::source_capture;
use octocode_contract_types::ErrorCode;
use serde_json::{Value, json};

/// What cuts an image row the provider window rejects: each image question
/// re-reads its images.
const IMAGE_WINDOW_HINT: &str = "Each image question re-reads its images: ask fewer image questions per call, or crop the image.";

/// What a binary source needs when it is an image.
const IMAGE_SOURCE_HINT: &str = "source takes text; a PNG or JPEG image is judged under assets, as the clasify call in related[] does.";

/// What a `source` whose path is a glob needs: it reads one file.
const GLOB_SOURCE_HINT: &str = "A source reads one file; a glob goes in resources, one resource per file, as the clasify call in related[] does.";

/// What a fresh read larger than one response needs
/// ([`Captured::read_pages`]): the runtime that runs it pages it, which a
/// command that exits after one call refuses.
const PAGED_READ_HINT: &str = "This source is larger than one response, so its read in related[] pages: run it with storage.mode persistent, or in one running MCP server.";

/// Says on `row` that the read `captured` offers pages, when it does.
pub(super) fn disclose_paged_read(row: &mut Value, captured: &Captured) {
    if captured.read_pages {
        crate::response::result::add_hints(row, [PAGED_READ_HINT]);
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
    let stale = crate::id::error_code_of(failure) == Some(ErrorCode::StaleSnapshot);
    if matches!(row.input, Input::Models) || (stale && matches!(row.input, Input::Sections(_))) {
        return;
    }
    let acquisition = captured.is_none() && matches!(row.input, Input::Judge(Target::Source(_)));
    let too_large = matches!(
        crate::id::error_code_of(failure),
        Some(ErrorCode::ClassificationContextTooLarge | ErrorCode::ClassificationStateTooLarge)
    );
    if too_large
        && let Input::Judge(Target::Source(source)) = &row.input
        && let Some(split) = sections_call(row, source, can_resume)
    {
        crate::response::result::attach_related(failure, split);
    }
    // A binary source is no text to judge: an image is judged under assets.
    if acquisition
        && crate::id::error_code_of(failure) == Some(ErrorCode::BinaryFileUnsupported)
        && let Input::Judge(Target::Source(source)) = &row.input
        && let Some(call) = assets_call(row, source)
    {
        crate::response::result::attach_related(failure, call);
        crate::response::result::prepend_hints(failure, [IMAGE_SOURCE_HINT]);
    }
    // A localFetch source path with glob characters that names no file is a
    // glob: its files are read as resources.
    if acquisition
        && crate::id::error_code_of(failure) == Some(ErrorCode::NotFound)
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
            None => crate::response::result::attach_related(failure, call),
        }
        crate::response::result::prepend_hints(failure, [GLOB_SOURCE_HINT]);
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
        crate::response::result::attach_calls(failure, json!({"read":read}));
        if let Some(captured) = captured {
            disclose_paged_read(failure, captured);
        }
    }
    // A refused input judged nothing: its own hints say how to fix it, and
    // no judgment failed that another tool could stand in for.
    let refused = failure["errorCode"]
        .as_str()
        .is_some_and(|code| error_class(code) == ErrorClass::InvalidInput);
    if acquisition || refused {
        return;
    }
    let notice = crate::contracts::classification_notice;
    let hint = notice(if read.is_some() {
        "failedWithRead"
    } else {
        "failed"
    });
    let mut tips = vec![hint];
    if row.assets.is_some() {
        tips.push(notice("imageInspection"));
        if crate::id::error_code_of(failure) == Some(ErrorCode::ClassificationStateTooLarge) {
            tips.push(IMAGE_WINDOW_HINT);
        }
    }
    crate::response::result::prepend_hints(failure, tips);
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
    sections_over(
        questions,
        row.context.as_deref(),
        row.model.as_deref(),
        vec![(None, source.clone())],
        can_resume,
    )
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
    context: Option<&str>,
    model: Option<&str>,
    sources: Vec<(Option<String>, Value)>,
    can_resume: bool,
) -> Option<Value> {
    if !can_resume || questions.is_empty() || sources.is_empty() {
        return None;
    }
    let admitted = |key: &str, source: &Value| {
        let mut probe = serde_json::Map::new();
        probe.insert(key.to_owned(), source.clone());
        crate::contracts::validate_built_query(
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
        let key = crate::tools::source_capture::glob::free_key(&wanted, |key| {
            resources.contains_key(key)
        });
        resources.insert(key, source);
    }
    let mut call = json!({"operation":"sections","resources":resources,"questions":questions});
    if let Some(context) = context {
        call["context"] = json!(context);
    }
    if let Some(model) = model {
        call["model"] = json!(model);
    }
    Some(crate::response::result::Continuation::related(ToolId::Clasify, call).build())
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
    if let Some(context) = &row.context {
        call["context"] = json!(context);
    }
    if let Some(model) = &row.model {
        call["model"] = json!(model);
    }
    if let Some(long_context) = row.long_context {
        call["longContext"] = json!(long_context);
    }
    if let Some(assets) = &row.assets {
        call["assets"] = assets.clone();
    }
    crate::contracts::validate_built_query(ToolId::Clasify.as_str(), call.clone()).ok()?;
    Some(crate::response::result::Continuation::related(ToolId::Clasify, call).build())
}

/// The same questions over a file `source` as an image under assets, named
/// by the one-line content: the route for an image a text source refuses.
fn assets_call(row: &Row, source: &Value) -> Option<Value> {
    const KEY: &str = "image";
    let tool = ToolId::from_name(source["tool"].as_str()?)?;
    let identity: &[&str] = match tool {
        ToolId::LocalFetch => &["path"],
        ToolId::GhGetFileContent => &["owner", "repo", "path", "ref"],
        _ => return None,
    };
    let query = identity
        .iter()
        .filter_map(|field| Some(((*field).to_owned(), source["query"].get(*field)?.clone())))
        .collect::<serde_json::Map<_, _>>();
    let questions = row
        .keys
        .iter()
        .cloned()
        .zip(row.questions.iter().cloned())
        .collect::<serde_json::Map<_, _>>();
    let mut call = json!({"operation":"evaluate","content":KEY,
        "assets":{KEY:{"source":{"tool":tool.as_str(),"query":query}}},"questions":questions});
    if let Some(context) = &row.context {
        call["context"] = json!(context);
    }
    if let Some(model) = &row.model {
        call["model"] = json!(model);
    }
    Some(crate::response::result::Continuation::related(ToolId::Clasify, call).build())
}

/// Whether the same read can answer differently when repeated: not after a
/// not-found, permission or input refusal.
pub(super) fn rereadable(failure: &Value) -> bool {
    failure["errorCode"].as_str().is_none_or(|code| {
        !matches!(
            error_class(code),
            ErrorClass::NotFound | ErrorClass::Permission | ErrorClass::InvalidInput
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source_row() -> Row {
        Row {
            context: None,
            model: None,
            keys: vec![],
            questions: vec![],
            input: Input::Judge(Target::Source(
                json!({"tool":"localFetch","query":{"path":"source.rs","ranges":[{"startLine":8,"endLine":12}]}}),
            )),
            assets: None,
            long_context: None,
            details: true,
            per_item: false,
        }
    }

    #[test]
    fn failed_judgment_retains_exact_captured_read_and_provider_guidance_without_source_body() {
        let row = source_row();
        let Input::Judge(Target::Source(source)) = &row.input else {
            unreachable!()
        };
        let snapshot = "f".repeat(64);
        let capture = Captured {
            read_chars: 0,
            read_pages: false,
            state: json!({"content":"PRIVATE SOURCE BODY"}),
            read: source_capture::read_lead(
                ToolId::LocalFetch,
                &source["query"],
                Some(snapshot.as_str()),
            ),
            source: None,
            pin: None,
        };
        let mut failure =
            json!({"errorCode":"classificationProviderError","hints":["Provider guidance."]});
        add(&row, Some(&capture), &mut failure, true);
        assert_eq!(failure["next"]["read"], capture.read);
        assert_eq!(failure["next"]["read"]["kind"], "continue");
        assert_eq!(
            failure["next"]["read"]["input"]["queries"][0]["snapshot"],
            json!(snapshot)
        );
        assert!(
            failure["hints"]
                .as_array()
                .unwrap()
                .contains(&json!("Provider guidance."))
        );
        assert!(!failure.to_string().contains("PRIVATE SOURCE BODY"));
        let once = failure.clone();
        add(&row, Some(&capture), &mut failure, true);
        assert_eq!(failure, once);
    }

    #[test]
    fn acquisition_failure_offers_fresh_scoped_retry_only_when_repeat_can_succeed() {
        let row = source_row();
        let Input::Judge(Target::Source(source)) = &row.input else {
            unreachable!()
        };
        let mut failure = json!({"errorCode":"classificationContextUnavailable"});
        add(&row, None, &mut failure, true);
        assert_eq!(failure["related"][0]["kind"], "related");
        assert_eq!(
            failure["related"][0]["input"]["queries"][0],
            source["query"]
        );
        assert!(failure.get("next").is_none());
        // Too large to judge whole: no read of it, a ready call in parts.
        let mut asked = source_row();
        asked.keys = vec!["q".into()];
        asked.questions = vec![json!({"type":"noul","instructions":"Is it big?"})];
        let mut too_large = json!({"errorCode":"classificationContextTooLarge"});
        add(&asked, None, &mut too_large, true);
        assert!(too_large.get("next").is_none(), "{too_large}");
        let split = &too_large["related"][0]["input"]["queries"][0];
        assert_eq!(split["operation"], "sections", "{too_large}");
        assert_eq!(split["resources"]["source"], *source, "{too_large}");
        // A runtime that keeps nothing for a later call refuses a plan: it
        // offers none.
        let mut one_shot = json!({"errorCode":"classificationContextTooLarge"});
        add(&asked, None, &mut one_shot, false);
        assert!(one_shot.get("related").is_none(), "{one_shot}");
        for code in ["notFound", "outsideAllowedRoots"] {
            let mut failure = json!({"errorCode":code});
            add(&row, None, &mut failure, true);
            assert!(failure.get("next").is_none(), "{failure}");
            assert!(failure.get("related").is_none(), "{failure}");
        }
    }

    /// A provider window an image row exceeds is cut by asking fewer image
    /// questions per call; a text row keeps only the provider's own advice.
    #[test]
    fn an_image_row_over_the_provider_window_says_to_ask_fewer_image_questions() {
        let mut row = source_row();
        row.input = Input::Judge(Target::Content(json!("literal")));
        let failure =
            || json!({"errorCode":"classificationStateTooLarge","hints":["Provider guidance."]});
        let mut text = failure();
        add(&row, None, &mut text, true);
        assert!(!text.to_string().contains(IMAGE_WINDOW_HINT), "{text}");
        row.assets = Some(json!({"image":{}}));
        let mut image = failure();
        add(&row, None, &mut image, true);
        let hints = image["hints"].as_array().unwrap();
        assert!(hints.contains(&json!(IMAGE_WINDOW_HINT)), "{image}");
        assert!(hints.contains(&json!("Provider guidance.")), "{image}");
        let once = image.clone();
        add(&row, None, &mut image, true);
        assert_eq!(image, once);
        let mut other = json!({"errorCode":"classificationProviderError"});
        add(&row, None, &mut other, true);
        assert!(!other.to_string().contains(IMAGE_WINDOW_HINT), "{other}");
    }

    /// An input refusal judged nothing: it keeps only its own input-fix
    /// guidance, never the notices for a failed judgment or for inspecting
    /// the images some other way.
    #[test]
    fn an_input_refusal_carries_only_its_input_fix() {
        let mut row = source_row();
        row.input = Input::Judge(Target::Content(json!("literal")));
        row.assets = Some(json!({"pic":{}}));
        let notice = crate::contracts::classification_notice;
        for code in [
            "invalidClassificationContext",
            "invalidClassificationRequest",
        ] {
            let mut refused = json!({"errorCode":code,"hints":["Fix the input."]});
            add(&row, None, &mut refused, true);
            assert_eq!(refused["hints"], json!(["Fix the input."]), "{refused}");
        }
        // A judgment that failed still says so, with the image route.
        let mut failed = json!({"errorCode":"classificationProviderError"});
        add(&row, None, &mut failed, true);
        let hints = failed["hints"].as_array().unwrap();
        assert!(hints.contains(&json!(notice("failed"))), "{failed}");
        assert!(
            hints.contains(&json!(notice("imageInspection"))),
            "{failed}"
        );
    }

    /// An image given as a text source gets the same questions as a ready
    /// call with it under assets.
    #[test]
    fn a_binary_source_offers_the_same_questions_with_the_file_under_assets() {
        let mut row = source_row();
        row.keys = vec!["q".into()];
        row.questions = vec![json!({"type":"noul","instructions":"Is a popup shown?"})];
        let mut failure = json!({"errorCode":"binaryFileUnsupported","hints":["Reader advice."]});
        add(&row, None, &mut failure, true);
        let hints = failure["hints"].as_array().unwrap();
        assert_eq!(hints[0], json!(IMAGE_SOURCE_HINT), "{failure}");
        assert!(hints.contains(&json!("Reader advice.")), "{failure}");
        let call = failure["related"]
            .as_array()
            .unwrap()
            .iter()
            .find(|lead| lead["tool"] == "clasify")
            .expect("assets call");
        let query = &call["input"]["queries"][0];
        assert_eq!(
            query["assets"]["image"]["source"],
            json!({"tool":"localFetch","query":{"path":"source.rs"}}),
            "{failure}"
        );
        assert_eq!(query["content"], "image");
        assert_eq!(query["questions"]["q"], row.questions[0]);
    }

    /// Live check D4: a glob in `source` names no file; the row says globs go
    /// in `resources` and offers the same row with the glob there.
    #[test]
    fn a_glob_in_source_points_to_resources() {
        let mut row = source_row();
        row.keys = vec!["q".into()];
        row.questions = vec![json!({"type":"noul","instructions":"Is it set?"})];
        let glob = json!({"tool":"localFetch","query":{"path":"w/*.md"}});
        row.input = Input::Judge(Target::Source(glob.clone()));
        let tree = json!({"tool":"structureSearch","kind":"related",
            "input":{"queries":[{"operation":"tree","path":"w"}]}});
        let mut failure = json!({"errorCode":"notFound","error":"Path does not exist: w/*.md",
            "related":[tree]});
        add(&row, None, &mut failure, true);
        let hints = failure["hints"].as_array().cloned().unwrap_or_default();
        assert!(
            hints
                .iter()
                .any(|hint| hint.as_str().is_some_and(|hint| hint.contains("resources"))),
            "{failure:#}"
        );
        // The way to the files leads; the directory listing stays after it.
        let call = &failure["related"][0];
        assert_eq!(call["tool"], "clasify", "first: {failure:#}");
        assert_eq!(failure["related"][1], tree, "{failure:#}");
        let query = &call["input"]["queries"][0];
        assert_eq!(query["operation"], "evaluate", "{call:#}");
        assert_eq!(query["resources"], json!({"source":glob}), "{call:#}");
        // A literal missing path is no glob: nothing points to resources.
        let mut plain = json!({"errorCode":"notFound","error":"Path does not exist: source.rs"});
        add(&source_row(), None, &mut plain, true);
        assert!(!plain.to_string().contains("resources"), "{plain:#}");
    }

    #[test]
    fn literal_content_and_model_discovery_do_not_fabricate_source_reads() {
        let mut row = source_row();
        row.input = Input::Judge(Target::Content(json!("literal")));
        let mut failure = json!({"errorCode":"classificationProviderError"});
        add(&row, None, &mut failure, true);
        assert!(failure.get("next").is_none());
        assert!(failure.get("related").is_none());
        row.input = Input::Models;
        let before = failure.clone();
        add(&row, None, &mut failure, true);
        assert_eq!(failure, before);
    }
}
