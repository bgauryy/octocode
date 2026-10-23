use super::{ContractValidationError, issue, query_values};

pub(super) fn validate(input: &serde_json::Value) -> Result<(), ContractValidationError> {
    let mut issues = Vec::new();
    for (index, query) in query_values(input) {
        if let Some(scope) = query.get("scope") {
            let point = |name: &str| {
                (
                    scope[name]["line"].as_u64().unwrap_or(0),
                    scope[name]["column"].as_u64().unwrap_or(0),
                )
            };
            if point("start") >= point("end") {
                issues.extend(
                    issue(
                        "ast-search.rule-shape",
                        vec!["queries".into(), index.to_string(), "scope".into()],
                        "scope start must precede its end",
                    )
                    .issues,
                );
            }
        }
        let Some(rule) = query.get("rule").filter(|rule| rule.is_object()) else {
            continue;
        };
        if let Err(message) = octocode_engine::structural::validate_rule_shape(rule) {
            issues.extend(
                issue(
                    "ast-search.rule-shape",
                    vec!["queries".into(), index.to_string(), "rule".into()],
                    message,
                )
                .issues,
            );
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
    use serde_json::json;

    #[test]
    fn structural_rule_admission_rejects_misplaced_and_matcherless_metadata() {
        let rules = json!([{"phase":"validate","opcode":"structural_rule"}]);
        for rule in [
            json!({"has":{"field":"name"}}),
            json!({"kind":"identifier","field":"name"}),
            json!({"kind":"identifier","stopBy":"end"}),
            json!({"follows":{"kind":"identifier","field":"name"}}),
            json!({"inside":{"kind":"identifier","stopBy":{"kind":"identifier","field":"name"}}}),
        ] {
            for value in [rule.clone(), json!({"rule":rule})] {
                let input = json!({"queries":[{"rule":value}]});
                let error = super::super::apply_validation_rules(&rules, &input)
                    .expect_err("same rule-shape refusal as core");
                assert_eq!(error.issues[0].path, ["queries", "0", "rule"]);
            }
        }
    }

    #[test]
    fn structural_rule_admission_leaves_grammar_checks_to_execution() {
        let rules = json!([{"phase":"validate","opcode":"structural_rule"}]);
        for rule in [
            json!({"pattern":{"context":"class C { $K = $V; }","selector":"public_field_definition"}}),
            json!({"kind":"function_declaration","has":{"kind":"identifier","field":"name"}}),
            json!({"kind":"identifier","inside":{"kind":"absent_grammar_kind","stopBy":"end"}}),
        ] {
            super::super::apply_validation_rules(&rules, &json!({"queries":[{"rule":rule}]}))
                .expect("grammar validation is the compiler's job");
        }
    }
}
