//! Local preview never dispatches a remote reader or starts an evaluation.
//! Named remote sources are described through selectors; local direct reads
//! use the same capture/redaction path as the live evaluation.
use super::*;
use serde_json::Map;

pub(super) fn row(
    evaluator: &Evaluator,
    row: &Row,
    reader: &dyn SourceReader,
    context: &ExecutionContext,
) -> RowOutcome {
    let origin = match evaluator.provider_origin() {
        Ok(origin) => origin,
        Err(e) => return evaluate::rejected(e, None),
    };
    let models = matches!(row.input, Input::Models);
    let sends = if models {
        "model catalog request"
    } else {
        "selected evidence, questions, and supplied task context"
    };
    let mut destinations = vec![
        json!({"provider":"classification","origin":origin,"sends":[sends,"classification provider authorization header"]}),
    ];
    if let Err(error) = add_source_destinations(&row.query, reader, &mut destinations) {
        return evaluate::rejected(error, None);
    }
    let mut preview = json!({"destinations":destinations});
    if models {
        preview["requestsCeiling"] = json!(1 + evaluator.retry_room());
        return RowOutcome::Preview(json!({"preview":preview}));
    }
    let mut selection = declared_selection(row, evaluator);
    let mut ceiling = row.max_requests;
    if let Input::Judge(target) = &row.input
        && row.assets.is_none()
        && !row.per_item
        && !row.per_hit
        && !row.collapse
        && match target {
            Target::Content(_) => true,
            Target::Source(s) => s["tool"] == "localFetch",
        }
    {
        // Only literal or direct local text reaches this reader. Binary inputs
        // stay source selectors, without pricing or persisting image bytes.
        match evaluate::preview_text(row, target, reader, context) {
            Ok(Some(prepared)) => {
                if matches!(target, Target::Source(_)) {
                    selection.insert("captured".into(), prepared.sent(row).into_owned());
                }
                let plan = prepared.plan(evaluator, row);
                let price = match super::admission::admit(
                    row.max_requests,
                    plan,
                    plan.first,
                    evaluator.retry_room(),
                ) {
                    Ok(admitted) => admitted.price.ceiling,
                    Err(over) => over.price.ceiling,
                };
                ceiling = row.max_requests.or(price);
            }
            Ok(None) => {}
            Err(out) => return *out,
        }
    }
    let mut selection = Value::Object(selection);
    // URL queries, userinfo and image encodings never appear in the preview.
    safe_display(&mut selection);
    reader.security().sanitize_data(&mut selection);
    let markers = octocode_runtime_base::security::placeholders_in(&selection);
    preview["selection"] = selection;
    preview["redaction"] = json!(format!(
        "Screened selection contains {markers} redaction markers. Source selectors without captured text are unresolved. PNG/JPEG pixels can be sent without text redaction."
    ));
    if let Some(ceiling) = ceiling {
        preview["requestsCeiling"] = json!(ceiling);
    }
    RowOutcome::Preview(json!({"preview":preview}))
}

/// The fields that select or supply evidence to the classification provider.
fn declared_selection(row: &Row, evaluator: &Evaluator) -> Map<String, Value> {
    let mut selection = Map::new();
    for key in [
        "content",
        "questions",
        "predicates",
        "background",
        "mainGoal",
        "source",
        "assets",
        "resources",
        "perItem",
        "perHit",
        "collapse",
        "model",
        "longContext",
    ] {
        if let Some(value) = row.query.get(key) {
            selection.insert(key.to_owned(), value.clone());
        }
    }
    selection
        .entry("model")
        .or_insert_with(|| json!(evaluator.model));
    selection
}

fn add_source_destinations(
    query: &Value,
    reader: &dyn SourceReader,
    destinations: &mut Vec<Value>,
) -> Result<(), ClassificationError> {
    for source in query
        .get("source")
        .into_iter()
        .chain(
            query["resources"]
                .as_object()
                .into_iter()
                .flat_map(|m| m.values()),
        )
        .chain(
            query["assets"]
                .as_object()
                .into_iter()
                .flat_map(|m| m.values())
                .filter_map(|asset| asset.get("source")),
        )
    {
        for dest in reader_destinations(source, reader)? {
            if !destinations.contains(&dest) {
                destinations.push(dest);
            }
        }
    }

    Ok(())
}

/// Inspect a nested source without executing it. Registry sources use the same
/// offline planner as a direct artifactSearch preview.
fn reader_destinations(
    source: &Value,
    reader: &dyn SourceReader,
) -> Result<Vec<Value>, ClassificationError> {
    let github = &reader.config().resolved.github.api_url;
    if let Some(raw) = source["url"].as_str() {
        let url = octocode_tools_shared::source_capture::url::parse(raw)?;
        return Ok(vec![
            json!({"provider":"source website","origin":url.origin().ascii_serialization(),"sends":["requested URL path and query"],"when":"source capture"}),
            json!({"provider":"source website","sends":["redirect URL path and query"],"when":"source responds with a redirect to another permitted HTTPS origin"}),
        ]);
    }
    let tool = source["tool"]
        .as_str()
        .and_then(octocode_runtime_base::id::ToolId::from_name);
    if tool == Some(octocode_runtime_base::id::ToolId::ArtifactSearch) {
        if source["query"]["preview"] == true {
            return Ok(Vec::new());
        }
        let query = serde_json::from_value(source["query"].clone()).map_err(|_| {
            ClassificationError::new(
                octocode_contract_types::ErrorCode::InvalidInput,
                "Invalid registry source selector.",
                "Supply an artifactSearch query.",
            )
        })?;
        return reader.artifact_preview_destinations(&query);
    }
    if tool.is_some_and(octocode_runtime_base::id::ToolId::is_github) {
        let origin = url::Url::parse(github)
            .ok()
            .map(|url| url.origin().ascii_serialization());
        let mut dest = json!({"provider":"github","sends":["repository, source selection and revision","GitHub authorization header when authenticated"],"when":"source capture"});
        if let Some(origin) = origin {
            dest["origin"] = origin.into();
        }
        return Ok(vec![dest]);
    }
    Ok(Vec::new())
}

fn safe_display(value: &mut Value) {
    match value {
        Value::Object(fields) => {
            for (key, v) in fields {
                if matches!(key.as_str(), "url" | "registryUrl")
                    && let Some(url) = v.as_str().and_then(|u| url::Url::parse(u).ok())
                {
                    *v = json!(url.origin().ascii_serialization());
                } else {
                    safe_display(v);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(safe_display),
        Value::String(text)
            if text.contains("data:image/")
                || text.contains("iVBORw0KGgo")
                || text.contains("/9j/") =>
        {
            *value = json!("[image data withheld]");
        }
        _ => {}
    }
}
