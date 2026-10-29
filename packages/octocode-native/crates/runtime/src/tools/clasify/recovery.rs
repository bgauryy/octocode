use super::{Captured, Input, Row, Target};
use crate::id::ToolId;
use crate::id::{error_class, error_codes::ErrorClass};
use crate::tools::source_capture;
use octocode_contract_types::ErrorCode;
use serde_json::{Value, json};

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
    let too_large =
        crate::id::error_code_of(failure) == Some(ErrorCode::ClassificationContextTooLarge);
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
    let mut tips = vec![json!(hint)];
    if row.assets.is_some() {
        tips.push(json!(notice("imageInspection")));
    }
    if let Some(existing) = failure["hints"].as_array() {
        tips.extend(
            existing
                .iter()
                .filter(|item| !tips.contains(item))
                .cloned()
                .collect::<Vec<_>>(),
        );
    }
    failure["hints"] = json!(tips);
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
        let capture = Captured {
            state: json!({"content":"PRIVATE SOURCE BODY"}),
            read: source_capture::read_lead(
                ToolId::LocalFetch,
                &source["query"],
                Some("frozen-snapshot"),
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
            "frozen-snapshot"
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
        for code in [
            "pathNotFound",
            "outsideAllowedRoots",
            "classificationContextTooLarge",
        ] {
            let mut failure = json!({"errorCode":code});
            add(&row, None, &mut failure);
            assert!(failure.get("next").is_none(), "{failure}");
            assert!(failure.get("related").is_none(), "{failure}");
        }
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
