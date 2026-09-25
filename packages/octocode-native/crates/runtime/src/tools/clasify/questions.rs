//! Explicit research-template expansion. Custom provider questions are unchanged.
use super::{request_error, transport::ClassificationError};
use serde_json::{Value, json};

fn templates() -> Result<&'static Value, ClassificationError> {
    crate::contracts::parsed_contract()
        .ok()
        .and_then(|contract| contract["tools"].as_array())
        .and_then(|tools| tools.iter().find(|tool| tool["name"] == "clasify"))
        .and_then(|tool| tool.get("questionTemplates"))
        .ok_or_else(|| request_error("The embedded research question templates are unavailable."))
}

pub(crate) fn version() -> Result<Value, ClassificationError> {
    Ok(templates()?["version"].clone())
}

pub(crate) fn expand(question: &Value) -> Result<Value, ClassificationError> {
    let Some(kind) = question.get("questionType") else {
        return Ok(question.clone());
    };
    let kind = kind
        .as_str()
        .ok_or_else(|| request_error("questionType must be a research check name."))?;
    if kind == "locate" {
        validate_locate(question)?;
        return Ok(question.clone());
    }
    let prompt = templates()?["questions"][kind]
        .as_str()
        .ok_or_else(|| request_error("Unknown research questionType."))?;
    let target = question["target"]
        .as_str()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| request_error("Research checks require a nonblank target."))?;
    let novelty = kind == "addsEvidence";
    let fields = question
        .as_object()
        .ok_or_else(|| request_error("Question must be an object."))?;
    if fields.keys().any(|key| {
        !(matches!(key.as_str(), "questionType" | "target") || novelty && key == "knownEvidence")
    }) {
        return Err(request_error(
            "Research checks accept questionType, target, and knownEvidence only for addsEvidence; use a custom question for instructions or criteria.",
        ));
    }
    let mut instructions = json!({"question":prompt,"target":target});
    if novelty {
        let known = question
            .get("knownEvidence")
            .filter(|value| match value {
                Value::String(text) => !text.trim().is_empty(),
                Value::Array(items) => !items.is_empty(),
                Value::Object(fields) => !fields.is_empty(),
                _ => false,
            })
            .ok_or_else(|| request_error("addsEvidence requires non-empty knownEvidence."))?;
        instructions["knownEvidence"] = known.clone();
    }
    Ok(json!({"type":"noul","instructions":instructions}))
}

pub(crate) fn is_locate(question: &Value) -> bool {
    question.get("questionType").and_then(Value::as_str) == Some("locate")
}

fn validate_locate(question: &Value) -> Result<(), ClassificationError> {
    let fields = question
        .as_object()
        .ok_or_else(|| request_error("Question must be an object."))?;
    if fields
        .keys()
        .any(|key| !matches!(key.as_str(), "questionType" | "target"))
    {
        return Err(request_error(
            "locate accepts questionType and target only.",
        ));
    }
    if question["target"]
        .as_str()
        .is_none_or(|value| value.trim().is_empty())
    {
        return Err(request_error("locate requires a nonblank target."));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_expand_without_changing_custom_questions() {
        for kind in ["contribution", "addsEvidence", "supportsClaim"] {
            let mut question = json!({"questionType":kind,"target":"shutdown timing"});
            if kind == "addsEvidence" {
                question["knownEvidence"] = json!(["onClose runs after active requests finish"]);
            }
            let expanded = expand(&question).unwrap();
            assert_eq!(expanded["type"], "noul");
            assert_eq!(expanded["instructions"]["target"], question["target"]);
            assert!(
                expanded["instructions"]["question"]
                    .as_str()
                    .unwrap()
                    .contains("content")
            );
            assert!(expanded.get("questionType").is_none());
            assert_eq!(
                expanded["instructions"]["knownEvidence"],
                question["knownEvidence"]
            );
        }
        let custom = json!({"type":"choice","instructions":{"task":"Choose"},"criteria":{"a":null,"b":"other"}});
        assert_eq!(expand(&custom).unwrap(), custom);
        let locate = json!({"questionType":"locate","target":"shutdown ordering"});
        assert_eq!(expand(&locate).unwrap(), locate);
        assert!(is_locate(&locate));
        assert_eq!(version().unwrap(), 1);
    }

    #[test]
    fn invalid_presets_do_not_fall_back_to_an_invented_question() {
        for question in [
            json!({"questionType":"unknown","target":"x"}),
            json!({"questionType":"contribution","target":" "}),
            json!({"questionType":"addsEvidence","target":"x"}),
            json!({"questionType":"addsEvidence","target":"x","knownEvidence":[]}),
            json!({"questionType":"contribution","target":"x","instructions":"override"}),
            json!({"questionType":"locate","target":" "}),
            json!({"questionType":"locate","target":"x","unknown":true}),
        ] {
            assert!(expand(&question).is_err(), "{question}");
        }
    }
}
