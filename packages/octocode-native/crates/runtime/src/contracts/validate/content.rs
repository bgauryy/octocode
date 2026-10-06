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
        let ranged = query.get("ranges").is_some();
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
                ranged && (full || matched),
                "content.extraction-mode",
                "ranges",
                "Choose ranges or fullContent/matchString.",
            );
            add(
                query.get("block") == Some(&Value::Bool(true)) && !(matched || ranged),
                "content.block-selector",
                "block",
                "block widens ranges or matchString; set one.",
            );
            add(
                full && matched,
                "content.extraction-mode",
                "matchString",
                "Choose fullContent or matchString.",
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
                full && ["offset", "length", "unit"]
                    .iter()
                    .any(|field| query.get(field).is_some()),
                "content.full-controls",
                "fullContent",
                "Remove unit, offset, and length when fullContent is true.",
            );
            add(
                query.get("minify").and_then(Value::as_str) == Some("symbols")
                    && (matched || ranged),
                "content.symbol-selector",
                "minify",
                "minify:\"symbols\" cannot accompany matchString or ranges. Read the outline, then select source lines.",
            );
            add(
                !matched && (query.get("regex").is_some() || query.get("caseMode").is_some()),
                "content.match-controls",
                "matchString",
                "Match options require matchString.",
            );
        }
        if extraction {
            for (position, range) in query
                .get("ranges")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .enumerate()
            {
                let reversed = range
                    .as_str()
                    .and_then(crate::tools::line_spans::parse_span::<u64>)
                    .is_some_and(|(start, end)| end < start);
                if reversed {
                    issues.extend(
                        issue(
                            "content.range-order",
                            vec![
                                "queries".into(),
                                index.to_string(),
                                "ranges".into(),
                                position.to_string(),
                            ],
                            "Set each range as start-end with end >= start.",
                        )
                        .issues,
                    );
                }
            }
        }
    }
    if issues.is_empty() {
        Ok(())
    } else {
        Err(ContractValidationError { issues })
    }
}
