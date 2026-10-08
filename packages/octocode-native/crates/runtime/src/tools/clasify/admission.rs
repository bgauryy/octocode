//! Batch admission: the relation rules core's clasify schema refines and a
//! row-wise contract check cannot express (unique ids, candidate-evidence and
//! prefilter tools, expanded cell limits). The engine runs them once, after
//! contract validation and before any provider spend; the wording is
//! core-authored and pinned by the parity fixtures.
use super::{is_candidate_search_tool, is_file_read_tool};
use crate::contracts::{ContractValidationError, ValidationIssue, policy_names};
use crate::tools::id::{ToolId, clasify_policy};
use serde_json::Value;
use std::collections::HashSet;

/// Checks every validated matrix of one call; issue paths name the caller's
/// fields under their `queries.N` row.
pub(crate) fn check<'a>(
    queries: impl IntoIterator<Item = &'a Value>,
) -> Result<(), ContractValidationError> {
    // Batch query ids stay unique so every result cell is correlatable.
    let queries: Vec<&Value> = queries.into_iter().collect();
    let mut seen = HashSet::new();
    for (index, query) in queries.iter().enumerate() {
        if let Some(id) = query.get("id").and_then(Value::as_str)
            && !seen.insert(id)
        {
            return Err(rejection(
                "clasify.unique-query-ids",
                row_path(index, &["id"]),
                format!("Duplicate query id: {id}"),
                None,
            ));
        }
    }
    let mut total_cells = 0usize;
    for (index, query) in queries.iter().enumerate() {
        relations(query).map_err(|mut error| {
            for issue in &mut error.issues {
                issue
                    .path
                    .splice(0..0, ["queries".to_owned(), index.to_string()]);
            }
            error
        })?;
        total_cells = total_cells.saturating_add(cell_count(query));
    }
    if total_cells > clasify_policy::MAX_TOTAL_CELLS {
        return Err(rejection(
            "clasify.total-cell-limit",
            vec!["queries".into()],
            format!(
                "Matrices produce {total_cells} cells in total; maximum is {}.",
                clasify_policy::MAX_TOTAL_CELLS
            ),
            None,
        ));
    }
    Ok(())
}

fn rejection(
    rule_id: &str,
    path: Vec<String>,
    message: String,
    received: Option<Value>,
) -> ContractValidationError {
    ContractValidationError {
        issues: vec![ValidationIssue {
            rule_id: rule_id.into(),
            path,
            message,
            schema: None,
            received,
        }],
    }
}

fn row_path(index: usize, fields: &[&str]) -> Vec<String> {
    let mut path = vec!["queries".to_owned(), index.to_string()];
    path.extend(fields.iter().map(|field| (*field).to_owned()));
    path
}

/// Core `expandedCells`: declared resources × questions. Runtime fan-out of a
/// search resource is bounded per call by `maxPages`, not here.
fn cell_count(query: &Value) -> usize {
    let count = |field: &str| query[field].as_array().map_or(0, Vec::len);
    count("resources").saturating_mul(count("questions"))
}

/// Relation checks over one matrix; issue paths name the caller's fields
/// (`resources.N.candidateEvidence`).
fn relations(query: &Value) -> Result<(), ContractValidationError> {
    let empty = Vec::new();
    let resources = query["resources"].as_array().unwrap_or(&empty);
    let questions = query["questions"].as_array().unwrap_or(&empty);
    for (index, resource) in resources.iter().enumerate() {
        let at = |field: &str| vec!["resources".to_owned(), index.to_string(), field.to_owned()];
        let tool = resource["tool"].as_str().and_then(ToolId::from_name);
        if resource.get("candidateEvidence").is_some()
            && !tool.is_some_and(is_candidate_search_tool)
        {
            return Err(rejection(
                "clasify.candidate-evidence",
                at("candidateEvidence"),
                format!(
                    "candidateEvidence requires {}.",
                    policy_names(clasify_policy::CANDIDATE_SEARCH_TOOLS)
                ),
                resource.get("candidateEvidence").cloned(),
            ));
        }
        let prefiltered = resource
            .get("prefilter")
            .and_then(Value::as_array)
            .is_some_and(|terms| !terms.is_empty());
        if prefiltered && !tool.is_some_and(is_file_read_tool) {
            return Err(rejection(
                "clasify.prefilter-tool",
                at("prefilter"),
                format!(
                    "prefilter applies only to {} file reads; remove it or narrow the search itself.",
                    policy_names(clasify_policy::FILE_READ_TOOLS)
                ),
                None,
            ));
        }
    }
    for (field, rows) in [("resources", resources), ("questions", questions)] {
        let mut seen = HashSet::new();
        for (index, row) in rows.iter().enumerate() {
            if let Some(id) = row.get("id").and_then(Value::as_str)
                && !seen.insert(id)
            {
                return Err(rejection(
                    "clasify.unique-ids",
                    vec![field.into(), index.to_string(), "id".into()],
                    format!("Duplicate {field} id: {id}"),
                    None,
                ));
            }
        }
    }
    let cells = cell_count(query);
    if cells > clasify_policy::MAX_CELLS {
        return Err(rejection(
            "clasify.cell-limit",
            Vec::new(),
            format!(
                "Resources × questions produces {cells} cells; maximum is {}.",
                clasify_policy::MAX_CELLS
            ),
            None,
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::contracts::{ContractValidationError, prepare_many_and_validate};
    use serde_json::{Value, json};

    /// The engine's clasify admission: contract validation, then the batch
    /// relation rules.
    fn admit(input: Value) -> Result<Vec<Value>, ContractValidationError> {
        let queries = prepare_many_and_validate("clasify", input)?;
        super::check(&queries)?;
        Ok(queries)
    }

    fn with_id(id: impl Into<String>, question: &Value) -> Value {
        let mut question = question.clone();
        question["id"] = Value::String(id.into());
        question
    }

    #[test]
    fn semantic_matrix_rejects_duplicate_ids_and_more_than_twenty_five_cells() {
        let question = json!({"type":"yesno","ask":"Is it relevant?"});
        let duplicate = admit(json!({"queries":[{
            "id":"duplicates",
            "reasoning":"Classify resources.","mainGoal":"Decide the next read.",
            "resources":[
                {"id":"same","value":"one"},
                {"id":"same","value":"two"}
            ],
            "questions":[with_id("q1", &question)]
        }]}))
        .expect_err("duplicate IDs must fail");
        assert_eq!(
            duplicate.issues[0].path,
            ["queries", "0", "resources", "1", "id"]
        );

        let resources: Vec<_> = (0..6)
            .map(|index| json!({"id":format!("r{index}"),"value":{"index":index}}))
            .collect();
        let questions: Vec<_> = (0..5)
            .map(|index| with_id(format!("q{index}"), &question))
            .collect();
        let oversized = admit(json!({"queries":[{
            "id":"oversized",
            "reasoning":"Classify resources.","mainGoal":"Decide the next read.",
            "resources":resources,
            "questions":questions
        }]}))
        .expect_err("matrix cell limit must fail");
        assert_eq!(oversized.issues[0].rule_id, "clasify.cell-limit");
        assert!(oversized.issues[0].message.contains("maximum is 25"));

        let hydrated_resources = ["one", "two"].map(|id| {
            json!({"id":id,
                "tool":"localSearch",
                "candidateEvidence":"fileChunks",
                "query":{"mainGoal": "test", "reasoning":"Find candidates.","path":"/tmp","matchString":"anchor"}
            })
        });
        let hydrated_questions = (0..3)
            .map(|index| with_id(format!("q{index}"), &question))
            .collect::<Vec<_>>();
        // Cells count declared resources: a search's candidates are bounded
        // per call by maxPages at runtime, not at admission.
        admit(json!({"queries":[{
            "id":"expanded",
            "reasoning":"Classify bounded file candidates.","mainGoal":"Decide the next read.",
            "resources":hydrated_resources,
            "questions":hydrated_questions
        }]}))
        .expect("file-chunk candidates do not count toward the cell limit");
    }

    #[test]
    fn clasify_rejects_invalid_candidate_evidence_ranges_and_batch_cells() {
        let question = json!({"type":"yesno","ask":"Is it relevant?"});
        let error = admit(json!({"queries":[{
            "id":"invalid-context",
            "reasoning":"Reject invalid delegated context before execution.","mainGoal":"Decide the next read.",
            "resources":[{"id":"r","tool":"localFetch","candidateEvidence":"fileChunks",
                "query":{"mainGoal": "test", "reasoning":"Invalid search mode.","path":"/tmp/a.rs"}}],
            "questions":[with_id("q", &question)]
        }]}))
        .expect_err("invalid context relation must fail at admission");
        assert_eq!(error.issues[0].rule_id, "clasify.candidate-evidence");

        // The resource's field is named where the caller wrote it, under its
        // row prefix.
        let flat = json!({"id":"r","tool":"localFetch","query":{"path":"/tmp/a.rs"},"candidateEvidence":"fileChunks"});
        let error = admit(json!({"queries":[{
            "reasoning":"Reject a flat resource's search mode.","mainGoal":"Decide the next read.",
            "resources":[flat],
            "questions":[with_id("q", &question)]
        }]}))
        .expect_err("candidateEvidence on a file read must fail");
        assert_eq!(
            error.issues[0].path,
            ["queries", "0", "resources", "0", "candidateEvidence"]
        );

        let matrix = |id: &str| {
            json!({
                "id":id,
                "reasoning":"Exercise the total expanded-cell limit.","mainGoal":"Decide the next read.",
                "resources":(0..5).map(|index| json!({"id":format!("r{index}"),"value":{"index":index}})).collect::<Vec<_>>(),
                "questions":(0..4).map(|index| with_id(format!("q{index}"), &question)).collect::<Vec<_>>()
            })
        };
        let error = admit(json!({"queries":[matrix("a"),matrix("b"),matrix("c")]}))
            .expect_err("batch-wide expanded cells must fail before execution");
        assert_eq!(error.issues[0].rule_id, "clasify.total-cell-limit");
    }

    #[test]
    fn clasify_rejects_duplicate_batch_query_ids() {
        let query = json!({
            "id":"qa",
            "reasoning":"Classify resources.","mainGoal":"Decide the next read.",
            "resources":[{"id":"r","value":"one"}],
            "questions":[{"id":"q1","type":"yesno","ask":"Relevant?"}]
        });
        let error = admit(json!({"queries":[query.clone(), query]}))
            .expect_err("duplicate batch query ids must fail before provider spend");
        assert_eq!(error.issues[0].rule_id, "clasify.unique-query-ids");
        assert_eq!(error.issues[0].path, ["queries", "1", "id"]);
    }

    /// Core authors the wording of cross-field clasify rejections; native
    /// renders the same text for the same input (fixture `messages`), and
    /// every clasify fixture's verdict is the engine admission's.
    #[test]
    fn admission_matches_the_core_clasify_fixtures() {
        let fixtures: Value =
            serde_json::from_str(crate::contracts::generated::CONTRACT_FIXTURES_JSON)
                .expect("generated fixture JSON");
        let mut worded = 0;
        for fixture in fixtures.as_array().expect("fixture array") {
            if fixture["tool"] != "clasify" {
                continue;
            }
            let result = admit(fixture["input"].clone());
            assert_eq!(
                result.is_ok(),
                fixture["accepted"].as_bool().expect("accepted"),
                "{}: {result:?}",
                fixture["id"],
            );
            if let Ok(queries) = &result {
                let expected = fixture["normalized"]
                    .get("queries")
                    .and_then(Value::as_array)
                    .and_then(|queries| queries.first())
                    .unwrap_or(&fixture["normalized"]);
                assert_eq!(&queries[0], expected, "{}", fixture["id"]);
            }
            if let Some(expected) = fixture.get("messages") {
                let messages = result
                    .expect_err("a fixture with messages is rejected")
                    .issues
                    .iter()
                    .map(|issue| Value::String(issue.message.clone()))
                    .collect::<Vec<_>>();
                assert_eq!(&Value::Array(messages), expected, "{}", fixture["id"]);
                worded += 1;
            }
        }
        assert!(worded >= 5, "clasify wording fixtures: {worded}");
    }
}
