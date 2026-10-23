//! Interpreter for the canonical content extraction and controls opcodes.
use super::{ContractValidationError, ValidationIssue, issue};
use serde_json::{Value, json};

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
                full && (matched || ranged),
                "content.extraction-mode",
                "fullContent",
                "fullContent reads the whole file; drop it to select lines.",
            );
            add(
                query.get("block") == Some(&Value::Bool(true)) && !(matched || ranged),
                "content.block-selector",
                "block",
                "block widens ranges or matchString; set one.",
            );
        } else {
            add(
                query.get("contextLines").is_some() && !matched,
                "content.context-selector",
                "contextLines",
                "contextLines requires matchString; omit it for other reads.",
            );
            add(
                query.get("minify").and_then(Value::as_str) == Some("symbols")
                    && (matched || ranged),
                "content.symbol-selector",
                "minify",
                "minify:\"symbols\" cannot accompany matchString or ranges. Read the outline, then select source lines.",
            );
            // A classified read judges raw text in line units; a match read
            // stays verbatim whatever its minify, and a selection token's
            // page keeps its unit.
            let classified = query.get("clasifyContext").is_some();
            for (field, transformed) in [
                ("minify", query.get("minify").is_some() && !matched),
                (
                    "unit",
                    query.get("unit").and_then(Value::as_str) == Some("bytes")
                        && query.get("selection").is_none(),
                ),
            ] {
                add(
                    classified && transformed,
                    "content.classified-view",
                    field,
                    "clasifyContext requires raw text in line units; omit minify, and omit unit or use unit:lines.",
                );
            }
            // Each window control conflicts with fullContent at its own path,
            // so the repair drops exactly the controls and keeps the read.
            for field in ["offset", "length", "unit"] {
                if full && query.get(field).is_some() {
                    issues.push(ValidationIssue {
                        rule_id: "content.full-controls".into(),
                        path: vec!["queries".into(), index.to_string(), field.into()],
                        message: format!(
                            "{field} windows a read; fullContent reads the whole file. Remove {field}."
                        ),
                        schema: Some(json!({"conflictsWith": ["fullContent"]})),
                        received: None,
                    });
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

#[cfg(test)]
mod tests {
    use crate::contracts::validate::validate;
    use serde_json::{Value, json};

    /// A classified read judges raw text in line units: a transformed view or
    /// a byte window is refused before any read, on both content readers.
    #[test]
    fn classified_reads_refuse_transformed_views_before_reading() {
        let refusal = "clasifyContext requires raw text in line units; omit minify, and omit unit or use unit:lines.";
        let clasify = json!({"classifiers":["Does it expire keys?"]});
        for (tool, row) in [
            ("localFetch", json!({"path":"a.ts"})),
            (
                "ghGetFileContent",
                json!({"owner":"o","repo":"r","path":"a.ts"}),
            ),
        ] {
            let with = |fields: Value| {
                let mut query = row.clone();
                query["clasifyContext"] = clasify.clone();
                query
                    .as_object_mut()
                    .expect("row")
                    .extend(fields.as_object().expect("fields").clone());
                json!({"queries":[query]})
            };
            for (fields, field) in [
                (json!({"minify":"standard"}), "minify"),
                (json!({"unit":"bytes"}), "unit"),
            ] {
                let error = validate(tool, with(fields)).expect_err("refused before reading");
                assert_eq!(error.issues[0].path, ["queries", "0", field], "{tool}");
                assert_eq!(error.issues[0].message, refusal, "{tool}");
            }
            validate(tool, with(json!({"minify":"standard","matchString":"x"})))
                .expect("a match read stays verbatim");
            validate(tool, with(json!({"unit":"lines"}))).expect("line unit");
        }
    }
}
