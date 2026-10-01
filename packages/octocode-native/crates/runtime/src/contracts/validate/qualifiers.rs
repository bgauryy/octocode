//! Interpreter for the canonical `qualifier_fields` opcode: a GitHub search
//! `qualifiers` key may not repeat a typed field the same row sets.
use super::{ContractValidationError, issue};
use serde_json::Value;

/// Split `a:b label:"good first issue"` into terms; quotes group words.
pub(crate) fn qualifier_terms(text: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for c in text.chars() {
        match c {
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => {
                if !current.is_empty() {
                    terms.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        terms.push(current);
    }
    terms
}

/// `args.fields`: `[{key, field, values?}]` from the core catalog.
pub(super) fn validate(
    input: &Value,
    rule_id: &str,
    args: &Value,
) -> Result<(), ContractValidationError> {
    let fields = args["fields"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    let mut issues = Vec::new();
    for (index, query) in input["queries"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        let Some(text) = query.get("qualifiers").and_then(Value::as_str) else {
            continue;
        };
        for term in qualifier_terms(text) {
            let term = term.strip_prefix('-').unwrap_or(&term);
            let Some((key, value)) = term.split_once(':').filter(|(key, _)| !key.is_empty()) else {
                continue;
            };
            let repeated = fields.iter().find(|entry| {
                entry["key"] == key
                    && entry["values"]
                        .as_array()
                        .is_none_or(|values| values.iter().any(|v| v == value))
                    && entry["field"]
                        .as_str()
                        .is_some_and(|field| query.get(field).is_some())
            });
            if let Some(field) = repeated.and_then(|entry| entry["field"].as_str()) {
                issues.extend(
                    issue(
                        rule_id,
                        vec!["queries".into(), index.to_string(), "qualifiers".into()],
                        format!("qualifiers: {key}: repeats the {field} field; set it once."),
                    )
                    .issues,
                );
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
    use super::*;
    use serde_json::json;

    fn fields() -> Value {
        json!({"fields": [
            {"key": "created", "field": "created"},
            {"key": "is", "field": "state", "values": ["open", "closed"]},
            {"key": "is", "field": "draft", "values": ["draft"]},
        ]})
    }

    fn check(query: Value) -> Vec<String> {
        validate(&json!({"queries": [query]}), "rule", &fields())
            .err()
            .map(|error| {
                error
                    .issues
                    .into_iter()
                    .map(|issue| issue.message)
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn quotes_group_words_into_one_term() {
        assert_eq!(
            qualifier_terms(r#"label:"good first issue"  is:open"#),
            ["label:good first issue", "is:open"]
        );
    }

    #[test]
    fn a_qualifier_repeating_a_set_field_is_rejected() {
        assert_eq!(
            check(json!({"created": ">2020", "qualifiers": "created:<2019"})),
            ["qualifiers: created: repeats the created field; set it once."]
        );
        assert_eq!(
            check(json!({"draft": true, "qualifiers": "-is:draft"})),
            ["qualifiers: is: repeats the draft field; set it once."]
        );
    }

    #[test]
    fn a_qualifier_for_another_field_or_value_is_accepted() {
        assert!(check(json!({"state": "open", "qualifiers": "is:draft created:>1"})).is_empty());
        assert!(check(json!({"qualifiers": "created:<2019 is:closed"})).is_empty());
    }
}
