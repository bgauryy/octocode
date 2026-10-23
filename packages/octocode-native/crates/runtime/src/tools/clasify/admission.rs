//! Admission rules for clasify rows that the row schema cannot express.
use serde_json::Value;

/// The batch rule the row schema cannot express: query ids stay unique so
/// every answer row is correlatable. Core authors the wording (parity
/// fixture `clasify.unique-query-ids`).
pub(crate) fn admit<'a>(
    queries: impl IntoIterator<Item = &'a Value>,
) -> Result<(), crate::contracts::ContractValidationError> {
    let mut seen = std::collections::HashSet::new();
    for (index, query) in queries.into_iter().enumerate() {
        if let Some(id) = query.get("id").and_then(Value::as_str)
            && !seen.insert(id)
        {
            return Err(crate::contracts::ContractValidationError {
                issues: vec![crate::contracts::ValidationIssue {
                    rule_id: "clasify.unique-query-ids".into(),
                    path: vec!["queries".into(), index.to_string(), "id".into()],
                    message: format!("Duplicate query id: {id}"),
                    schema: None,
                    received: None,
                }],
            });
        }
    }
    Ok(())
}

/// The reader a row's source needs: its tool, or localFetch for a URL (its
/// capture is read back through localFetch).
fn source_reader(source: &Value) -> &str {
    source
        .get("tool")
        .and_then(Value::as_str)
        .unwrap_or(crate::id::ToolId::LocalFetch.as_str())
}

/// One issue for a row whose source reader this runtime does not enable.
/// MCP rejects the same input through its availability-scoped schema.
pub(crate) fn unavailable_source_issues(
    query: &Value,
    prefix: &[String],
    availability: &crate::execution::availability::ToolAvailability,
) -> Vec<crate::contracts::ValidationIssue> {
    let mut sources = Vec::new();
    if let Some(source) = query.get("source") {
        sources.push((source, vec!["source".to_owned()]));
    }
    if let Some(resources) = query["resources"].as_object() {
        for (key, source) in resources {
            sources.push((source, vec!["resources".into(), key.clone()]));
        }
    }
    if let Some(assets) = query["assets"].as_object() {
        for (key, asset) in assets {
            sources.push((
                &asset["source"],
                vec!["assets".into(), key.clone(), "source".into()],
            ));
        }
    }
    sources.into_iter().filter_map(|(source, suffix)| {
    let reader = source_reader(source);
    if availability.is_available(reader) { return None; }
    let reason = availability
        .get(reader)
        .map_or_else(|| "no such tool.".to_owned(), |entry| entry.reason_text());
    let mut path = prefix.to_vec();
    path.extend(suffix);
    Some(crate::contracts::ValidationIssue {
        rule_id: "clasify.source-reader-available".into(),
        path,
        message: format!(
            "This source reads through {reader}, which is disabled in this runtime: {reason} Supply the data as state instead."
        ),
        schema: None,
        received: None,
    })
    }).collect()
}
