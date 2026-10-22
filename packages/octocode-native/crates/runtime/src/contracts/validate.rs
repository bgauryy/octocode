mod artifact;
mod content;
mod depth_range;
mod enum_value;
mod github_search;
mod history;
mod local_search;
mod lsp;
#[cfg(test)]
mod normalizer_tests;
#[cfg(test)]
mod numeric_tests;
#[cfg(test)]
mod pattern_tests;
mod qualifiers;
mod repair;
mod repository;
mod shape;
mod structural_rule;
pub(crate) use qualifiers::qualifier_terms;
pub(crate) use repair::repair_input;
mod schema;
mod union;
use artifact::validate_artifact_registry_url;
use github_search::{GithubSearchKind, validate_github_search_queries};
use history::{validate_history_keyword_scope, validate_history_repository_scope};
use local_search::validate_local_search_queries;
use lsp::validate_lsp_queries;
use schema::{validate_input_schema, validate_schema};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt::{Display, Formatter};

use crate::id::ToolId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationIssue {
    pub rule_id: String,
    pub path: Vec<String>,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub received: Option<Value>,
}

/// Where an unknown field sits: `queries[N]` (the row's 0-based output
/// `index`) for a bulk row, else the object path (`questions.0`), so a
/// root-level clasify matrix is not called a query row.
fn query_label(path: &[String]) -> String {
    match path.first().map(String::as_str) {
        Some("queries") => {
            let index = path.get(1).map_or("0", String::as_str);
            if path.len() > 3 {
                format!("queries[{index}].{}", path[2..path.len() - 1].join("."))
            } else {
                format!("queries[{index}]")
            }
        }
        _ if path.len() > 1 => path[..path.len() - 1].join("."),
        _ => "the request".into(),
    }
}

/// `siblingRequires` is set by union validation when another branch declares
/// the unknown field; it names the fields that select that branch.
fn sibling_requirement(issue: &ValidationIssue) -> Option<String> {
    let fields = issue
        .schema
        .as_ref()?
        .get("siblingRequires")?
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>();
    (!fields.is_empty()).then(|| fields.join(" and "))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractValidationError {
    pub issues: Vec<ValidationIssue>,
}

/// Projects transport-neutral validation issues into the stable tool-error
/// envelope. `mcp` selects the schema pointer: MCP clients read the tool's
/// `inputSchema`; only the CLI has the `schema` command.
#[must_use]
pub fn format_input_error(tool_name: &str, error: &ContractValidationError) -> Value {
    let details = error
        .issues
        .iter()
        .map(|issue| {
            let path = issue.path.join(".");
            // One wording for an unknown field, alone or beside other issues.
            if issue.rule_id == "schema.unknown-field" {
                return unknown_field_message(tool_name, issue);
            }
            let message = match required_guidance(issue) {
                Some(guidance) => format!("{} ({guidance})", issue.message),
                None => issue.message.clone(),
            };
            if path.is_empty() {
                message
            } else {
                format!("{path}: {message}")
            }
        })
        .chain(routing_details(tool_name, &error.issues))
        .collect::<Vec<_>>();
    issues_error(details)
}

/// The repair for one unknown field: the branch that declares it, a
/// top-level row field to move, or removal with the closest known field.
fn unknown_field_message(tool_name: &str, issue: &ValidationIssue) -> String {
    let query = query_label(&issue.path);
    let field = issue.path.last().map(String::as_str).unwrap_or("unknown");
    // Known fields from the embedded schema suggest the closest match (edit
    // distance <= 3) as a "did you mean ...?" hint.
    let known: Vec<&str> = issue
        .schema
        .as_ref()
        .and_then(|s| s["knownFields"].as_array())
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str())
        .collect();
    let msg = match (
        sibling_requirement(issue),
        field_guess(issue, field, &known),
    ) {
        (Some(requires), _) => sibling_message(issue, field, &query, &requires),
        _ if is_root_brief(tool_name, &issue.path) => root_brief_message(field),
        (None, Some(s)) => {
            format!("Remove unknown field '{field}' from {query} (did you mean '{s}'?)")
        }
        (None, None) => format!("Remove unknown field(s) from {query}: {field}"),
    };
    if issue.path.iter().any(|segment| segment == "rule") {
        format!(
            "{}. Example rule: {}",
            msg.trim_end_matches('.'),
            octocode_engine::structural::EXAMPLE_RULE
        )
    } else {
        msg
    }
}

/// The error envelope every surface returns for a call that never reached a
/// tool: the error and, when there are more, its other repair details. The
/// caller knows the tool; `error` (not `results`) marks the envelope.
pub fn tool_error(error: &str, details: Vec<String>) -> Value {
    let mut value = serde_json::json!({ "error": error });
    if !details.is_empty() {
        value["details"] = serde_json::json!(details);
    }
    value
}

/// Repair details as one error: the first names the fix, the rest follow.
fn issues_error(mut details: Vec<String>) -> Value {
    if details.is_empty() {
        return tool_error("Invalid input.", Vec::new());
    }
    let first = details.remove(0);
    tool_error(&first, details)
}

/// Core-authored guidance for a missing required field: the field's
/// contract `x-requiredMessage`, rendered after `Missing required field:
/// <name>` as MCP renders the core message.
fn required_guidance(issue: &ValidationIssue) -> Option<&str> {
    if issue.rule_id != "schema.required" {
        return None;
    }
    issue.schema.as_ref()?.get("x-requiredMessage")?.as_str()
}

/// A row field (`mainGoal`, `reasoning`, …) sent beside `queries` instead of
/// inside each row: the envelope does not declare it, the rows do.
fn is_root_brief(tool_name: &str, path: &[String]) -> bool {
    matches!(path, [field] if crate::id::ToolId::from_name(tool_name)
        .is_some_and(|tool| super::query_declares(tool, field)))
}

fn root_brief_message(field: &str) -> String {
    format!("Move '{field}' into each queries[] row: a top-level {field} is not inherited.")
}

/// Guidance that names the fix beyond the field-level issues: the tool that
/// owns a row's unknown fields or operation, and how to split an oversized
/// batch.
fn routing_details(tool_name: &str, issues: &[ValidationIssue]) -> Vec<String> {
    let mut details = Vec::new();
    // Per row: its unknown fields and whether each has an in-tool suggestion.
    let mut rows: Vec<(String, Vec<(&str, bool)>)> = Vec::new();
    for issue in issues {
        match (issue.rule_id.as_str(), issue.path.as_slice()) {
            ("schema.unknown-field", [queries, _, field]) if queries == "queries" => {
                let label = query_label(&issue.path);
                let known = issue
                    .schema
                    .as_ref()
                    .and_then(|s| s["knownFields"].as_array())
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>();
                // A field another form of this tool declares is fixed in
                // place (`it applies only with …`), never rerouted.
                let in_tool = suggest_field(field, &known).is_some()
                    || name_field(issue).is_some()
                    || sibling_requirement(issue).is_some();
                let entry = (field.as_str(), in_tool);
                match rows.iter_mut().find(|(row, _)| *row == label) {
                    Some((_, fields)) => fields.push(entry),
                    None => rows.push((label, vec![entry])),
                }
            }
            ("schema.enum", [.., last]) if last == "operation" => {
                if let Some(operation) = issue.received.as_ref().and_then(Value::as_str) {
                    let owners = owning_tools(tool_name, |tool| {
                        operations(tool).iter().any(|name| name == operation)
                    });
                    if let Some(owners) = owners {
                        details.push(format!(
                            "operation \"{operation}\" is a {owners} operation: send that row to {owners}."
                        ));
                    }
                }
            }
            ("schema.range", [queries, _, field])
                if queries == "queries" && field == "length" && windowed(tool_name) =>
            {
                if let Some(message) = window_cap_message(issue) {
                    details.push(message);
                }
            }
            ("schema.size", [queries]) if queries == "queries" => {
                if let Some(message) = split_batch_message(&issue.message) {
                    details.push(message);
                }
            }
            _ => {}
        }
    }
    for (label, mut entries) in rows {
        entries.sort_unstable();
        entries.dedup();
        // One misspelled field the tool itself can name needs no rerouting.
        if let [(_, true)] = entries.as_slice() {
            continue;
        }
        let fields = entries.iter().map(|(field, _)| *field).collect::<Vec<_>>();
        let owners = owning_tools(tool_name, |tool| {
            let known = query_fields(tool);
            fields
                .iter()
                .all(|field| known.iter().any(|name| name == field))
        });
        if let Some(owners) = owners {
            let (subject, noun) = if fields.len() == 1 {
                (format!("{} is a", fields[0]), "field")
            } else {
                (format!("{} are", fields.join(", ")), "fields")
            };
            details.push(format!(
                "{subject} {owners} {noun}: send {label} to {owners}."
            ));
        }
    }
    details.dedup();
    details
}

/// A `length` above the window cap → the same read at the cap, and where the
/// rest is: every result's `next` continuation carries the following offset.
/// A tool whose rows read a window: `length` pages from an `offset`.
fn windowed(tool_name: &str) -> bool {
    crate::id::ToolId::from_name(tool_name)
        .is_some_and(|tool| super::query_declares(tool, "offset"))
}

fn window_cap_message(issue: &ValidationIssue) -> Option<String> {
    let cap = issue.schema.as_ref()?.get("maximum")?.as_u64()?;
    (issue.received.as_ref()?.as_u64()? > cap).then(|| {
        format!(
            "Send the same read with \"length\": {cap}: the result's next carries the offset of the rest \
             (copy it into the following call) until a page has no next."
        )
    })
}

/// `Value length 7 exceeds the maximum of 5` → how many calls to send.
fn split_batch_message(message: &str) -> Option<String> {
    let numbers = message
        .split(|c: char| !c.is_ascii_digit())
        .filter_map(|part| part.parse::<usize>().ok())
        .collect::<Vec<_>>();
    let [length, maximum] = numbers.as_slice() else {
        return None;
    };
    (message.contains("exceeds") && *maximum > 0).then(|| {
        format!(
            "Send at most {maximum} rows per call: split the batch into {} calls.",
            length.div_ceil(*maximum)
        )
    })
}

/// The one other non-beta tool of the same family (or `a or b` for two)
/// whose query schema satisfies `owns`; `None` when no tool or too many do.
fn owning_tools(tool_name: &str, owns: impl Fn(&Value) -> bool) -> Option<String> {
    // Family and beta are generated from the same contract fields, so only
    // same-family candidates are parsed.
    let family = ToolId::from_name(tool_name)?.family();
    let owners = ToolId::ALL
        .into_iter()
        .filter(|id| id.as_str() != tool_name && !id.is_beta() && id.family() == family)
        .filter_map(|id| super::tool_contract(id).ok())
        .filter(|tool| owns(tool))
        .filter_map(|tool| tool["name"].as_str())
        .collect::<Vec<_>>();
    match owners.as_slice() {
        [one] => Some((*one).to_owned()),
        [a, b] => Some(format!("{a} or {b}")),
        _ => None,
    }
}

/// Every property name any branch of a tool's query schema declares.
fn query_fields(tool: &Value) -> Vec<String> {
    let mut fields = Vec::new();
    walk_query_schema(tool, &mut |schema| {
        if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
            fields.extend(properties.keys().cloned());
        }
    });
    fields
}

/// Every `operation` value any branch of a tool's query schema allows.
fn operations(tool: &Value) -> Vec<String> {
    let mut values = Vec::new();
    walk_query_schema(tool, &mut |schema| {
        if let Some(operation) = schema.pointer("/properties/operation") {
            let pinned = operation.get("const").into_iter();
            let listed = operation
                .get("enum")
                .and_then(Value::as_array)
                .into_iter()
                .flatten();
            values.extend(
                pinned
                    .chain(listed)
                    .filter_map(Value::as_str)
                    .map(str::to_owned),
            );
        }
    });
    values
}

fn walk_query_schema(tool: &Value, visit: &mut dyn FnMut(&Value)) {
    fn walk(root: &Value, schema: &Value, depth: usize, visit: &mut dyn FnMut(&Value)) {
        if depth > 16 {
            return;
        }
        let schema = match schema.get("$ref").and_then(Value::as_str) {
            Some(reference) => match reference.strip_prefix('#') {
                Some(pointer) => root.pointer(pointer).unwrap_or(&Value::Null),
                None => return,
            },
            None => schema,
        };
        visit(schema);
        for key in ["anyOf", "oneOf", "allOf"] {
            for branch in schema
                .get(key)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                walk(root, branch, depth + 1, visit);
            }
        }
    }
    let root = &tool["querySchema"];
    walk(root, root, 0, visit);
}

impl Display for ContractValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "contract validation failed with {} issue(s)",
            self.issues.len()
        )
    }
}

impl std::error::Error for ContractValidationError {}

/// The named tool and its embedded contract entry, or the unknown-tool issue.
fn contract_tool(tool_name: &str) -> Result<(ToolId, &'static Value), ContractValidationError> {
    let id = ToolId::from_name(tool_name).ok_or_else(|| {
        issue(
            "contract.unknown-tool",
            vec![],
            format!("Unknown tool: {tool_name}"),
        )
    })?;
    super::tool_contract(id)
        .map(|tool| (id, tool))
        .map_err(|error| internal(error.to_string()))
}

/// Validates the canonical input, applying schema defaults and integer number
/// representations before typed decoding. Relation rules follow structural parsing.
pub fn validate(tool_name: &str, input: Value) -> Result<Value, ContractValidationError> {
    let (input, batch) = validate_batch(tool_name, input)?;
    let mut issues: Vec<_> = batch
        .rows
        .into_iter()
        .flatten()
        .chain(batch.envelope)
        .collect();
    if !issues.is_empty() {
        issues.dedup();
        return Err(ContractValidationError { issues });
    }
    Ok(input)
}

/// One validation pass's issues: each row's own (paths `queries.<i>.…`, in
/// row order) and the envelope's.
pub(crate) struct BatchIssues {
    pub rows: Vec<Vec<ValidationIssue>>,
    pub envelope: Vec<ValidationIssue>,
}

/// [`validate`] in one pass that keeps each row's issues apart, so a batch
/// isolates its invalid rows without validating any row twice. `Err` is a
/// failure of the contract itself (an unknown tool, a broken rule).
pub(crate) fn validate_batch(
    tool_name: &str,
    mut input: Value,
) -> Result<(Value, BatchIssues), ContractValidationError> {
    let (id, tool) = contract_tool(tool_name)?;
    let plan = RowPlan::of(id, tool);
    apply_normalization_rules(&tool["rules"], &mut input)?;
    let mut rows = Vec::new();
    let item_schema = plan.item_schema.as_ref();
    if let Some(queries) = input.get_mut("queries").and_then(Value::as_array_mut) {
        for (index, query) in queries.iter_mut().enumerate() {
            let mut issues = Vec::new();
            let mut path = vec!["queries".to_owned(), index.to_string()];
            let mut parsed =
                validate_input_schema(&tool["querySchema"], &tool["querySchema"], query, &mut path);
            if parsed.is_ok()
                && let Some(item_schema) = item_schema
            {
                parsed = validate_input_schema(&tool["inputSchema"], item_schema, query, &mut path);
            }
            if let Err(error) = parsed {
                rows.push(error.issues);
                continue;
            }
            // Row rules see a one-row batch; the row moves in and back out.
            let scoped = serde_json::json!({"queries":[std::mem::take(query)]});
            let ruled = apply_validation_rules(&tool["rules"], &scoped);
            if let Value::Object(mut scoped) = scoped
                && let Some(Value::Array(mut rows)) = scoped.remove("queries")
                && let Some(row) = rows.pop()
            {
                *query = row;
            }
            if let Err(mut error) = ruled {
                for issue in &mut error.issues {
                    if issue.path.first().is_some_and(|part| part == "queries")
                        && issue.path.get(1).is_some_and(|part| part == "0")
                    {
                        issue.path[1] = index.to_string();
                    }
                }
                issues.extend(error.issues);
            }
            rows.push(issues);
        }
    }
    // The rows were checked above; the envelope pass checks only the
    // envelope (its own fields and the batch size), and reports its issues
    // beside the rows' so one repair can fix every one.
    let envelope = validate_envelope_with(tool, plan, &mut input)
        .err()
        .map(|error| error.issues)
        .unwrap_or_default();
    Ok((input, BatchIssues { rows, envelope }))
}

/// The envelope pass of [`validate_batch`] alone: the envelope's own fields
/// and the batch size over rows that are validated already.
pub(crate) fn validate_envelope(
    tool_name: &str,
    input: &mut Value,
) -> Result<(), ContractValidationError> {
    let (id, tool) = contract_tool(tool_name)?;
    validate_envelope_with(tool, RowPlan::of(id, tool), input)
}

fn validate_envelope_with(
    tool: &Value,
    plan: &RowPlan,
    input: &mut Value,
) -> Result<(), ContractValidationError> {
    validate_input_schema(&tool["inputSchema"], &plan.envelope, input, &mut Vec::new())
}

/// How one tool's rows validate, decided once from its contract.
struct RowPlan {
    /// The envelope's row schema says more than the query schema: rows
    /// validate against both. Equal schemas (under the same `$defs`) are one
    /// pass.
    item_schema: Option<Value>,
    /// The input schema without its row schema: rows are validated already.
    envelope: Value,
}

impl RowPlan {
    fn of(id: ToolId, tool: &Value) -> &'static RowPlan {
        static PLANS: [std::sync::OnceLock<RowPlan>; ToolId::ALL.len()] =
            [const { std::sync::OnceLock::new() }; ToolId::ALL.len()];
        PLANS[id.index()].get_or_init(|| Self::derive(tool))
    }

    fn derive(tool: &Value) -> Self {
        let without_marker = |schema: &Value| {
            let mut schema = schema.clone();
            if let Some(fields) = schema.as_object_mut() {
                fields.remove("$schema");
            }
            schema
        };
        let (input, query) = (&tool["inputSchema"], &tool["querySchema"]);
        let mut items = Vec::new();
        for queries in super::schema_facts::property_schemas(input, &input["$defs"], "queries") {
            for item in super::schema_facts::item_schemas(queries, &input["$defs"]) {
                if !items.contains(&item) {
                    items.push(item);
                }
            }
        }
        let mut envelope = input.clone();
        let item_schema = if let [item] = items.as_slice() {
            strip_query_items(&mut envelope);
            (without_marker(item) != without_marker(query)
                || (item.to_string().contains("\"$ref\"") && input["$defs"] != query["$defs"]))
                .then(|| (*item).clone())
        } else {
            // Different envelope branches may impose different row schemas;
            // keep their complete validation instead of intersecting alternatives.
            None
        };
        Self {
            item_schema,
            envelope,
        }
    }
}

fn strip_query_items(schema: &mut Value) {
    if let Some(queries) = schema
        .pointer_mut("/properties/queries")
        .and_then(Value::as_object_mut)
    {
        queries.remove("items");
    }
    for keyword in ["anyOf", "oneOf", "allOf"] {
        if let Some(branches) = schema.get_mut(keyword).and_then(Value::as_array_mut) {
            for branch in branches {
                strip_query_items(branch);
            }
        }
    }
}

/// The one input shape every host's call takes: a bare query row (an object
/// without `queries`) runs as a one-row `queries` (see [`one_row_envelope`]).
/// Nothing else changes: a value the canonical schema rejects is rejected by
/// validation with a repair hint, never repaired here. An unknown tool is
/// returned unchanged.
#[must_use]
pub(crate) fn normalize_input(tool_name: &str, input: Value) -> Value {
    let Some(Ok(tool)) = super::tool_contract_named(tool_name) else {
        return input;
    };
    one_row_envelope(&tool["inputSchema"], input)
}

/// A bare query row (an object without `queries`) as `{"queries":[row]}`: the
/// envelope fields it carries (`responseLength`, …) stay on the envelope. An
/// object with only envelope fields is not a row and is returned unchanged.
fn one_row_envelope(input_schema: &Value, input: Value) -> Value {
    let Value::Object(mut row) = input else {
        return input;
    };
    if row.contains_key("queries") {
        return Value::Object(row);
    }
    let mut envelope = serde_json::Map::new();
    for key in super::schema_facts::property_names(input_schema, &input_schema["$defs"]) {
        if let Some(value) = row.remove(key) {
            envelope.insert(key.to_owned(), value);
        }
    }
    if row.is_empty() {
        return Value::Object(envelope);
    }
    envelope.insert("queries".into(), Value::Array(vec![Value::Object(row)]));
    Value::Object(envelope)
}

/// Validates a single flat query object and returns the validated, defaulted
/// query. Strips the internal `queries.0.` path prefix from error messages so
/// callers see `field` rather than `queries.0.field`.
pub fn validate_query(tool_name: &str, query: Value) -> Result<Value, ContractValidationError> {
    let wrapped = serde_json::json!({ "queries": [query] });
    let mut validated = validate(tool_name, wrapped).map_err(strip_queries_prefix)?;
    validated["queries"]
        .as_array_mut()
        .and_then(|arr| arr.first_mut())
        .map(|q| q.take())
        .ok_or_else(|| ContractValidationError {
            issues: vec![ValidationIssue {
                rule_id: "validate.extract".to_owned(),
                path: Vec::new(),
                message: "validated queries array was empty".to_owned(),
                schema: None,
                received: None,
            }],
        })
}

fn strip_queries_prefix(mut error: ContractValidationError) -> ContractValidationError {
    for issue in &mut error.issues {
        if issue.path.first().map(String::as_str) == Some("queries")
            && issue.path.get(1).map(String::as_str) == Some("0")
        {
            issue.path.drain(..2);
        }
    }
    error
}

/// Validates a completed structured response, exactly as a client receives
/// it, against the canonical generated output contract. Validation uses a
/// clone because schema defaults, if ever introduced by the contract owner,
/// must not mutate an already produced result.
pub fn validate_output(tool_name: &str, output: &Value) -> Result<(), ContractValidationError> {
    let (_, tool) = contract_tool(tool_name)?;
    let schema = &tool["outputSchema"];
    let mut candidate = output.clone();
    validate_schema(schema, schema, &mut candidate, &mut Vec::new())?;
    #[cfg(test)]
    undeclared_check(schema, &candidate)?;
    Ok(())
}

/// Tests also reject a key the core output schema never declares: the
/// published schemas stay open, so plain validation would accept it.
#[cfg(test)]
fn undeclared_check(schema: &Value, output: &Value) -> Result<(), ContractValidationError> {
    let paths = super::undeclared::undeclared_paths(schema, output);
    if paths.is_empty() {
        return Ok(());
    }
    Err(ContractValidationError {
        issues: paths
            .into_iter()
            .map(|path| ValidationIssue {
                rule_id: "undeclaredOutputField".into(),
                path: path
                    .trim_start_matches('.')
                    .split('.')
                    .map(|segment| segment.trim_end_matches("[]").to_owned())
                    .collect(),
                message: format!("{path} is not declared by the core output schema"),
                schema: None,
                received: None,
            })
            .collect(),
    })
}

/// [`validate_output`] without the copy when validation cannot change the
/// output: the tool's output schema declares no defaults to insert. Every
/// other schema step only reads (`not` probes and union branches validate
/// their own copies), so the response stays byte-identical and is walked once.
pub(crate) fn validate_output_in_place(
    tool_name: &str,
    output: &mut Value,
) -> Result<(), ContractValidationError> {
    let (_, tool) = contract_tool(tool_name)?;
    let schema = &tool["outputSchema"];
    if declares_defaults(tool_name, schema) {
        return validate_output(tool_name, output);
    }
    validate_schema(schema, schema, output, &mut Vec::new())?;
    #[cfg(test)]
    undeclared_check(schema, output)?;
    Ok(())
}

/// Whether a tool's output schema has a `default` keyword anywhere (cached:
/// the embedded contract never changes).
fn declares_defaults(tool_name: &str, schema: &Value) -> bool {
    fn any_default(value: &Value) -> bool {
        match value {
            Value::Object(map) => map.contains_key("default") || map.values().any(any_default),
            Value::Array(items) => items.iter().any(any_default),
            _ => false,
        }
    }
    static DECLARED: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<String, bool>>,
    > = std::sync::OnceLock::new();
    let mut declared = DECLARED
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(&known) = declared.get(tool_name) {
        return known;
    }
    let found = any_default(schema);
    declared.insert(tool_name.to_owned(), found);
    found
}

type RowNormalizer = fn(&mut Value, &Value) -> Result<(), ContractValidationError>;

fn apply_normalization_rules(
    rules: &Value,
    input: &mut Value,
) -> Result<(), ContractValidationError> {
    let rules = rules
        .as_array()
        .ok_or_else(|| internal("rules must be an array".into()))?;
    for rule in rules.iter().filter(|rule| rule["phase"] == "normalize") {
        let opcode = rule["opcode"].as_str();
        let row_normalizer: Option<RowNormalizer> = match opcode {
            Some("trim_fields") => Some(trim_fields),
            Some("drop_blank_fields") => Some(drop_blank_fields),
            _ => None,
        };
        if let Some(normalize) = row_normalizer {
            if let Some(queries) = input.get_mut("queries").and_then(Value::as_array_mut) {
                for query in queries {
                    normalize(query, &rule["args"])?;
                }
            }
            continue;
        }
        match opcode {
            Some(opcode) => {
                return Err(issue(
                    "contract.unsupported-normalizer",
                    vec![],
                    format!("Unsupported normalization opcode: {opcode}"),
                ));
            }
            None => {
                return Err(internal(
                    "normalization rule opcode must be a string".into(),
                ));
            }
        }
    }
    Ok(())
}

/// The declared normalizers use JavaScript String.trim semantics, as core does.
fn ecma_trim(text: &str) -> Result<&str, ContractValidationError> {
    static SPACE: std::sync::LazyLock<Result<regress::Regex, String>> =
        std::sync::LazyLock::new(|| {
            regress::Regex::new(r"^\s$").map_err(|error| error.to_string())
        });
    let space = SPACE
        .as_ref()
        .map_err(|error| internal(format!("Invalid ECMAScript whitespace pattern: {error}")))?;
    Ok(text.trim_matches(|character: char| {
        let mut utf8 = [0; 4];
        space.find(character.encode_utf8(&mut utf8)).is_some()
    }))
}

fn trim_fields(query: &mut Value, args: &Value) -> Result<(), ContractValidationError> {
    let Some(fields) = args["fields"].as_array() else {
        return Ok(());
    };
    for field in fields.iter().filter_map(Value::as_str) {
        if let Some(array_field) = field.strip_suffix("[]") {
            if let Some(values) = query.get_mut(array_field).and_then(Value::as_array_mut) {
                for value in values {
                    if let Some(text) = value.as_str() {
                        *value = Value::String(ecma_trim(text)?.to_owned());
                    }
                }
            }
        } else if let Some(value) = query.get_mut(field)
            && let Some(text) = value.as_str()
        {
            *value = Value::String(ecma_trim(text)?.to_owned());
        }
    }
    Ok(())
}

/// Removes optional string fields that hold only whitespace: they carry no
/// information, so dropping them is lossless.
fn drop_blank_fields(query: &mut Value, args: &Value) -> Result<(), ContractValidationError> {
    let (Some(fields), Some(row)) = (args["fields"].as_array(), query.as_object_mut()) else {
        return Ok(());
    };
    for field in fields.iter().filter_map(Value::as_str) {
        if let Some(text) = row.get(field).and_then(Value::as_str)
            && ecma_trim(text)?.is_empty()
        {
            row.remove(field);
        }
    }
    Ok(())
}

fn apply_validation_rules(rules: &Value, input: &Value) -> Result<(), ContractValidationError> {
    let rules = rules
        .as_array()
        .ok_or_else(|| internal("rules must be an array".into()))?;
    let mut issues = Vec::new();
    for rule in rules.iter().filter(|rule| rule["phase"] == "validate") {
        let result = match rule["opcode"].as_str() {
            Some("content_extraction_mode") => content::validate(input, true),
            Some("content_controls") => content::validate(input, false),
            Some("artifact_registry_url") => validate_artifact_registry_url(input),
            // The query schema itself (validated before any rule) enforces these.
            Some("schema_union") | Some("json_schema") | Some("artifact_mode") => Ok(()),
            Some("github_code_search_runnable") => {
                validate_github_search_queries(input, GithubSearchKind::Code)
            }
            Some("github_repo_search_runnable") => {
                validate_github_search_queries(input, GithubSearchKind::Repositories)
            }
            Some("local_search_mode") => validate_local_search_queries(input),
            Some("history_keyword_scope") => validate_history_keyword_scope(input),
            Some("history_repository_scope") => validate_history_repository_scope(input),
            Some("qualifier_fields") => qualifiers::validate(
                input,
                rule["id"].as_str().unwrap_or("qualifier_fields"),
                &rule["args"],
            ),
            Some("lsp_rust_context") => validate_lsp_queries(input),
            Some("depth_range") => depth_range::validate(input),
            Some("structural_rule") => structural_rule::validate(input),
            Some(opcode) => {
                return Err(issue(
                    "contract.unsupported-validator",
                    vec![],
                    format!("Unsupported validation opcode: {opcode}"),
                ));
            }
            None => return Err(internal("validation rule opcode must be a string".into())),
        };
        if let Err(error) = result {
            issues.extend(error.issues);
        }
    }
    issues.dedup();
    if issues.is_empty() {
        Ok(())
    } else {
        Err(ContractValidationError { issues })
    }
}

fn query_values(input: &Value) -> impl Iterator<Item = (usize, &Value)> {
    input["queries"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
}

fn issue(
    rule_id: impl Into<String>,
    path: Vec<String>,
    message: impl Into<String>,
) -> ContractValidationError {
    ContractValidationError {
        issues: vec![ValidationIssue {
            rule_id: rule_id.into(),
            path,
            message: message.into(),
            schema: None,
            received: None,
        }],
    }
}

fn schema_issue(
    rule_id: impl Into<String>,
    path: Vec<String>,
    message: impl Into<String>,
    schema: &Value,
    received: &Value,
) -> ContractValidationError {
    let mut error = issue(rule_id, path, message);
    // A speculative probe reads only pass/fail: skip the copies.
    if !union::speculative() {
        error.issues[0].schema = Some(schema.clone());
        error.issues[0].received = Some(received.clone());
    }
    error
}

fn internal(message: String) -> ContractValidationError {
    issue("contract.generated-json", vec![], message)
}

/// The one known field whose values list the unknown field's value
/// (`{"type":"match"}` meant `operation`): the value is stronger evidence
/// than a similar spelling.
fn value_field(issue: &ValidationIssue) -> Option<&str> {
    match issue.schema.as_ref()?["valueFields"].as_array()?.as_slice() {
        [only] => only.as_str(),
        _ => None,
    }
}

/// The one known field that takes the kind of value the unknown field is
/// named for (`branch:"main"` meant `ref`).
fn kind_field(issue: &ValidationIssue) -> Option<String> {
    match issue.schema.as_ref()?["kindFields"].as_array()?.as_slice() {
        [only] => Some(only.as_str()?.to_owned()),
        _ => None,
    }
}

/// The value an unknown `true` field names (`merged:true` →
/// `state:"merged"`): the one known field whose enum lists its name.
fn name_field(issue: &ValidationIssue) -> Option<String> {
    match issue.schema.as_ref()?["nameFields"].as_array()?.as_slice() {
        [only] => Some(format!("{}:\"{}\"", only.as_str()?, issue.path.last()?)),
        _ => None,
    }
}

/// What an unknown field most likely meant: the field its value names, the
/// field its name is a kind of value of, the value its name names, else the
/// nearest known spelling.
fn field_guess(issue: &ValidationIssue, field: &str, known: &[&str]) -> Option<String> {
    value_field(issue)
        .map(str::to_owned)
        .or_else(|| kind_field(issue))
        .or_else(|| name_field(issue))
        .or_else(|| suggest_field(field, known).map(str::to_owned))
}

/// A field another form of the row declares: when the fix sets that form's
/// omitted selector (`setFields`), either fix is the caller's to choose.
fn sibling_message(issue: &ValidationIssue, field: &str, query: &str, requires: &str) -> String {
    if repository::set_fields(issue).is_some() {
        format!("'{field}' in {query} applies only with {requires}: set it, or remove '{field}'.")
    } else {
        format!("Remove '{field}' from {query}: it applies only with {requires}.")
    }
}

/// The field `known` most likely meant by `unknown`: a known field that
/// prefixes it, or the nearest spelling. The edit budget scales with the
/// name so a short guess (`depth`, `mode`) never lands on an unrelated field.
fn suggest_field<'a>(unknown: &str, known: &[&'a str]) -> Option<&'a str> {
    // `keywordsToSearch` → `keywords`: a known field that prefixes the guess.
    if let Some(prefix) = known
        .iter()
        .copied()
        .filter(|k| k.len() >= 4 && unknown.starts_with(*k) && unknown != *k)
        .max_by_key(|k| k.len())
    {
        return Some(prefix);
    }
    let lowered = unknown.to_lowercase();
    // `depth` → `maxDepth`, `isRegex` → `regex`: one name ends with the
    // other (a qualifier word added or dropped in front).
    if let Some(suffix) = known
        .iter()
        .copied()
        .filter(|k| {
            let k = k.to_lowercase();
            k != lowered
                && k.len().min(lowered.len()) >= 4
                && (k.ends_with(&lowered) || lowered.ends_with(&k))
        })
        .min_by_key(|k| k.len().abs_diff(unknown.len()))
    {
        return Some(suffix);
    }
    let max_dist = (unknown.chars().count() / 3).clamp(2, 3);
    known
        .iter()
        .filter_map(|&k| {
            let d = levenshtein(&lowered, &k.to_lowercase());
            (d <= max_dist).then_some((d, k))
        })
        .min_by_key(|(d, _)| *d)
        .map(|(_, k)| k)
}

/// Classic Wagner-Fischer Levenshtein distance, O(m*n) time, O(min(m,n)) space.
pub(crate) fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }
    let (a, b) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    let mut prev: Vec<usize> = (0..=a.len()).collect();
    let mut curr = vec![0usize; a.len() + 1];
    for (j, cb) in b.iter().enumerate() {
        curr[0] = j + 1;
        for (i, ca) in a.iter().enumerate() {
            let cost = usize::from(ca != cb);
            curr[i + 1] = (prev[i + 1] + 1).min(curr[i] + 1).min(prev[i] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[a.len()]
}

#[cfg(test)]
mod tests {
    #[test]
    fn union_envelope_normalization_preserves_response_controls() {
        let schema = serde_json::json!({"anyOf":[
            {"type":"object","properties":{"queries":{"type":"array"},"responseLength":{"type":"integer"},"responseSnapshot":{"not":{}}}},
            {"type":"object","properties":{"queries":{"not":{}},"responseSnapshot":{"type":"string"},"responseOffset":{"type":"integer"}}}
        ]});
        assert_eq!(
            super::one_row_envelope(
                &schema,
                serde_json::json!({"path":"src/a.rs","responseLength":900})
            ),
            serde_json::json!({"queries":[{"path":"src/a.rs"}],"responseLength":900})
        );
        let saved = serde_json::json!({"responseSnapshot":"saved","responseOffset":2});
        assert_eq!(super::one_row_envelope(&schema, saved.clone()), saved);
    }

    #[test]
    fn row_plan_retains_union_envelope_checks_and_separates_rows() {
        let query = serde_json::json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"],"additionalProperties":false});
        let input = serde_json::json!({"anyOf":[
            {"type":"object","properties":{"queries":{"type":"array","minItems":1,"items":query},"responseSnapshot":{"not":{}}},"required":["queries"],"additionalProperties":false},
            {"type":"object","properties":{"queries":{"not":{}},"responseSnapshot":{"type":"string","minLength":1}},"required":["responseSnapshot"],"additionalProperties":false}
        ]});
        let plan =
            super::RowPlan::derive(&serde_json::json!({"querySchema":query,"inputSchema":input}));
        assert!(plan.item_schema.is_none());
        assert!(
            plan.envelope
                .pointer("/anyOf/0/properties/queries/items")
                .is_none()
        );
        for (mut value, valid) in [
            (serde_json::json!({"queries":[{"path":"a"}]}), true),
            (serde_json::json!({"responseSnapshot":"saved"}), true),
            (
                serde_json::json!({"queries":[{"path":"a"}],"responseSnapshot":"saved"}),
                false,
            ),
            (serde_json::json!({"queries":[]}), false),
            (serde_json::json!({}), false),
        ] {
            assert_eq!(
                super::validate_input_schema(&input, &plan.envelope, &mut value, &mut Vec::new())
                    .is_ok(),
                valid,
                "{value}"
            );
        }
    }

    #[test]
    fn guessed_fields_point_at_a_field_the_schema_accepts() {
        let known = ["operation", "keywords", "contextLines", "reasoning"];
        assert_eq!(
            super::suggest_field("keywordsToSearch", &known),
            Some("keywords")
        );
        assert_eq!(super::suggest_field("operaton", &known), Some("operation"));
        assert_eq!(super::suggest_field("pattern", &known), None);
    }

    use super::{format_input_error, normalize_input, tool_error, validate};
    use crate::contracts::prepare_and_validate;
    use serde_json::{Value, json};

    #[test]
    fn tool_error_envelope_is_the_error_and_details_only_when_more() {
        assert_eq!(tool_error("bad", Vec::new()), json!({"error":"bad"}));
        assert_eq!(
            tool_error("bad", vec!["fix".into()]),
            json!({"error":"bad","details":["fix"]})
        );
    }

    #[test]
    fn pure_clasify_preserves_provider_entries_and_rejects_removed_fields() {
        let query = json!({"id":"decision","reasoning":"Decide the next evidence read.","mainGoal":"Files that decide the next read.",
            "state": {"observation": true},
            "questions":{"answer":{"type": "noul", "instructions": {"prompt":"Assess supplied state"}, "criteria":{"true":null,"false":null}}}});
        let prepared = prepare_and_validate("clasify", query.clone())
            .expect("a pure provider query needs no workflow fields");
        assert_eq!(prepared, query);
        let validated = validate("clasify", json!({"queries": [query.clone()]}))
            .expect("pure provider envelope");
        assert_eq!(validated["queries"][0], query);
        let mut padded = query.clone();
        padded["reasoning"] = json!("  Decide the next evidence read.  ");
        assert_eq!(prepare_and_validate("clasify", padded).unwrap(), query);
        for field in ["debug", "route", "sources", "resources", "carry"] {
            let mut invalid = query.clone();
            invalid[field] = json!("not part of the provider protocol");
            assert!(prepare_and_validate("clasify", invalid).is_err(), "{field}");
        }
        for field in ["state", "questions"] {
            let mut invalid = query.clone();
            invalid.as_object_mut().expect("query object").remove(field);
            assert!(
                prepare_and_validate("clasify", invalid).is_err(),
                "missing {field}"
            );
        }
        let mut blank_reasoning = query.clone();
        blank_reasoning["reasoning"] = json!("");
        let dropped =
            prepare_and_validate("clasify", blank_reasoning).expect("blank reasoning is dropped");
        assert!(dropped.get("reasoning").is_none());
    }

    #[test]
    fn validates_local_fetch_and_applies_schema_defaults() {
        let output = validate(
            "localFetch",
            json!({"queries":[{"path":"/tmp/a","mainGoal":"Read the fixture.","reasoning":"The next step needs these lines."}]}),
        )
        .expect("valid query");
        assert_eq!(output["queries"][0]["mainGoal"], "Read the fixture.");
        assert!(
            validate(
                "localFetch",
                json!({"queries":[{"path":"/tmp/a","reasoning":"The next step needs these lines."}]}),
            )
            .is_ok(),
            "mainGoal is optional"
        );
        assert!(
            validate(
                "localFetch",
                json!({"queries":[{"path":"/tmp/a","mainGoal":"Read the fixture."}]}),
            )
            .is_ok(),
            "reasoning is optional"
        );
    }

    #[test]
    fn rejects_local_fetch_relations_and_unknown_fields() {
        let relation = validate(
            "localFetch",
            json!({"queries":[{"path":"/tmp/a","fullContent":true,"length":2,"mainGoal": "test", "reasoning":"Read the complete fixture."}]}),
        )
        .expect_err("invalid relation");
        // `length` is a valid window control, mutually exclusive with
        // fullContent, so the relation is rejected as a field conflict rather
        // than an unknown field.
        assert_eq!(
            format_input_error("localFetch", &relation),
            json!({
                "error":"queries.0.length: length windows a read; fullContent reads the whole file. Remove length."
            })
        );
        assert!(
            validate(
                "localFetch",
                json!({"queries":[{"path":"/tmp/a","wat":true,"mainGoal": "test", "reasoning":"Exercise unknown-field validation."}]})
            )
            .is_err()
        );
    }

    #[test]
    fn aggregates_schema_violations_across_fields_and_queries() {
        let error = validate(
            "localFetch",
            json!({
                "queries": [
                    {"wat": true, "alsoWat": 1},
                    {"path": 7, "offset": "bad", "fullContent": "bad"}
                ]
            }),
        )
        .expect_err("invalid fields");
        let keys = error
            .issues
            .iter()
            .map(|issue| (issue.rule_id.as_str(), issue.path.join(".")))
            .collect::<Vec<_>>();
        assert!(keys.contains(&("schema.unknown-field", "queries.0.wat".into())));
        assert!(keys.contains(&("schema.unknown-field", "queries.0.alsoWat".into())));
        assert!(keys.contains(&("schema.required", "queries.0.path".into())));
        assert!(keys.contains(&("schema.type", "queries.1.path".into())));
        assert!(keys.contains(&("schema.type", "queries.1.offset".into())));
        assert!(keys.contains(&("schema.type", "queries.1.fullContent".into())));
    }

    #[test]
    fn formats_stable_cli_input_errors() {
        // A malformed value is echoed beside the form it must take.
        let range = validate(
            "localFetch",
            json!({"queries":[{"path":"/tmp/a","ranges":["5:2"],"mainGoal": "test", "reasoning":"Exercise range validation."}]}),
        )
        .expect_err("range");
        assert_eq!(
            format_input_error("localFetch", &range),
            json!({"error":"queries.0.ranges.0: \"5:2\" is not a line range; ranges: [\"299-342\"] (strings \"start-end\")"})
        );
        // A span spelled another way is named as the span it spells.
        let typo = validate(
            "localFetch",
            json!({"queries":[{"path":"/tmp/a","ranges":["3:6"]}]}),
        )
        .expect_err("range typo");
        assert_eq!(
            format_input_error("localFetch", &typo),
            json!({"error":"queries.0.ranges.0: \"3:6\" is not a line range (did you mean \"3-6\"?); ranges: [\"299-342\"] (strings \"start-end\")"})
        );
        // A blank brief carries nothing: it is dropped, not rejected.
        let blank = validate(
            "localFetch",
            json!({"queries":[
                {"path":"/tmp/a","mainGoal":"ok","reasoning":"ok"},
                {"path":"/tmp/a","mainGoal":"  ","reasoning":""}]}),
        )
        .expect("blank brief is dropped");
        assert!(blank["queries"][1].get("mainGoal").is_none());
        assert!(blank["queries"][1].get("reasoning").is_none());
        let blank_path = validate(
            "localFetch",
            json!({"queries":[{"path":" ","mainGoal":"ok","reasoning":"ok"}]}),
        )
        .expect_err("blank path");
        assert_eq!(
            format_input_error("localFetch", &blank_path),
            json!({"error":"queries.0.path: is empty; give non-blank text."})
        );
        let unknown = validate(
            "localFetch",
            json!({"queries":[{"path":"/tmp/a","madeUp":true,"mainGoal": "test", "reasoning":"Exercise unknown-field validation."}]}),
        )
        .expect_err("unknown");
        assert_eq!(
            format_input_error("localFetch", &unknown),
            json!({"error":"Remove unknown field(s) from queries[0]: madeUp"})
        );
    }

    /// A `length` above the window cap names the cap and the same read at it,
    /// and says where the rest is.
    #[test]
    fn a_length_over_the_cap_names_the_cap_and_the_fix() {
        for (unit, length) in [("bytes", 60_000), ("lines", 50_001)] {
            let error = validate(
                "localFetch",
                json!({"queries":[{"path":"/tmp/a","unit":unit,"length":length,"mainGoal":"test","reasoning":"Exercise the length cap."}]}),
            )
            .expect_err("length over the cap");
            let shaped = format_input_error("localFetch", &error);
            let text = shaped["error"].as_str().expect("error text");
            assert!(text.contains("allowed range (1-50000)"), "{shaped}");
            let lead = shaped["details"][0].as_str().expect("a lead");
            assert!(lead.contains("\"length\": 50000"), "{lead}");
            assert!(lead.contains("next"), "{lead}");
        }
    }

    /// An unknown key inside an `astSearch` rule object shows one valid
    /// rule, as the YAML-rule error does.
    #[test]
    fn an_unknown_rule_key_shows_one_valid_rule() {
        let unknown = validate(
            "astSearch",
            json!({"queries":[{"operation":"match","path":"/tmp","rule":{"kind":"function_item","has":{"constraints":{}}},"mainGoal":"test","reasoning":"Exercise rule validation."}]}),
        )
        .expect_err("unknown rule key");
        let error = format_input_error("astSearch", &unknown);
        let text = error["error"].as_str().expect("error text").to_owned();
        assert!(text.contains("queries[0].rule.has"), "{text}");
        assert!(
            text.contains(&format!(
                "Example rule: {}",
                octocode_engine::structural::EXAMPLE_RULE
            )),
            "{text}"
        );
        let plain = validate(
            "localFetch",
            json!({"queries":[{"path":"/tmp/a","madeUp":true,"mainGoal":"t","reasoning":"Exercise unknown-field validation."}]}),
        )
        .expect_err("unknown");
        assert!(
            !format_input_error("localFetch", &plain)
                .to_string()
                .contains("Example rule")
        );
    }

    #[test]
    fn input_errors_carry_no_static_schema_note() {
        let unknown = validate(
            "localFetch",
            json!({"queries":[
                {"path":"/tmp/a","mainGoal":"test","reasoning":"Valid row."},
                {"path":"/tmp/a","madeUp":true,"mainGoal":"test","reasoning":"Unknown field."}
            ]}),
        )
        .expect_err("unknown");
        // The per-call repair is the whole error: no CLI `schema` pointer and
        // no inputSchema pointer (that note lives once in the instructions).
        let formatted = format_input_error("localFetch", &unknown);
        let text = formatted.to_string();
        assert!(!text.contains("octocode schema"), "{text}");
        assert!(!text.contains("inputSchema"), "{text}");
        assert!(formatted.get("details").is_none(), "{formatted}");
        assert!(
            formatted["error"]
                .as_str()
                .is_some_and(|error| error.contains("queries[1]") && error.contains("madeUp")),
            "{formatted}"
        );
    }

    #[test]
    fn blank_optional_source_selectors_survive_normalization_and_fail_validation() {
        let github = json!({"queries":[{
            "owner":"octocat","repo":"Hello-World","path":"README.md",
            "mainGoal":"Read the requested source version.",
            "reasoning":"Keep the caller's source selector exact."
        }]});
        let package = json!({"queries":[{
            "ecosystem":"npm","packageName":"zod",
            "mainGoal":"Read the requested package version.",
            "reasoning":"Keep the caller's source selector exact."
        }]});
        for (tool, mut input, field, value) in [
            ("ghGetFileContent", github.clone(), "ref", json!("  ")),
            ("ghGetFileContent", github, "ranges", json!([])),
            ("artifactSearch", package, "version", json!(" ")),
        ] {
            input["queries"][0][field] = value.clone();
            let normalized = normalize_input(tool, input);
            assert_eq!(
                normalized["queries"][0][field], value,
                "{tool}.{field}: {normalized}"
            );
            let error =
                validate(tool, normalized).expect_err("blank selector must fail before dispatch");
            assert!(
                error
                    .issues
                    .iter()
                    .any(|issue| issue.path.join(".").ends_with(field)),
                "{tool}.{field}: {error:?}"
            );
        }
    }

    #[test]
    fn unrepairable_list_values_get_honest_array_hints() {
        let reject = |include: Value| {
            let error = validate(
                "localSearch",
                json!({"queries":[{"path":"/tmp","matchString":"x","include":include,"mainGoal":"test","reasoning":"Hint."}]}),
            )
            .expect_err("not an array");
            error.issues[0].message.clone()
        };
        assert_eq!(
            reject(json!("[\"*.go\"")),
            "Expected array; send a JSON array, not a JSON-encoded string"
        );
        assert_eq!(reject(json!({"a":1})), "Expected array");
    }

    #[test]
    fn names_the_selector_a_sibling_branch_needs_for_a_rejected_literal() {
        let error = validate(
            "localSearch",
            json!({"queries":[{"path":"/tmp","matchString":"foo","unique":"list","mainGoal": "test", "reasoning":"List values."}]}),
        )
        .expect_err("unique:list needs matchOnly");
        let formatted = format_input_error("localSearch", &error);
        assert!(
            formatted["error"].as_str().is_some_and(
                |error| error.contains("unique:\"list\" requires resultView:\"matchOnly\"")
            ),
            "{formatted}"
        );
    }

    /// The MCP surface renders the same guidance from core Zod issues; the
    /// MCP parity suite runs these inputs through both surfaces.
    #[test]
    fn guidance_names_every_form_alias_and_core_required_message() {
        let details = |tool: &str, mut row: Value| {
            row["mainGoal"] = json!("g");
            row["reasoning"] = json!("r");
            let error = validate(tool, json!({"queries":[row]})).expect_err(tool);
            let formatted = format_input_error(tool, &error);
            // The first repair is the error; any others follow in details.
            std::iter::once(&formatted["error"])
                .chain(formatted["details"].as_array().into_iter().flatten())
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join("\n")
        };
        let symbols = details(
            "astSearch",
            json!({"operation":"symbols","path":"/r","include":["*.ts"]}),
        );
        // `include` belongs to the match operation: adding a pattern to a
        // symbols query would not make it valid, so the operation is named.
        assert!(
            symbols.contains("it applies only with operation:\"match\"."),
            "{symbols}"
        );
        assert!(!symbols.contains("localSearch"), "in-tool field: {symbols}");
        let owner = details("ghSearchCode", json!({"keywords":["x"]}));
        assert!(
            owner.contains(
                "Missing required field: owner (Set owner: code search cannot span all of GitHub"
            ),
            "{owner}"
        );
        let history = details(
            "ghSearchHistory",
            json!({"operation":"pullRequest","owner":"a","repo":"b","qualifers":"author:x"}),
        );
        assert!(history.contains("did you mean 'qualifiers'?"), "{history}");
    }

    #[test]
    fn names_the_mode_of_a_field_declared_by_a_sibling_branch() {
        let error = validate(
            "artifactSearch",
            json!({"queries":[{"ecosystem":"npm","packageName":"zod","pageSize":3,"mainGoal": "test", "reasoning":"Exact lookup."}]}),
        )
        .expect_err("pageSize is discovery-only");
        let formatted = format_input_error("artifactSearch", &error);
        assert_eq!(
            formatted,
            json!({"error":"Remove 'pageSize' from queries[0]: it applies only with keywords."})
        );
    }

    #[test]
    fn names_operations_when_no_branch_shares_the_query_mode() {
        let query = json!({
            "mainGoal":"g","reasoning":"r","operation":"issue",
            "owner":"o","repo":"r","number":1,"matchString":"thanks"
        });
        let error = validate("ghGetHistoryItem", json!({ "queries": [query] }))
            .expect_err("issues take no matchString");
        let formatted = format_input_error("ghGetHistoryItem", &error);
        let message = formatted["error"].as_str().unwrap_or_default();
        assert!(
            message.starts_with(
                "Remove 'matchString' from queries[0]: it applies only with operation one of"
            ),
            "{formatted}"
        );
        for operation in ["pullRequest", "commit", "compare"] {
            assert!(message.contains(operation), "{formatted}");
        }
    }

    #[test]
    fn names_the_operation_that_declares_a_field_of_another_operation() {
        let query = json!({
            "mainGoal":"g","reasoning":"r","operation":"commit",
            "owner":"octocat","repo":"Hello-World","state":"open"
        });
        let error = validate("ghSearchHistory", json!({ "queries": [query] }))
            .expect_err("the field belongs to another operation");
        let formatted = format_input_error("ghSearchHistory", &error);
        assert_eq!(
            formatted["error"],
            "Remove 'state' from queries[0]: it applies only with operation one of \"pullRequest\", \"issue\".",
            "{formatted}"
        );
    }

    #[test]
    fn offers_either_fix_for_a_field_of_clasify_sections() {
        let legacy = json!({
            "id":"legacy",
            "resources":[{"id":"evidence","value":"Supplied evidence."}],
            "questions":[{"id":"bounded","type":"yesno","ask":"Is evidence supplied?"}]
        });
        let error = validate("clasify", json!({"queries":[legacy]}))
            .expect_err("resources belongs to the sections form");
        assert_eq!(
            format_input_error("clasify", &error),
            json!({
                "error":"'resources' in queries[0] applies only with operation:\"sections\": set it, or remove 'resources'.",
                "details":[
                    "queries.0.state: Missing required field: state",
                    "queries.0.questions: Expected object"
                ]
            })
        );
    }

    /// An unknown field whose value is one of a field's listed values names
    /// that field: `{"analysis":"dependents"}` meant `operation`.
    #[test]
    fn an_unknown_field_holding_a_listed_value_suggests_its_field() {
        for (tool, row) in [
            (
                "structureSearch",
                json!({"type":"tree","path":".","mainGoal":"g","reasoning":"r"}),
            ),
            (
                "astSearch",
                json!({"analysis":"symbols","path":".","mainGoal":"g","reasoning":"r"}),
            ),
        ] {
            let error = validate(tool, json!({"queries":[row]})).expect_err("rejected");
            let formatted = format_input_error(tool, &error).to_string();
            assert!(
                formatted.contains("did you mean 'operation'?"),
                "{tool}: {formatted}"
            );
        }
    }

    #[test]
    fn suggests_reasoning_for_a_typo_across_every_tool_shape() {
        // The enforcement IR carries no presentation examples; the accepted
        // parity fixtures are the generated per-tool query corpus instead.
        let fixtures: Value =
            serde_json::from_str(crate::contracts::generated::CONTRACT_FIXTURES_JSON)
                .expect("generated fixtures");
        let contract = crate::contracts::parsed_contract().expect("generated contract");
        for tool in contract["tools"].as_array().expect("tool array") {
            let name = tool["name"].as_str().expect("tool name");
            let mut query = fixtures
                .as_array()
                .expect("fixture array")
                .iter()
                .find(|fixture| {
                    fixture["tool"] == *name && fixture["accepted"] == Value::Bool(true)
                })
                .expect("accepted fixture for every tool")["input"]["queries"][0]
                .clone();
            let object = query.as_object_mut().expect("query example");
            object.remove("reasoning");
            object.insert("reasonng".into(), json!("Exercise typo recovery."));
            let error =
                validate(name, json!({"queries":[query]})).expect_err("typo must be rejected");
            let formatted = format_input_error(name, &error).to_string();
            assert!(
                formatted.contains("did you mean 'reasoning'?"),
                "{name}: {formatted}"
            );
        }
    }

    /// Admission accepts exactly the canonical input language: a value the
    /// published schema rejects is rejected with a repair hint, never
    /// repaired (no string-to-boolean/number, enum casing or plurals,
    /// encoded or bare-scalar lists, or line-range spellings).
    #[test]
    fn admission_rejects_what_the_canonical_schema_rejects() {
        let admit = |tool: &str, row: Value| {
            crate::contracts::prepare_many_and_validate(
                tool,
                normalize_input(tool, json!({"queries":[row]})),
            )
        };
        let rejected = [
            (
                "localFetch",
                json!({"path":"a.rs","fullContent":"true"}),
                "send true",
            ),
            (
                "localFetch",
                json!({"path":"a.rs","minify":"none"}),
                "allowed:",
            ),
            (
                "localFetch",
                json!({"path":"a.rs","contextLines":"3"}),
                "send 3",
            ),
            (
                "localFetch",
                json!({"path":"a.rs","ranges":[" 140-150"]}),
                "ranges",
            ),
            (
                "localFetch",
                json!({"path":"a.rs","ranges":["190"]}),
                "ranges",
            ),
            (
                "localFetch",
                json!({"path":"a.rs","ranges":[["248","325"]]}),
                "ranges",
            ),
            (
                "localFetch",
                json!({"path":"a.rs","ranges":"[\"1-5\"]"}),
                "JSON-encoded",
            ),
            (
                "localSearch",
                json!({"path":".","matchString":"x","include":"[\"*.go\"]"}),
                "JSON-encoded",
            ),
            (
                "localSearch",
                json!({"path":".","matchString":"x","include":"*.go"}),
                "wrap the value",
            ),
            (
                "ghSearchHistory",
                json!({"operation":"pullRequests","owner":"o","repo":"r"}),
                "operation",
            ),
        ];
        for (tool, row, hint) in rejected {
            let error = admit(tool, row.clone()).expect_err(&format!("{tool} {row}"));
            let formatted = format_input_error(tool, &error).to_string();
            assert!(formatted.contains(hint), "{tool} {row}: {formatted}");
        }
        let accepted = [
            ("localFetch", json!({"path":"a.rs","fullContent":true})),
            ("localFetch", json!({"path":"a.rs","minify":"symbols"})),
            ("localFetch", json!({"path":"a.rs","ranges":["140-150"]})),
            (
                "localSearch",
                json!({"path":".","matchString":"x","include":["*.go"]}),
            ),
        ];
        for (tool, row) in accepted {
            admit(tool, row.clone()).unwrap_or_else(|error| panic!("{tool} {row}: {error:?}"));
        }
    }

    /// `localSearch` takes `localFetch`'s `matchString` form: one text or a
    /// list of 1 to 10 non-empty texts that match any.
    #[test]
    fn local_search_match_string_is_one_text_or_a_bounded_list() {
        let admit = |match_string: Value| {
            crate::contracts::prepare_many_and_validate(
                "localSearch",
                normalize_input(
                    "localSearch",
                    json!({"queries":[{"path":".","matchString":match_string}]}),
                ),
            )
        };
        let many = |count: usize| (0..count).map(|n| format!("v{n}")).collect::<Vec<_>>();
        for accepted in [json!("x"), json!(["a", "b"]), json!(many(10))] {
            admit(accepted.clone()).unwrap_or_else(|error| panic!("{accepted}: {error:?}"));
        }
        for rejected in [json!(""), json!([]), json!(["a", ""]), json!(many(11))] {
            assert!(admit(rejected.clone()).is_err(), "{rejected}");
        }
    }

    #[test]
    fn matches_generated_reference_corpus() {
        let fixtures: Value =
            serde_json::from_str(crate::contracts::generated::CONTRACT_FIXTURES_JSON)
                .expect("generated fixture JSON");
        // Clasify's batch relation rules run at engine admission; its
        // fixtures are checked there (`tools::clasify::admission`).
        for fixture in fixtures.as_array().expect("fixture array") {
            if fixture["tool"] == "clasify" {
                continue;
            }
            let tool = fixture["tool"].as_str().expect("tool");
            let result =
                crate::contracts::prepare_many_and_validate(tool, fixture["input"].clone());
            assert_eq!(
                result.is_ok(),
                fixture["accepted"].as_bool().expect("accepted"),
                "{}: {result:?}",
                fixture["id"],
            );
            if let Ok(rows) = result {
                let normalized = super::validate(tool, fixture["input"].clone())
                    .expect("accepted fixture validates as a complete envelope");
                assert_eq!(normalized, fixture["normalized"], "{}", fixture["id"]);
                let expected_rows = fixture["normalized"]
                    .get("queries")
                    .and_then(Value::as_array)
                    .map_or(&[][..], Vec::as_slice);
                assert_eq!(rows, expected_rows, "{}", fixture["id"]);
            }
        }
    }

    /// A guessed field points at the accepted field the schema names, never
    /// at an unrelated near-spelling.
    #[test]
    fn guessed_field_names_suggest_the_accepted_field() {
        let local_search = ["matchString", "include", "regex", "includeHidden", "page"];
        assert_eq!(
            super::suggest_field("includes", &local_search),
            Some("include")
        );
        assert_eq!(super::suggest_field("regexp", &local_search), Some("regex"));
        assert_eq!(super::suggest_field("mode", &local_search), None);
        let structure = ["path", "maxDepth", "debug", "include", "detail"];
        assert_eq!(
            super::suggest_field("maxdepth", &structure),
            Some("maxDepth")
        );
        assert_eq!(
            super::suggest_field("depth", &["debug", "detail"]),
            None,
            "a short name never matches an unrelated field three edits away"
        );
        // One name ends with the other: a qualifier word added or dropped.
        assert_eq!(super::suggest_field("depth", &structure), Some("maxDepth"));
        assert_eq!(
            super::suggest_field("isRegex", &local_search),
            Some("regex")
        );
        assert_eq!(super::suggest_field("filePattern", &local_search), None);
    }

    /// No legacy spelling maps to a current field: `uri` is simply unknown.
    #[test]
    fn a_retired_field_name_has_no_alias() {
        let lsp_search = ["operation", "path", "symbolName", "line", "graphDepth"];
        assert_eq!(super::suggest_field("uri", &lsp_search), None);
    }

    #[test]
    fn a_row_sent_to_the_wrong_tool_names_the_tool_that_owns_its_fields() {
        let error = validate(
            "localFetch",
            json!({"queries":[{"path":"package.json","matchString":"name","multiline":"on","resultView":"files","mainGoal":"g","reasoning":"r"}]}),
        )
        .expect_err("localSearch fields");
        let formatted = format_input_error("localFetch", &error).to_string();
        assert!(
            formatted.contains("multiline, resultView are localSearch fields"),
            "{formatted}"
        );

        let error = validate(
            "structureSearch",
            json!({"queries":[{"path":"packages","depth":2,"mainGoal":"g","reasoning":"r"}]}),
        )
        .expect_err("misspelled maxDepth");
        let formatted = format_input_error("structureSearch", &error).to_string();
        assert!(
            formatted.contains("did you mean 'maxDepth'?"),
            "{formatted}"
        );
        assert!(
            !formatted.contains("send queries[0] to"),
            "a field the tool can name is not rerouted: {formatted}"
        );

        let error = validate(
            "astSearch",
            json!({"queries":[{"path":"packages","operation":"files","pageSize":5,"mainGoal":"g","reasoning":"r"}]}),
        )
        .expect_err("structureSearch operation");
        let formatted = format_input_error("astSearch", &error).to_string();
        assert!(
            formatted.contains("operation \\\"files\\\" is a structureSearch operation"),
            "{formatted}"
        );
    }

    #[test]
    fn an_oversized_batch_says_how_to_split_it() {
        let rows = (0..7)
            .map(|_| json!({"path":"package.json","mainGoal":"g","reasoning":"r"}))
            .collect::<Vec<_>>();
        let error = validate("localFetch", json!({ "queries": rows })).expect_err("too many rows");
        let formatted = format_input_error("localFetch", &error).to_string();
        assert!(
            formatted.contains("split the batch into 2 calls"),
            "{formatted}"
        );
    }

    #[test]
    fn a_top_level_brief_is_moved_into_each_row() {
        let error = crate::contracts::prepare_many_and_validate(
            "localSearch",
            json!({"mainGoal":"g","queries":[{"path":".","matchString":"x","mainGoal":"g","reasoning":"r"}]}),
        )
        .expect_err("mainGoal is per row");
        let formatted = format_input_error("localSearch", &error).to_string();
        assert!(
            formatted.contains("Move 'mainGoal' into each queries[] row"),
            "{formatted}"
        );
    }
}
