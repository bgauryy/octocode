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

/// Recovery for a failed row. A captured source failed in classification:
/// its exact read is the way back to the evidence. A source that was never
/// captured failed in its reader: that error and its own hints say what
/// happened, nothing was classified, and the requested read is offered only
/// when repeating it can succeed (not for a missing path or a refusal). A
/// selection too large to judge offers no read of itself: its hints say how
/// to judge it.
pub(super) fn add(row: &Row, captured: Option<&Captured>, failure: &mut Value) {
    if matches!(row.input, Input::Models) {
        return;
    }
    let acquisition = captured.is_none() && matches!(row.input, Input::Judge(Target::Source(_)));
    let too_large = matches!(
        crate::id::error_code_of(failure),
        Some(ErrorCode::ClassificationContextTooLarge | ErrorCode::ClassificationStateTooLarge)
    );
    if too_large
        && let Input::Judge(Target::Source(source)) = &row.input
        && let Some(split) = sections_call(row, source)
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
    }
    if acquisition {
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

/// The same questions over `source` as a ready `sections` call: where each
/// answer sits, and the way to judge a source too large for one state. A box
/// has no text to read, so it stays out.
pub(super) fn sections_call(row: &Row, source: &Value) -> Option<Value> {
    let questions = row
        .keys
        .iter()
        .zip(&row.questions)
        .filter(|(_, question)| question["type"] != "box")
        .map(|(key, question)| (key.clone(), question.clone()))
        .collect::<serde_json::Map<_, _>>();
    if questions.is_empty() {
        return None;
    }
    let mut call =
        json!({"operation":"sections","resources":{"source":source},"questions":questions});
    if let Some(context) = &row.context {
        call["context"] = json!(context);
    }
    if let Some(model) = &row.model {
        call["model"] = json!(model);
    }
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
            state: json!({"content":"PRIVATE SOURCE BODY"}),
            read: source_capture::read_lead(
                ToolId::LocalFetch,
                &source["query"],
                Some(snapshot.as_str()),
            ),
            source: None,
        };
        let mut failure =
            json!({"errorCode":"classificationProviderError","hints":["Provider guidance."]});
        add(&row, Some(&capture), &mut failure);
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
        add(&row, Some(&capture), &mut failure);
        assert_eq!(failure, once);
    }

    #[test]
    fn acquisition_failure_offers_fresh_scoped_retry_only_when_repeat_can_succeed() {
        let row = source_row();
        let Input::Judge(Target::Source(source)) = &row.input else {
            unreachable!()
        };
        let mut failure = json!({"errorCode":"classificationContextUnavailable"});
        add(&row, None, &mut failure);
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
        add(&asked, None, &mut too_large);
        assert!(too_large.get("next").is_none(), "{too_large}");
        let split = &too_large["related"][0]["input"]["queries"][0];
        assert_eq!(split["operation"], "sections", "{too_large}");
        assert_eq!(split["resources"]["source"], *source, "{too_large}");
        for code in ["pathNotFound", "outsideAllowedRoots"] {
            let mut failure = json!({"errorCode":code});
            add(&row, None, &mut failure);
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
        add(&row, None, &mut text);
        assert!(!text.to_string().contains(IMAGE_WINDOW_HINT), "{text}");
        row.assets = Some(json!({"image":{}}));
        let mut image = failure();
        add(&row, None, &mut image);
        let hints = image["hints"].as_array().unwrap();
        assert!(hints.contains(&json!(IMAGE_WINDOW_HINT)), "{image}");
        assert!(hints.contains(&json!("Provider guidance.")), "{image}");
        let once = image.clone();
        add(&row, None, &mut image);
        assert_eq!(image, once);
        let mut other = json!({"errorCode":"classificationProviderError"});
        add(&row, None, &mut other);
        assert!(!other.to_string().contains(IMAGE_WINDOW_HINT), "{other}");
    }

    /// An image given as a text source gets the same questions as a ready
    /// call with it under assets.
    #[test]
    fn a_binary_source_offers_the_same_questions_with_the_file_under_assets() {
        let mut row = source_row();
        row.keys = vec!["q".into()];
        row.questions = vec![json!({"type":"noul","instructions":"Is a popup shown?"})];
        let mut failure = json!({"errorCode":"binaryFileUnsupported","hints":["Reader advice."]});
        add(&row, None, &mut failure);
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

    #[test]
    fn literal_content_and_model_discovery_do_not_fabricate_source_reads() {
        let mut row = source_row();
        row.input = Input::Judge(Target::Content(json!("literal")));
        let mut failure = json!({"errorCode":"classificationProviderError"});
        add(&row, None, &mut failure);
        assert!(failure.get("next").is_none());
        assert!(failure.get("related").is_none());
        row.input = Input::Models;
        let before = failure.clone();
        add(&row, None, &mut failure);
        assert_eq!(failure, before);
    }
}
