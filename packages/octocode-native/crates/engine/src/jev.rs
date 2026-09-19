//! Transport-neutral TypeSafe Jev request construction and response handling.
//!
//! Credentials, HTTP policy, runtime availability, and pre-call gates belong to
//! the native runtime. This module owns only the reusable typed-decision wire
//! contract and provisional application semantics.

use serde_json::{json, Map, Value};
use std::fmt::{Display, Formatter};

const PROBABILITY_TOLERANCE: f64 = 0.02;
// Noul decision bands, aligned with the JS reference host policy
// (skills/octocode-jev-reasoning-loop/assets/default-policy.json). A noul in the
// open ambiguous band (LEAN_NO_MAXIMUM, LEAN_YES_MINIMUM) is neither a lean-yes nor
// a lean-no, so it blocks for more evidence instead of forcing a decision.
const LEAN_YES_MINIMUM: f64 = 0.60;
const LEAN_NO_MAXIMUM: f64 = 0.40;
const GROUNDED_MINIMUM: f64 = 0.50;
// A selected-minus-runner-up choice gap below SOFT_TIE_GAP is a soft tie: it blocks
// for more evidence unless a route-specific preference resolves it.
const SOFT_TIE_GAP: f64 = 0.15;

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

fn noul_is_ambiguous(value: f64) -> bool {
    value > LEAN_NO_MAXIMUM && value < LEAN_YES_MINIMUM
}

/// True when a `next_check` soft tie is resolvable by a route-specific preference:
/// at least two of the near-tied labels name a supplied check in `state.nextChecks`
/// (mirrors the JS reference `close.length > 1` guard, which suppresses the block).
fn next_check_has_cost_preference(
    state: &Value,
    probabilities: &Map<String, Value>,
    selected_probability: f64,
) -> bool {
    let Some(checks) = state.get("nextChecks").and_then(Value::as_array) else {
        return false;
    };
    let close = probabilities
        .iter()
        .filter(|(_, value)| {
            value
                .as_f64()
                .is_some_and(|value| selected_probability - value < SOFT_TIE_GAP)
        })
        .filter(|(label, _)| {
            checks
                .iter()
                .any(|check| check.get("id").and_then(Value::as_str) == Some(label.as_str()))
        })
        .count();
    close > 1
}

/// Cross-cutting host block policy, ported faithfully from the JS reference
/// `applyResponse` (skills/octocode-jev-reasoning-loop/scripts/decision-contract.mjs):
/// an ambiguous noul, a grounding noul below the minimum, a choice that selected
/// `none`, or an unresolved soft-tie choice each blocks the provisional decision.
fn policy_block_reasons(request: &Value, response: &Value) -> Vec<String> {
    let mut reasons = Vec::new();
    let (Some(questions), Some(answers)) = (
        request["questions"].as_object(),
        response["answers"].as_object(),
    ) else {
        return reasons;
    };
    let state = &request["state"];
    for (id, question) in questions {
        let Some(answer) = answers.get(id) else {
            continue;
        };
        match question["type"].as_str() {
            Some("choice") => {
                let selected = answer["choice"].as_str().unwrap_or("");
                if selected == "none" {
                    reasons.push(format!(
                        "{id} selected none; follow the route-specific reframe protocol."
                    ));
                    continue;
                }
                let Some(probabilities) = answer["probabilities"].as_object() else {
                    continue;
                };
                let selected_probability = probabilities
                    .get(selected)
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0);
                let runner_up = probabilities
                    .iter()
                    .filter(|(label, _)| label.as_str() != selected)
                    .filter_map(|(_, value)| value.as_f64())
                    .fold(f64::NEG_INFINITY, f64::max);
                let runner_up = if runner_up.is_finite() {
                    runner_up
                } else {
                    selected_probability
                };
                let gap = (selected_probability - runner_up).max(0.0);
                if gap < SOFT_TIE_GAP
                    && !(id == "next_check"
                        && next_check_has_cost_preference(
                            state,
                            probabilities,
                            selected_probability,
                        ))
                {
                    reasons.push(format!(
                        "{id} is a soft tie; widen evidence before commitment."
                    ));
                }
            }
            Some("noul") => {
                let value = answer["noul"].as_f64().unwrap_or(0.0);
                if noul_is_ambiguous(value) {
                    reasons.push(format!(
                        "{id} is ambiguous; retrieve evidence instead of forcing a decision."
                    ));
                }
                if id == "grounded" && value < GROUNDED_MINIMUM {
                    reasons.push(format!(
                        "grounded is below policy minimum {GROUNDED_MINIMUM}."
                    ));
                }
                if id == "scope_matches" && value < GROUNDED_MINIMUM {
                    reasons.push(
                        "scope_matches is below policy minimum; narrow the claim.".to_owned(),
                    );
                }
            }
            _ => {}
        }
    }
    reasons
}

/// Converts a valid judgment into an explicit provisional host action. The
/// `nextAction` is route-specific; `blocked`/`blockReasons` come from the shared
/// [`policy_block_reasons`] pass so native decisions match the JS reference runner.
pub fn apply_response(route: &str, request: &Value, response: &Value) -> Result<Value, JevError> {
    validate_response(request, response)?;
    let state = &request["state"];
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
            match choice_action(state, "nextChecks", check) {
                Some(action) if hypothesis != "none" => action,
                _ => "Replace the hypothesis deck or design a new discriminating check.".to_owned(),
            }
        }
        "reflection_delta" => {
            if selected_noul(response, "reframe_needed").unwrap_or(0.0) >= LEAN_YES_MINIMUM {
                "Replace or expand the hypothesis deck before another check.".to_owned()
            } else {
                match selected_choice(response, "updated_lead") {
                    Some("none") | None => {
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
                "Block or narrow the assertion until direct grounding exists.".to_owned()
            }
        }
        _ => return Err(JevError::request(format!("unsupported Jev route: {route}"))),
    };
    let block_reasons = policy_block_reasons(request, response);
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
    use serde_json::{json, Value};

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

    // ---- host block policy parity with the JS reference runner ----------------

    fn delib() -> serde_json::Value {
        json!({
            "observations": "One anchored observation supports the lead.",
            "uncertainty": "Whether to expand the lead.",
            "strongestCounter": "Another explanation may own the behavior.",
            "falsifier": "A focused test would separate the explanations."
        })
    }

    fn choice(selected: &str, probabilities: serde_json::Value) -> serde_json::Value {
        json!({ "type": "choice", "choice": selected, "probabilities": probabilities, "confidence": 0.8 })
    }

    fn apply(route: &str, request: &Value, answers: serde_json::Value) -> Value {
        let response = json!({
            "model": "jev-1.13.0",
            "answers": answers,
            "usage": { "input_tokens": 10, "output_tokens": 2 }
        });
        apply_response(route, request, &response).expect("apply")
    }

    fn block_reasons(applied: &Value) -> Vec<String> {
        applied["blockReasons"]
            .as_array()
            .expect("blockReasons array")
            .iter()
            .map(|value| value.as_str().expect("reason string").to_owned())
            .collect()
    }

    #[test]
    fn ambiguous_noul_blocks_for_more_evidence() {
        // An ambiguous hunch (0.5, inside the (0.4,0.6) band) must block, where a
        // clear yes (0.8) did not — the JS reference behavior the native port lacked.
        let request = build_request(&hunch_query(), "jev-latest").expect("request");
        let applied = apply(
            "hunch_check",
            &request,
            json!({ "worth_pursuing": { "type": "noul", "noul": 0.5 } }),
        );
        assert_eq!(applied["blocked"], true);
        assert!(block_reasons(&applied)[0].contains("worth_pursuing is ambiguous"));

        let clear = apply(
            "hunch_check",
            &request,
            json!({ "worth_pursuing": { "type": "noul", "noul": 0.8 } }),
        );
        assert_eq!(clear["blocked"], false);
    }

    fn decision_query() -> serde_json::Value {
        json!({
            "route": "decision_review",
            "deliberation": delib(),
            "state": {
                "proposal": "Ship the parity change.",
                "assumptions": ["The JS policy is the calibrated source of truth."],
                "risks": [
                    { "id": "R1", "description": "A missed additive gap." },
                    { "id": "R2", "description": "A regression risk." }
                ]
            }
        })
    }

    #[test]
    fn decision_review_ambiguous_evidence_need_blocks() {
        // The exact dogfood case: proposal viable (0.8) but more_evidence_needed
        // ambiguous (0.48) — the native tool now blocks, matching the JS runner.
        let request = build_request(&decision_query(), "jev-latest").expect("request");
        let applied = apply(
            "decision_review",
            &request,
            json!({
                "proposal_viable": { "type": "noul", "noul": 0.8 },
                "primary_risk": choice("R1", json!({ "R1": 0.7, "R2": 0.2, "none": 0.1 })),
                "more_evidence_needed": { "type": "noul", "noul": 0.48 }
            }),
        );
        assert_eq!(applied["blocked"], true);
        assert!(block_reasons(&applied)
            .iter()
            .any(|reason| reason.contains("more_evidence_needed is ambiguous")));
    }

    #[test]
    fn choice_soft_tie_and_none_block_but_clear_lead_does_not() {
        let request = build_request(&decision_query(), "jev-latest").expect("request");
        let clear = json!({
            "proposal_viable": { "type": "noul", "noul": 0.85 },
            "more_evidence_needed": { "type": "noul", "noul": 0.1 }
        });

        // Soft tie: R1 0.5 vs R2 0.44 (gap 0.06 < 0.15) blocks.
        let mut answers = clear.as_object().unwrap().clone();
        answers.insert(
            "primary_risk".to_owned(),
            choice("R1", json!({ "R1": 0.5, "R2": 0.44, "none": 0.06 })),
        );
        let applied = apply("decision_review", &request, Value::Object(answers));
        assert_eq!(applied["blocked"], true);
        assert!(block_reasons(&applied)
            .iter()
            .any(|reason| reason.contains("primary_risk is a soft tie")));

        // Selected none blocks with the reframe reason.
        let mut answers = clear.as_object().unwrap().clone();
        answers.insert(
            "primary_risk".to_owned(),
            choice("none", json!({ "none": 0.7, "R1": 0.2, "R2": 0.1 })),
        );
        let applied = apply("decision_review", &request, Value::Object(answers));
        assert!(block_reasons(&applied)
            .iter()
            .any(|reason| reason.contains("primary_risk selected none")));

        // Clear lead: R1 0.8 vs R2 0.15 (gap 0.65) does not block.
        let mut answers = clear.as_object().unwrap().clone();
        answers.insert(
            "primary_risk".to_owned(),
            choice("R1", json!({ "R1": 0.8, "R2": 0.15, "none": 0.05 })),
        );
        let applied = apply("decision_review", &request, Value::Object(answers));
        assert_eq!(applied["blocked"], false);
    }

    #[test]
    fn next_check_soft_tie_resolves_by_preference_but_blocks_against_none() {
        let base = json!({
            "route": "hypothesis_triage",
            "deliberation": delib(),
            "state": {
                "hypotheses": [
                    { "id": "h1", "statement": "The runtime owns it." },
                    { "id": "h2", "statement": "The adapter owns it." }
                ],
                "nextChecks": [
                    { "id": "c1", "action": "Run the runtime test." },
                    { "id": "c2", "action": "Run the adapter test." }
                ]
            }
        });
        let request = build_request(&base, "jev-latest").expect("request");
        let hypothesis = choice("h1", json!({ "h1": 0.9, "h2": 0.08, "none": 0.02 }));

        // Two near-tied real checks (c1 0.5, c2 0.45) resolve by preference: no block.
        let applied = apply(
            "hypothesis_triage",
            &request,
            json!({
                "hypothesis": hypothesis,
                "next_check": choice("c1", json!({ "c1": 0.5, "c2": 0.45, "none": 0.05 }))
            }),
        );
        assert_eq!(
            applied["blocked"],
            false,
            "reasons: {:?}",
            block_reasons(&applied)
        );

        // A soft tie against `none` (not a real check) has no preference: blocks.
        let hypothesis = choice("h1", json!({ "h1": 0.9, "h2": 0.08, "none": 0.02 }));
        let applied = apply(
            "hypothesis_triage",
            &request,
            json!({
                "hypothesis": hypothesis,
                "next_check": choice("c1", json!({ "c1": 0.5, "c2": 0.04, "none": 0.46 }))
            }),
        );
        assert!(block_reasons(&applied)
            .iter()
            .any(|reason| reason.contains("next_check is a soft tie")));
    }

    #[test]
    fn grounded_below_relaxed_minimum_blocks_and_point_five_gate_passes() {
        let base = json!({
            "route": "hallucination_gate",
            "deliberation": delib(),
            "state": {
                "claim": "The function validates input.",
                "evidence": [
                    { "id": "e1", "content": "if (!valid) throw" },
                    { "id": "e2", "content": "unrelated log line" }
                ]
            }
        });
        let request = build_request(&base, "jev-latest").expect("request");

        // grounded 0.3 (< 0.5) blocks with the below-minimum reason.
        let applied = apply(
            "hallucination_gate",
            &request,
            json!({
                "grounded": { "type": "noul", "noul": 0.3 },
                "evidence_anchor": choice("e1", json!({ "e1": 0.8, "e2": 0.1, "none": 0.1 }))
            }),
        );
        assert!(block_reasons(&applied)
            .iter()
            .any(|reason| reason.contains("grounded is below policy minimum 0.5")));

        // grounded 0.9 with a clear anchor passes the relaxed gate.
        let applied = apply(
            "hallucination_gate",
            &request,
            json!({
                "grounded": { "type": "noul", "noul": 0.9 },
                "evidence_anchor": choice("e1", json!({ "e1": 0.85, "e2": 0.1, "none": 0.05 }))
            }),
        );
        assert_eq!(applied["blocked"], false);
    }
}
