use super::{Captured, Input, Row, Target};
use crate::id::ToolId;
use crate::id::{error_class, error_codes::ErrorClass};
use crate::tools::source_capture;
use serde_json::{Value, json};

/// Recovery for a failed row. A captured source failed in classification:
/// its exact read is the way back to the evidence. A source that was never
/// captured failed in its reader: that error and its own hints say what
/// happened, nothing was classified, and the requested read is offered only
/// when repeating it can succeed (not for a missing path or a refusal).
pub(super) fn add(row: &Row, captured: Option<&Captured>, failure: &mut Value) {
    if matches!(row.input, Input::Models) {
        return;
    }
    let acquisition = captured.is_none() && matches!(row.input, Input::Judge(Target::Source(_)));
    let read = captured.map(|capture| capture.read.clone()).or_else(|| {
        let Input::Judge(Target::Source(source)) = &row.input else {
            return None;
        };
        let tool = ToolId::from_name(source["tool"].as_str()?)?;
        (matches!(tool, ToolId::LocalFetch | ToolId::GhGetFileContent) && rereadable(failure))
            .then(|| source_capture::read_lead(tool, &source["query"], None))
    });
    if failure["hints"].is_null() {
        failure["hints"] = json!({});
    }
    if let Some(read) = &read {
        failure["hints"]["read"] = read.clone();
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
    let mut tips = vec![json!(hint)];
    if row.assets.is_some() {
        tips.push(json!(notice("imageInspection")));
    }
    if let Some(existing) = failure["hints"]["text"].as_array() {
        tips.extend(
            existing
                .iter()
                .filter(|item| !tips.contains(item))
                .cloned()
                .collect::<Vec<_>>(),
        );
    }
    failure["hints"]["text"] = json!(tips);
}

/// Whether the same read can answer differently when repeated: not after a
/// not-found, permission or input refusal.
fn rereadable(failure: &Value) -> bool {
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

    fn row(input: Input, assets: Option<Value>) -> Row {
        Row {
            id: "q0".into(),
            model: None,
            keys: vec![],
            questions: vec![],
            input,
            assets,
            long_context: false,
        }
    }

    #[test]
    fn captured_read_wins_and_existing_error_guidance_survives() {
        let row = row(
            Input::Judge(Target::Source(json!({"tool":"ghGetFileContent","query":{
                "owner":"o","repo":"r","path":"a.rs","ref":"main"
            }}))),
            None,
        );
        let captured = Captured {
            state: json!({"content":"PRIVATE_SOURCE_BODY"}),
            read: json!({"tool":"ghGetFileContent","query":{"queries":[{
                "owner":"o","repo":"r","path":"a.rs","ref":"immutable-commit","ranges":["8-12"]
            }]}}),
            source: None,
        };
        let mut failure = json!({"errorCode":"classificationProviderError","hints":{
            "text":["Provider repair tip."],"retry":{"tool":"clasify","query":{}}
        }});
        add(&row, Some(&captured), &mut failure);
        assert_eq!(failure["hints"]["read"], captured.read);
        assert_eq!(failure["hints"]["text"][1], "Provider repair tip.");
        assert_eq!(failure["hints"]["retry"]["tool"], "clasify");
        assert!(!failure.to_string().contains("PRIVATE_SOURCE_BODY"));
        let once = failure.clone();
        add(&row, Some(&captured), &mut failure);
        assert_eq!(failure, once);
    }

    #[test]
    fn acquisition_failure_keeps_the_requested_scope_without_claiming_a_snapshot() {
        for (tool, query) in [
            ("localFetch", json!({"path":"a.rs","ranges":["8-12"]})),
            (
                "ghGetFileContent",
                json!({"owner":"o","repo":"r","path":"a.rs","ref":"release","ranges":["8-12"]}),
            ),
        ] {
            let row = row(
                Input::Judge(Target::Source(json!({"tool":tool,"query":query}))),
                None,
            );
            let mut failure = json!({"errorCode":"classificationSourceUnavailable"});
            add(&row, None, &mut failure);
            let read = &failure["hints"]["read"];
            assert_eq!(read["tool"], tool);
            assert_eq!(read["query"]["queries"][0]["ranges"], query["ranges"]);
            assert_eq!(read["query"]["queries"][0]["ref"], query["ref"]);
            assert!(read["query"]["queries"][0].get("snapshot").is_none());
        }
    }

    #[test]
    fn a_reader_failure_is_not_worded_as_classification_and_offers_no_dead_read() {
        for code in [
            "pathNotFound",
            "outsideAllowedRoots",
            "largeSourceWindowOnly",
        ] {
            let row = row(
                Input::Judge(Target::Source(
                    json!({"tool":"localFetch","query":{"path":"nope/missing.rs","ranges":["1-5"]}}),
                )),
                None,
            );
            let mut failure = json!({"errorCode":code,"error":"missing","hints":{
                "text":["Run the same read directly to inspect it."]
            }});
            add(&row, None, &mut failure);
            assert!(failure["hints"].get("read").is_none(), "{code}: {failure}");
            assert!(
                !failure.to_string().contains("Classification failed"),
                "{code}: {failure}"
            );
            assert_eq!(failure["hints"]["text"].as_array().map(Vec::len), Some(1));
        }
    }

    #[test]
    fn image_failure_routes_to_image_tools_without_fabricating_a_text_read() {
        let row = row(
            Input::Judge(Target::State(json!({}))),
            Some(json!({"screen":{
                "type":"image","source":{"tool":"localFetch","query":{"path":"screen.png"}}
            }})),
        );
        let mut failure = json!({"errorCode":"classificationProviderError"});
        add(&row, None, &mut failure);
        assert!(failure["hints"].get("read").is_none());
        assert!(
            failure["hints"]["text"]
                .to_string()
                .contains("image-capable tool")
        );
        assert!(
            failure["hints"]["text"]
                .as_array()
                .unwrap()
                .iter()
                .all(|tip| tip.as_str().unwrap().len() <= 120)
        );
    }

    #[test]
    fn model_discovery_failure_has_no_evidence_to_inspect() {
        let row = row(Input::Models, None);
        let mut failure =
            json!({"errorCode":"classificationProviderError","hints":{"text":["Retry later."]}});
        let before = failure.clone();
        add(&row, None, &mut failure);
        assert_eq!(failure, before);
    }
}
