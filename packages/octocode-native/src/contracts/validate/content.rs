//! Interpreter for the canonical content extraction and controls opcodes.
use super::{ContractValidationError, issue};
use serde_json::Value;

pub(super) fn validate(input: &Value, extraction: bool) -> Result<(), ContractValidationError> {
    let mut issues = Vec::new();
    for (index, query) in input
        .get("queries")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        let full = query.get("fullContent") == Some(&Value::Bool(true));
        let matched = query.get("matchString").is_some();
        let ranged = query.get("startLine").is_some() || query.get("endLine").is_some();
        let mut add = |condition: bool, rule: &str, field: &str, message: &str| {
            if condition {
                issues.extend(
                    issue(
                        rule,
                        vec!["queries".into(), index.to_string(), field.into()],
                        message,
                    )
                    .issues,
                );
            }
        };
        if extraction {
            add(
                full && matched,
                "content.extraction-mode",
                "matchString",
                "Choose fullContent or matchString.",
            );
            add(
                full && ranged,
                "content.extraction-mode",
                "startLine",
                "Choose fullContent or startLine/endLine.",
            );
            add(
                matched && ranged,
                "content.extraction-mode",
                "startLine",
                "Choose matchString or startLine/endLine.",
            );
            add(
                query.get("startLine").is_some() != query.get("endLine").is_some(),
                "content.range-pair",
                if query.get("startLine").is_none() {
                    "startLine"
                } else {
                    "endLine"
                },
                "Set startLine and endLine together.",
            );
            add(
                query
                    .get("startLine")
                    .and_then(Value::as_f64)
                    .zip(query.get("endLine").and_then(Value::as_f64))
                    .is_some_and(|(start, end)| end < start),
                "content.range-order",
                "endLine",
                "Set endLine greater than or equal to startLine.",
            );
        } else {
            add(
                query.get("contextBytes").is_some()
                    && (query.get("contextLines").is_some() || !matched),
                "content.context-unit",
                "contextBytes",
                "contextBytes requires matchString and is exclusive with contextLines.",
            );
            add(
                full && ["offset", "limit", "chunkType"]
                    .iter()
                    .any(|field| query.get(field).is_some()),
                "content.full-controls",
                "fullContent",
                "Choose fullContent or chunk controls.",
            );
            add(
                query.get("minify").and_then(Value::as_str) == Some("symbols")
                    && (matched || ranged),
                "content.symbol-selector",
                "minify",
                "minify:\"symbols\" cannot accompany matchString or startLine/endLine. Read the outline, then select source lines.",
            );
            add(
                !matched
                    && (query.get("matchStringIsRegex").is_some()
                        || query.get("matchStringCaseSensitive").is_some()),
                "content.match-controls",
                "matchString",
                "Match options require matchString.",
            );
        }
    }
    if issues.is_empty() {
        Ok(())
    } else {
        Err(ContractValidationError { issues })
    }
}
