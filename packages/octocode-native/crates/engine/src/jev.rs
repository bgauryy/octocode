//! Transport-neutral TypeSafe Jev request construction and response handling.
//!
//! Credentials, HTTP policy, runtime availability, and pre-call gates belong to
//! the native runtime. This module owns only the reusable typed-decision wire
//! contract and provisional application semantics.

use serde_json::{json, Map, Value};
use std::fmt::{Display, Formatter};

const PROBABILITY_TOLERANCE: f64 = 0.02;
const LEAN_YES_MINIMUM: f64 = 0.55;
const GROUNDED_MINIMUM: f64 = 0.70;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JevError {
    pub code: &'static str,
    pub message: String,
}

impl JevError {
    fn request(message: impl Into<String>) -> Self {
        Self {
            code: "invalidJevRequest",
            message: message.into(),
        }
    }

    fn response(message: impl Into<String>) -> Self {
        Self {
            code: "invalidJevResponse",
            message: message.into(),
        }
    }
}

impl Display for JevError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for JevError {}

fn object<'a>(value: &'a Value, field: &str) -> Result<&'a Map<String, Value>, JevError> {
    value
        .as_object()
        .ok_or_else(|| JevError::request(format!("{field} must be an object")))
}

fn non_blank<'a>(value: &'a Value, field: &str) -> Result<&'a str, JevError> {
    value
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .ok_or_else(|| JevError::request(format!("{field} must be a non-blank string")))
}

fn question(kind: &str, instructions: &str, criteria: Value) -> Value {
    json!({
        "type": kind,
        "instructions": instructions,
        "criteria": criteria,
    })
}

fn choice_criteria(
    values: &Value,
    description_field: &str,
    none_description: &str,
) -> Result<Value, JevError> {
    let values = values
        .as_array()
        .ok_or_else(|| JevError::request("choice candidates must be an array"))?;
    let mut criteria = Map::new();
    for value in values {
        let item = object(value, "choice candidate")?;
        let id = non_blank(&item["id"], "choice candidate id")?;
        let description = non_blank(
            &item[description_field],
            &format!("choice candidate {description_field}"),
        )?;
        criteria.insert(id.to_owned(), Value::String(description.to_owned()));
    }
    criteria.insert(
        "none".to_owned(),
        Value::String(none_description.to_owned()),
    );
    Ok(Value::Object(criteria))
}

fn build_questions(route: &str, state: &Value) -> Result<Value, JevError> {
    let state = object(state, "state")?;
    let questions = match route {
        "hunch_check" => json!({
            "worth_pursuing": question(
                "noul",
                "Based solely on state.basis, is state.hunch a useful lead worth turning into competing falsifiable hypotheses?",
                json!({
                    "true": "The hunch is a useful lead to test.",
                    "false": "The supplied basis does not justify pursuing the hunch."
                }),
            ),
        }),
        "hypothesis_triage" => json!({
            "hypothesis": question(
                "choice",
                "Which supplied hypothesis best fits the observations as a provisional lead? Select none when the deck is inadequate.",
                choice_criteria(
                    &state["hypotheses"],
                    "statement",
                    "No supplied hypothesis is a useful lead.",
                )?,
            ),
            "next_check": question(
                "choice",
                "Which supplied check is expected to reduce uncertainty most effectively? Judge discrimination only; host policy handles cost.",
                choice_criteria(
                    &state["nextChecks"],
                    "action",
                    "No supplied check usefully distinguishes the hypotheses.",
                )?,
            ),
        }),
        "reflection_delta" => json!({
            "effect_on_prior_lead": question(
                "choice",
                "How does state.newEvidence affect state.priorLead relative to the frozen predictions and expected outcomes?",
                json!({
                    "strengthens": "The new evidence matches a prediction of the prior lead.",
                    "weakens": "The new evidence conflicts with a prediction of the prior lead.",
                    "neutral": "The evidence does not materially distinguish the prior lead.",
                    "ambiguous": "Its effect cannot be resolved from the supplied state.",
                    "none": "The supplied effect labels do not fit the observation."
                }),
            ),
            "updated_lead": question(
                "choice",
                "After applying only state.newEvidence, which supplied hypothesis is the best provisional lead?",
                choice_criteria(
                    &state["hypotheses"],
                    "statement",
                    "No supplied hypothesis adequately explains the updated evidence.",
                )?,
            ),
            "reframe_needed": question(
                "noul",
                "Does state.newEvidence indicate that the supplied hypothesis set is no longer an adequate framing?",
                json!({
                    "true": "The hypothesis set should be replaced or expanded.",
                    "false": "The current set remains adequate for another check."
                }),
            ),
        }),
        "decision_review" => json!({
            "proposal_viable": question(
                "noul",
                "Is state.proposal viable enough to execute given the supplied assumptions, risks, cost, and reversibility?",
                json!({
                    "true": "The proposal remains viable with its stated safeguards.",
                    "false": "A supplied risk or assumption blocks the proposal."
                }),
            ),
            "primary_risk": question(
                "choice",
                "Which supplied risk most threatens the usefulness or safety of state.proposal?",
                choice_criteria(
                    &state["risks"],
                    "description",
                    "No supplied risk materially blocks the proposal.",
                )?,
            ),
            "more_evidence_needed": question(
                "noul",
                "Should the host retrieve more evidence before executing state.proposal?",
                json!({
                    "true": "Retrieve evidence before acting.",
                    "false": "The supplied state is adequate for a provisional action."
                }),
            ),
        }),
        "disputed_inference" => json!({
            "claim_status": question(
                "choice",
                "Assess state.claim using only the supplied evidence and counterclaim.",
                json!({
                    "supported": "Evidence establishes the claim.",
                    "contradicted": "Evidence establishes that the claim is false.",
                    "insufficient": "Evidence is not enough to decide.",
                    "conflicting": "Evidence gives an unresolved contradiction."
                }),
            ),
            "decisive_basis": question(
                "choice",
                "Which supplied evidence basis most directly establishes or refutes state.claim?",
                choice_criteria(
                    &state["evidenceBases"],
                    "description",
                    "No supplied basis settles the claim.",
                )?,
            ),
        }),
        "hallucination_gate" => {
            let mut questions = Map::from_iter([
                (
                    "grounded".to_owned(),
                    question(
                        "noul",
                        "Based solely on state.evidence, is state.claim directly grounded?",
                        json!({
                            "true": "At least one evidence item directly supports the claim.",
                            "false": "No evidence item directly supports the claim."
                        }),
                    ),
                ),
                (
                    "evidence_anchor".to_owned(),
                    question(
                        "choice",
                        "Which supplied evidence item most directly grounds state.claim? Select none when no item does.",
                        choice_criteria(
                            &state["evidence"],
                            "content",
                            "No supplied evidence directly grounds the claim.",
                        )?,
                    ),
                ),
            ]);
            if state.get("claimScope").is_some() {
                questions.insert(
                    "scope_matches".to_owned(),
                    question(
                        "noul",
                        "Does state.claimScope match the scope of the supplied grounding evidence?",
                        json!({
                            "true": "Claim and evidence scopes match.",
                            "false": "The claim is broader or otherwise incompatible."
                        }),
                    ),
                );
            }
            Value::Object(questions)
        }
        _ => return Err(JevError::request(format!("unsupported Jev route: {route}"))),
    };
    Ok(questions)
}

/// Builds the documented `{model,state,questions}` TypeSafe System One request.
pub fn build_request(query: &Value, default_model: &str) -> Result<Value, JevError> {
    let query = object(query, "query")?;
    let route = non_blank(&query["route"], "route")?;
    let deliberation = object(&query["deliberation"], "deliberation")?;
    for field in [
        "observations",
        "uncertainty",
        "strongestCounter",
        "falsifier",
    ] {
        non_blank(&deliberation[field], &format!("deliberation.{field}"))?;
    }
    let mut state = object(&query["state"], "state")?.clone();
    if let Some(context) = query.get("context") {
        object(context, "context")?;
        state.insert("context".to_owned(), context.clone());
    }
    let state = Value::Object(state);
    let model = query
        .get("model")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            let value = default_model.trim();
            (!value.is_empty()).then_some(value)
        })
        .unwrap_or("jev-latest");
    Ok(Value::Object(Map::from_iter([
        ("model".to_owned(), Value::String(model.to_owned())),
        ("state".to_owned(), state.clone()),
        ("questions".to_owned(), build_questions(route, &state)?),
    ])))
}

fn probability(value: &Value, field: &str) -> Result<f64, JevError> {
    value
        .as_f64()
        .filter(|number| number.is_finite() && (0.0..=1.0).contains(number))
        .ok_or_else(|| JevError::response(format!("{field} must be between 0 and 1")))
}

fn validate_distribution(
    answer: &Map<String, Value>,
    criteria: &Map<String, Value>,
    field: &str,
) -> Result<(), JevError> {
    let probabilities = answer["probabilities"]
        .as_object()
        .ok_or_else(|| JevError::response(format!("{field}.probabilities must be an object")))?;
    if probabilities.len() != criteria.len()
        || !criteria
            .keys()
            .all(|label| probabilities.contains_key(label))
    {
        return Err(JevError::response(format!(
            "{field} probability labels differ from the request"
        )));
    }
    let mut sum = 0.0;
    for (label, value) in probabilities {
        sum += probability(value, &format!("{field}.probabilities.{label}"))?;
    }
    if (sum - 1.0).abs() > PROBABILITY_TOLERANCE {
        return Err(JevError::response(format!(
            "{field} probabilities do not sum to 1"
        )));
    }
    probability(&answer["confidence"], &format!("{field}.confidence"))?;
    Ok(())
}

/// Rejects malformed or request-incompatible System One responses.
pub fn validate_response(request: &Value, response: &Value) -> Result<(), JevError> {
    let request = object(request, "request")?;
    let response = response
        .as_object()
        .ok_or_else(|| JevError::response("response must be an object"))?;
    response["model"]
        .as_str()
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .ok_or_else(|| JevError::response("response.model must be non-blank"))?;
    let usage = response["usage"]
        .as_object()
        .ok_or_else(|| JevError::response("response.usage must be an object"))?;
    for field in ["input_tokens", "output_tokens"] {
        if usage[field].as_u64().is_none() {
            return Err(JevError::response(format!(
                "response.usage.{field} must be a non-negative integer"
            )));
        }
    }
    let questions = request["questions"]
        .as_object()
        .ok_or_else(|| JevError::request("request.questions must be an object"))?;
    let answers = response["answers"]
        .as_object()
        .ok_or_else(|| JevError::response("response.answers must be an object"))?;
    if answers.len() != questions.len()
        || !questions
            .keys()
            .all(|question| answers.contains_key(question))
    {
        return Err(JevError::response(
            "response answer IDs differ from request question IDs",
        ));
    }
    for (id, question) in questions {
        let question = object(question, &format!("questions.{id}"))?;
        let answer = answers[id]
            .as_object()
            .ok_or_else(|| JevError::response(format!("answers.{id} must be an object")))?;
        if answer["type"] != question["type"] {
            return Err(JevError::response(format!(
                "answers.{id}.type differs from the request"
            )));
        }
        match question["type"].as_str() {
            Some("noul") => {
                probability(&answer["noul"], &format!("answers.{id}.noul"))?;
            }
            Some("choice") => {
                let criteria = question["criteria"].as_object().ok_or_else(|| {
                    JevError::request(format!("questions.{id}.criteria must be an object"))
                })?;
                validate_distribution(answer, criteria, &format!("answers.{id}"))?;
                let selected = answer["choice"].as_str().ok_or_else(|| {
                    JevError::response(format!("answers.{id}.choice must be a string"))
                })?;
                if !criteria.contains_key(selected) {
                    return Err(JevError::response(format!(
                        "answers.{id}.choice is not a requested label"
                    )));
                }
                let selected_probability = answer["probabilities"][selected]
                    .as_f64()
                    .ok_or_else(|| JevError::response("selected probability is missing"))?;
                if answer["probabilities"].as_object().is_some_and(|values| {
                    values
                        .values()
                        .filter_map(Value::as_f64)
                        .any(|value| value > selected_probability + 1e-6)
                }) {
                    return Err(JevError::response(format!(
                        "answers.{id}.choice is not a highest-probability label"
                    )));
                }
            }
            Some("score") => {
                return Err(JevError::response(
                    "score answers are not used by the Jev reasoning routes",
                ));
            }
            _ => return Err(JevError::request(format!("questions.{id}.type is invalid"))),
        }
    }
    Ok(())
}

fn selected_choice<'a>(response: &'a Value, id: &str) -> Option<&'a str> {
    response.pointer(&format!("/answers/{id}/choice"))?.as_str()
}

fn selected_noul(response: &Value, id: &str) -> Option<f64> {
    response.pointer(&format!("/answers/{id}/noul"))?.as_f64()
}

fn choice_action(state: &Value, field: &str, id: &str) -> Option<String> {
    state[field].as_array()?.iter().find_map(|item| {
        (item["id"].as_str() == Some(id))
            .then(|| item["action"].as_str().map(str::to_owned))
            .flatten()
    })
}

/// Converts a valid judgment into an explicit provisional host action.
pub fn apply_response(route: &str, request: &Value, response: &Value) -> Result<Value, JevError> {
    validate_response(request, response)?;
    let state = &request["state"];
    let mut block_reasons = Vec::new();
    let next_action = match route {
        "hunch_check" => {
            if selected_noul(response, "worth_pursuing").unwrap_or(0.0) >= LEAN_YES_MINIMUM {
                "Frame competing falsifiable hypotheses before another Jev call.".to_owned()
            } else {
                "Drop the hunch and continue host evidence retrieval.".to_owned()
            }
        }
        "hypothesis_triage" => {
            let hypothesis = selected_choice(response, "hypothesis").unwrap_or("none");
            let check = selected_choice(response, "next_check").unwrap_or("none");
            if hypothesis == "none" {
                block_reasons.push("No supplied hypothesis was selected.".to_owned());
            }
            match choice_action(state, "nextChecks", check) {
                Some(action) if hypothesis != "none" => action,
                _ => {
                    block_reasons.push("No supplied discriminating check was selected.".to_owned());
                    "Replace the hypothesis deck or design a new discriminating check.".to_owned()
                }
            }
        }
        "reflection_delta" => {
            if selected_noul(response, "reframe_needed").unwrap_or(0.0) >= LEAN_YES_MINIMUM {
                "Replace or expand the hypothesis deck before another check.".to_owned()
            } else {
                match selected_choice(response, "updated_lead") {
                    Some("none") | None => {
                        block_reasons.push("No updated hypothesis lead was selected.".to_owned());
                        "Abandon the current lead and replace the hypothesis deck.".to_owned()
                    }
                    Some(lead) => format!("Carry {lead} only as the updated provisional lead."),
                }
            }
        }
        "decision_review" => {
            let viable =
                selected_noul(response, "proposal_viable").unwrap_or(0.0) >= LEAN_YES_MINIMUM;
            let retrieve =
                selected_noul(response, "more_evidence_needed").unwrap_or(0.0) >= LEAN_YES_MINIMUM;
            if retrieve {
                "Retrieve evidence that resolves the selected risk or assumption.".to_owned()
            } else if !viable {
                block_reasons.push("The proposal was not judged viable.".to_owned());
                "Stop the proposal and redesign it before execution.".to_owned()
            } else {
                match selected_choice(response, "primary_risk") {
                    Some("none") | None => {
                        "Proceed only within the supplied evidence and safeguards.".to_owned()
                    }
                    Some(risk) => format!("Mitigate supplied risk {risk} before execution."),
                }
            }
        }
        "disputed_inference" => {
            let status = selected_choice(response, "claim_status").unwrap_or("insufficient");
            let basis = selected_choice(response, "decisive_basis").unwrap_or("none");
            if matches!(status, "supported" | "contradicted") && basis != "none" {
                format!("Reopen every source in evidence basis {basis} before citation.")
            } else {
                block_reasons.push(format!(
                    "The bounded claim is {status} or lacks a decisive supplied basis."
                ));
                "Narrow the claim or retrieve evidence before reconsidering it.".to_owned()
            }
        }
        "hallucination_gate" => {
            let grounded = selected_noul(response, "grounded").unwrap_or(0.0) >= GROUNDED_MINIMUM;
            let anchor = selected_choice(response, "evidence_anchor").unwrap_or("none");
            let scope_matches = response.pointer("/answers/scope_matches").is_none()
                || selected_noul(response, "scope_matches").unwrap_or(0.0) >= GROUNDED_MINIMUM;
            if grounded && scope_matches && anchor != "none" {
                format!("Reopen and cite evidence anchor {anchor} before assertion.")
            } else {
                block_reasons.push(
                    "The claim is not directly grounded within the declared evidence scope."
                        .to_owned(),
                );
                "Block or narrow the assertion until direct grounding exists.".to_owned()
            }
        }
        _ => return Err(JevError::request(format!("unsupported Jev route: {route}"))),
    };
    Ok(json!({
        "answers": response["answers"],
        "blocked": !block_reasons.is_empty(),
        "blockReasons": block_reasons,
        "nextAction": next_action,
        "provisional": true,
    }))
}

#[cfg(test)]
mod tests {
    use super::{apply_response, build_request, validate_response};
    use serde_json::json;

    fn hunch_query() -> serde_json::Value {
        json!({
            "route": "hunch_check",
            "state": {
                "goal": "Decide whether to expand the lead.",
                "hunch": "The adapter owns the behavior.",
                "basis": "One source anchor suggests a rewrite."
            },
            "deliberation": {
                "observations": "The runtime emits one value.",
                "uncertainty": "Whether the adapter rewrites it.",
                "strongestCounter": "The runtime may own the final value.",
                "falsifier": "A focused adapter test observes no rewrite."
            }
        })
    }

    #[test]
    fn builds_minimal_state_and_omits_optional_context() {
        let request = build_request(&hunch_query(), "jev-1.13.0").expect("request");
        assert_eq!(request["model"], "jev-1.13.0");
        assert_eq!(request["questions"]["worth_pursuing"]["type"], "noul");
        assert!(request["state"].get("reasoning").is_none());
        assert!(request.get("context").is_none());
        assert!(request.get("route").is_none());
    }

    #[test]
    fn nests_optional_shareable_context_in_documented_provider_state() {
        let mut query = hunch_query();
        query["context"] = json!({
            "cot": "Observed runtime output, considered an adapter rewrite, and identified a focused falsifier.",
            "thinking": "The runtime explanation is the current provisional lead.",
            "context": { "task": "Choose the next evidence step." },
            "agentRole": "research host"
        });
        let request = build_request(&query, "jev-latest").expect("request");
        assert_eq!(request["state"]["context"], query["context"]);
        assert!(request.get("context").is_none());
        assert!(request["state"].get("reasoning").is_none());
    }

    #[test]
    fn validates_and_applies_a_provisional_hunch_judgment() {
        let request = build_request(&hunch_query(), "jev-latest").expect("request");
        let response = json!({
            "model": "jev-1.13.0",
            "answers": {
                "worth_pursuing": { "type": "noul", "noul": 0.8 }
            },
            "usage": { "input_tokens": 10, "output_tokens": 2 }
        });
        validate_response(&request, &response).expect("response");
        let applied = apply_response("hunch_check", &request, &response).expect("application");
        assert_eq!(applied["provisional"], true);
        assert_eq!(applied["blocked"], false);
        assert!(applied["nextAction"]
            .as_str()
            .is_some_and(|value| value.contains("falsifiable")));
    }

    #[test]
    fn rejects_answer_id_drift() {
        let request = build_request(&hunch_query(), "jev-latest").expect("request");
        let response = json!({
            "model": "jev-1.13.0",
            "answers": { "other": { "type": "noul", "noul": 0.8 } },
            "usage": { "input_tokens": 10, "output_tokens": 2 }
        });
        assert!(validate_response(&request, &response).is_err());
    }
}
