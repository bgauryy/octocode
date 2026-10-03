mod artifact;
mod ast_rewrite;
mod coerce;
mod content;
mod defaults;
mod github_search;
mod history;
mod local_search;
mod lsp;
mod qualifiers;
pub(crate) use qualifiers::qualifier_terms;
mod schema;
mod topology;
mod union;
use artifact::validate_artifact_queries;
use ast_rewrite::{validate_ast_rewrite_queries, validate_ast_rewrite_rules};
use defaults::apply_observed_defaults;
use github_search::{GithubSearchKind, validate_github_search_queries};
use history::{
    validate_history_content_selection, validate_history_keyword_scope,
    validate_history_repository_scope,
};
use local_search::validate_local_search_queries;
use lsp::{validate_lsp_queries, validate_operation_controls};
use schema::validate_schema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt::{Display, Formatter};
use topology::validate_topology_queries;

use crate::tools::id::ToolId;

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
/// `inputSchema`; only the CLI has the `scheme` command.
#[must_use]
pub fn format_input_error(tool_name: &str, error: &ContractValidationError, mcp: bool) -> Value {
    let unknown = error
        .issues
        .iter()
        .filter(|issue| issue.rule_id == "schema.unknown-field")
        .collect::<Vec<_>>();
    if !unknown.is_empty() && unknown.len() == error.issues.len() {
        let mut fields = unknown
            .iter()
            .filter_map(|issue| issue.path.last())
            .cloned()
            .collect::<Vec<_>>();
        fields.sort();
        fields.dedup();
        let mut details = Vec::new();
        for issue in &unknown {
            let query = query_label(&issue.path);
            let field = issue.path.last().map(String::as_str).unwrap_or("unknown");
            // Extract known fields from the embedded schema so we can suggest the
            // closest match (edit distance <= 3) as a "did you mean ...?" hint.
            let known: Vec<&str> = issue
                .schema
                .as_ref()
                .and_then(|s| s["knownFields"].as_array())
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str())
                .collect();
            let msg = match (sibling_requirement(issue), suggest_field(field, &known)) {
                (Some(requires), _) => {
                    format!("Remove '{field}' from {query}: it applies only with {requires}.")
                }
                _ if is_root_brief(&issue.path) => root_brief_message(field),
                (None, Some(s)) => {
                    format!("Remove unknown field '{field}' from {query} (did you mean '{s}'?)")
                }
                (None, None) => format!("Remove unknown field(s) from {query}: {field}"),
            };
            details.push(msg);
        }
        details.extend(routing_details(tool_name, &error.issues));
        details.push(if mcp {
            format!("See the {tool_name} inputSchema for valid fields.")
        } else {
            format!("Run scheme {tool_name} --view query --compact to see valid fields.")
        });
        return serde_json::json!({"kind":"octocode.toolError","version":1,"tool":tool_name,"error":format!("Unknown field(s): {}", fields.join(", ")),"details":details});
    }
    let details = error
        .issues
        .iter()
        .map(|issue| {
            let path = issue.path.join(".");
            // For unknown-field issues in the mixed-error path, append a suggestion too.
            if issue.rule_id == "schema.unknown-field" {
                let field = issue.path.last().map(String::as_str).unwrap_or("unknown");
                let known: Vec<&str> = issue
                    .schema
                    .as_ref()
                    .and_then(|s| s["knownFields"].as_array())
                    .into_iter()
                    .flatten()
                    .filter_map(|v| v.as_str())
                    .collect();
                if is_root_brief(&issue.path) && sibling_requirement(issue).is_none() {
                    return root_brief_message(field);
                }
                let base = if path.is_empty() {
                    issue.message.clone()
                } else {
                    format!("{path}: {}", issue.message)
                };
                return match suggest_field(field, &known) {
                    Some(s) => format!("{base} (did you mean '{s}'?)"),
                    None => base,
                };
            }
            let message = match required_guidance(tool_name, issue) {
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
    serde_json::json!({"kind":"octocode.toolError","version":1,"tool":tool_name,"error":"Check the query fields.","details":details})
}

/// Core-authored guidance for a missing required field (the core schema's
/// required-field error), rendered after `Missing required field: <name>` as
/// MCP renders the core message.
fn required_guidance(tool_name: &str, issue: &ValidationIssue) -> Option<&'static str> {
    if issue.rule_id != "schema.required" {
        return None;
    }
    Some(match (tool_name, issue.path.last()?.as_str()) {
        ("ghSearchCode", "owner") => {
            "Set owner: code search cannot span all of GitHub (add repo to narrow further)."
        }
        ("localFetch", "path") => "Set path to a local file.",
        ("ghGetFileContent", "path") => "Set path to a repository-relative file.",
        _ => return None,
    })
}

/// `goal`/`reasoning` sent beside `queries` instead of inside each row.
fn is_root_brief(path: &[String]) -> bool {
    matches!(path, [field] if field == "goal" || field == "reasoning")
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
                let in_tool =
                    suggest_field(field, &known).is_some() || sibling_requirement(issue).is_some();
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

/// The named tool's embedded contract entry, or the unknown-tool issue.
fn contract_tool(tool_name: &str) -> Result<&'static Value, ContractValidationError> {
    super::tool_contract_named(tool_name)
        .ok_or_else(|| {
            issue(
                "contract.unknown-tool",
                vec![],
                format!("Unknown tool: {tool_name}"),
            )
        })?
        .map_err(|error| internal(error.to_string()))
}

/// Validates and applies JSON-Schema defaults from the generated canonical
/// contract. Runtime-only relation rules are applied after structural parsing.
pub fn validate(tool_name: &str, mut input: Value) -> Result<Value, ContractValidationError> {
    let tool = contract_tool(tool_name)?;
    apply_normalization_rules(&tool["rules"], &mut input)?;
    let mut issues = Vec::new();
    let item_schema = tool["inputSchema"]
        .pointer("/properties/queries/items")
        .unwrap_or(&tool["querySchema"]);
    if let Some(queries) = input.get_mut("queries").and_then(Value::as_array_mut) {
        let typed = [
            (&tool["querySchema"], &tool["querySchema"]),
            (&tool["inputSchema"], item_schema),
        ];
        for (index, query) in queries.iter_mut().enumerate() {
            coerce::coerce_lossless(&typed, query);
            apply_observed_defaults(&tool["defaults"], query);
            let mut path = vec!["queries".to_owned(), index.to_string()];
            let parsed =
                validate_schema(&tool["querySchema"], &tool["querySchema"], query, &mut path)
                    .and_then(|()| {
                        validate_schema(&tool["inputSchema"], item_schema, query, &mut path)
                    });
            if let Err(error) = parsed {
                issues.extend(error.issues);
                continue;
            }
            let scoped = serde_json::json!({"queries":[query]});
            if let Err(mut error) = apply_validation_rules(&tool["rules"], &scoped) {
                for issue in &mut error.issues {
                    if issue.path.first().is_some_and(|part| part == "queries")
                        && issue.path.get(1).is_some_and(|part| part == "0")
                    {
                        issue.path[1] = index.to_string();
                    }
                }
                issues.extend(error.issues);
            }
        }
    }
    if !issues.is_empty() {
        issues.dedup();
        return Err(ContractValidationError { issues });
    }
    validate_schema(
        &tool["inputSchema"],
        &tool["inputSchema"],
        &mut input,
        &mut Vec::new(),
    )?;
    Ok(input)
}

/// Lossless, schema-driven input repair shared by every host before
/// validation: a JSON-encoded `queries` array, JSON-encoded or bare-scalar
/// values in list-only fields, and exact integer/boolean strings in
/// integer/boolean-only fields (see `coerce`). The input keeps its shape (flat
/// query, query array, or envelope); nothing is validated or defaulted, and
/// an unknown tool is returned unchanged.
#[must_use]
pub fn normalize_input(tool_name: &str, mut input: Value) -> Value {
    let Some(Ok(tool)) = super::tool_contract_named(tool_name) else {
        return input;
    };
    let item_schema = tool["inputSchema"]
        .pointer("/properties/queries/items")
        .unwrap_or(&tool["querySchema"]);
    let typed = [
        (&tool["querySchema"], &tool["querySchema"]),
        (&tool["inputSchema"], item_schema),
    ];
    let envelope = input.get("queries").is_some();
    if envelope {
        coerce::coerce_lossless(&[(&tool["inputSchema"], &tool["inputSchema"])], &mut input);
    }
    let rows: Vec<&mut Value> = match &mut input {
        Value::Object(object) if envelope => object
            .get_mut("queries")
            .and_then(Value::as_array_mut)
            .map(|queries| queries.iter_mut().collect())
            .unwrap_or_default(),
        Value::Array(queries) => queries.iter_mut().collect(),
        flat => vec![flat],
    };
    for row in rows {
        coerce::coerce_lossless(&typed, row);
    }
    input
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

/// Validates a completed structured response against the canonical generated
/// output contract. Validation uses a clone because schema defaults, if ever
/// introduced by the contract owner, must not mutate an already produced result.
pub fn validate_output(tool_name: &str, output: &Value) -> Result<(), ContractValidationError> {
    let tool = contract_tool(tool_name)?;
    let schema = &tool["outputSchema"];
    let mut candidate = output.clone();
    crate::runtime::response::restore_shared_fields(&mut candidate);
    validate_schema(schema, schema, &mut candidate, &mut Vec::new())
}

fn apply_normalization_rules(
    rules: &Value,
    input: &mut Value,
) -> Result<(), ContractValidationError> {
    let rules = rules
        .as_array()
        .ok_or_else(|| internal("rules must be an array".into()))?;
    for rule in rules.iter().filter(|rule| rule["phase"] == "normalize") {
        match rule["opcode"].as_str() {
            Some("trim_fields") => trim_fields(input, &rule["args"]),
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

fn trim_fields(input: &mut Value, args: &Value) {
    let Some(fields) = args["fields"].as_array() else {
        return;
    };
    let Some(queries) = input.get_mut("queries").and_then(Value::as_array_mut) else {
        return;
    };
    for query in queries {
        for field in fields.iter().filter_map(Value::as_str) {
            if let Some(array_field) = field.strip_suffix("[]") {
                if let Some(values) = query.get_mut(array_field).and_then(Value::as_array_mut) {
                    for value in values {
                        if let Some(text) = value.as_str() {
                            *value = Value::String(text.trim().to_owned());
                        }
                    }
                }
            } else if let Some(value) = query.get_mut(field)
                && let Some(text) = value.as_str()
            {
                *value = Value::String(text.trim().to_owned());
            }
        }
    }
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
            Some("artifact_mode") | Some("artifact_registry_url") => {
                validate_artifact_queries(input)
            }
            Some("schema_union") | Some("json_schema") => Ok(()),
            Some("github_code_search_runnable") => {
                validate_github_search_queries(input, GithubSearchKind::Code)
            }
            Some("github_repo_search_runnable") => {
                validate_github_search_queries(input, GithubSearchKind::Repositories)
            }
            Some("ast_rewrite_apply") => validate_ast_rewrite_queries(input),
            Some("local_search_mode") => validate_local_search_queries(input),
            Some("ast_topology") => validate_topology_queries(input),
            Some("history_keyword_scope") => validate_history_keyword_scope(input),
            Some("history_repository_scope") => validate_history_repository_scope(input),
            Some("qualifier_fields") => qualifiers::validate(
                input,
                rule["id"].as_str().unwrap_or("qualifier_fields"),
                &rule["args"],
            ),
            Some("lsp_rust_context") => validate_lsp_queries(input),
            Some("operation_controls") => validate_operation_controls(
                input,
                rule["id"].as_str().unwrap_or("operation_controls"),
                &rule["args"],
            ),
            Some("ast_rewrite_rule") => validate_ast_rewrite_rules(input),
            Some("history_content_selection") => validate_history_content_selection(input),
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
    error.issues[0].schema = Some(schema.clone());
    error.issues[0].received = Some(received.clone());
    error
}

fn internal(message: String) -> ContractValidationError {
    issue("contract.generated-json", vec![], message)
}

/// Legacy or commonly guessed field names agents send (observed in blind
/// evals and recorded sessions), mapped to the canonical field when the query
/// accepts it. A name may map to several fields; the first one the query
/// accepts wins.
const FIELD_ALIASES: [(&str, &str); 23] = [
    ("type", "operation"),
    ("path", "uri"),
    ("filePath", "uri"),
    ("keywordsToSearch", "keywords"),
    ("matchStringContextLines", "contextLines"),
    ("pattern", "searchText"),
    ("pattern", "names"),
    ("filesOnly", "resultView"),
    ("filePath", "path"),
    ("maxResults", "pageSize"),
    ("limit", "pageSize"),
    ("depth", "maxDepth"),
    ("lineStart", "startLine"),
    ("lineEnd", "endLine"),
    ("searchText", "matchString"),
    ("filePattern", "include"),
    ("fileFilter", "include"),
    ("includePattern", "include"),
    ("glob", "include"),
    ("useRegex", "regex"),
    ("isRegex", "regex"),
    ("includeHidden", "hidden"),
    ("showHidden", "hidden"),
];

/// The field `known` most likely meant by `unknown`: an alias, a known field
/// that prefixes it, or the nearest spelling. The edit budget scales with the
/// name so a short guess (`depth`, `mode`) never lands on an unrelated field.
fn suggest_field<'a>(unknown: &str, known: &[&'a str]) -> Option<&'a str> {
    let accepted = |name: &str| known.iter().copied().find(|k| *k == name);
    if let Some(target) = FIELD_ALIASES
        .iter()
        .filter(|(alias, _)| *alias == unknown)
        .find_map(|(_, target)| accepted(target))
    {
        return Some(target);
    }
    // `keywordsToSearch` → `keywords`: a known field that prefixes the guess.
    if let Some(prefix) = known
        .iter()
        .copied()
        .filter(|k| k.len() >= 4 && unknown.starts_with(*k) && unknown != *k)
        .max_by_key(|k| k.len())
    {
        return Some(prefix);
    }
    let max_dist = (unknown.chars().count() / 3).clamp(2, 3);
    let lowered = unknown.to_lowercase();
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
    fn guessed_legacy_fields_point_at_the_accepted_field() {
        let known = ["operation", "keywords", "contextLines", "reasoning"];
        assert_eq!(super::suggest_field("type", &known), Some("operation"));
        assert_eq!(
            super::suggest_field("keywordsToSearch", &known),
            Some("keywords")
        );
        assert_eq!(
            super::suggest_field("matchStringContextLines", &known),
            Some("contextLines")
        );
        assert_eq!(
            super::suggest_field("pattern", &known),
            None,
            "searchText not accepted here"
        );
    }

    use super::{format_input_error, normalize_input, validate};
    use crate::contracts::{PrepareOptions, prepare_and_validate};
    use serde_json::{Value, json};

    #[test]
    fn pure_clasify_requires_correlation_and_preserves_provider_entries() {
        let query = json!({"id":"decision","reasoning":"Decide the next evidence read.","goal":"Files that decide the next read.","resources":[{"id":"source","context": {"value": {"observation": true}},"maxChars":80000}], "questions":[{"id":"answer",
            "type": "noul", "instructions": {"prompt":"Assess supplied state"}, "criteria":{"true":null,"false":null}
        }]});
        let prepared = prepare_and_validate("clasify", query.clone(), PrepareOptions::default())
            .expect("pure semantic query needs no workflow fields");
        assert_eq!(prepared, query);
        let validated = validate("clasify", json!({"queries": [query.clone()]}))
            .expect("pure semantic envelope");
        assert_eq!(validated["queries"][0], query);
        let mut padded = query.clone();
        padded["reasoning"] = json!("  Decide the next evidence read.  ");
        assert_eq!(
            prepare_and_validate("clasify", padded, PrepareOptions::default()).unwrap(),
            query
        );
        for field in ["model", "debug", "route", "sources"] {
            let mut invalid = query.clone();
            invalid[field] = json!("not part of the pure protocol");
            assert!(
                prepare_and_validate("clasify", invalid, PrepareOptions::default()).is_err(),
                "{field}"
            );
        }
        for field in ["resources", "questions"] {
            let mut invalid = query.clone();
            invalid.as_object_mut().expect("query object").remove(field);
            assert!(
                prepare_and_validate("clasify", invalid, PrepareOptions::default()).is_err(),
                "missing {field}"
            );
        }
        let mut blank_reasoning = query.clone();
        blank_reasoning["reasoning"] = json!("");
        assert!(
            prepare_and_validate("clasify", blank_reasoning, PrepareOptions::default()).is_err(),
            "blank reasoning is rejected"
        );
        let mut missing_goal = query.clone();
        missing_goal
            .as_object_mut()
            .expect("query object")
            .remove("goal");
        assert!(
            prepare_and_validate("clasify", missing_goal, PrepareOptions::default()).is_err(),
            "missing goal"
        );
    }

    #[test]
    fn validates_local_fetch_and_applies_schema_defaults() {
        let output = validate(
            "localFetch",
            json!({"queries":[{"path":"/tmp/a","goal":"Read the fixture.","reasoning":"The next step needs these lines."}]}),
        )
        .expect("valid query");
        assert_eq!(output["queries"][0]["goal"], "Read the fixture.");
        assert!(
            validate(
                "localFetch",
                json!({"queries":[{"path":"/tmp/a","reasoning":"The next step needs these lines."}]}),
            )
            .is_err(),
            "missing goal"
        );
        assert!(
            validate(
                "localFetch",
                json!({"queries":[{"path":"/tmp/a","goal":"Read the fixture."}]}),
            )
            .is_err(),
            "missing reasoning"
        );
    }

    #[test]
    fn rejects_local_fetch_relations_and_unknown_fields() {
        let relation = validate(
            "localFetch",
            json!({"queries":[{"path":"/tmp/a","fullContent":true,"chunkSize":2,"goal": "test", "reasoning":"Read the complete fixture."}]}),
        )
        .expect_err("invalid relation");
        // `chunkSize` is a valid chunk control, mutually exclusive with
        // fullContent, so the relation is rejected as a field conflict rather
        // than an unknown field.
        assert_eq!(
            format_input_error("localFetch", &relation, false),
            json!({
                "kind":"octocode.toolError", "version":1, "tool":"localFetch",
                "error":"Check the query fields.",
                "details":[
                    "queries.0.fullContent: Choose fullContent or chunk controls."
                ]
            })
        );
        assert!(
            validate(
                "localFetch",
                json!({"queries":[{"path":"/tmp/a","wat":true,"goal": "test", "reasoning":"Exercise unknown-field validation."}]})
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
                    {"path": 7, "startLine": "bad", "fullContent": "bad"}
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
        assert!(keys.contains(&("schema.type", "queries.1.startLine".into())));
        assert!(keys.contains(&("schema.type", "queries.1.fullContent".into())));
    }

    #[test]
    fn formats_stable_cli_input_errors() {
        let range = validate(
            "localFetch",
            json!({"queries":[{"path":"/tmp/a","startLine":5,"endLine":2,"goal": "test", "reasoning":"Exercise range validation."}]}),
        )
        .expect_err("range");
        assert_eq!(
            format_input_error("localFetch", &range, false),
            json!({
                "kind":"octocode.toolError","version":1,"tool":"localFetch","error":"Check the query fields.",
                "details":["queries.0.endLine: Set endLine greater than or equal to startLine."]
            })
        );
        // A blank brief names the field and the fix, not a regex.
        let blank = validate(
            "localFetch",
            json!({"queries":[
                {"path":"/tmp/a","goal":"ok","reasoning":"ok"},
                {"path":"/tmp/a","goal":"  ","reasoning":""}]}),
        )
        .expect_err("blank brief");
        assert_eq!(
            format_input_error("localFetch", &blank, false)["details"],
            json!([
                "queries.1.goal: is empty; give one line on what to find or decide (goal and reasoning are required on every query).",
                "queries.1.reasoning: is empty; give one line on why this query (goal and reasoning are required on every query)."
            ])
        );
        let blank_path = validate(
            "localFetch",
            json!({"queries":[{"path":" ","goal":"ok","reasoning":"ok"}]}),
        )
        .expect_err("blank path");
        assert_eq!(
            format_input_error("localFetch", &blank_path, false)["details"],
            json!(["queries.0.path: is empty; give non-blank text."])
        );
        let unknown = validate(
            "localFetch",
            json!({"queries":[{"path":"/tmp/a","madeUp":true,"goal": "test", "reasoning":"Exercise unknown-field validation."}]}),
        )
        .expect_err("unknown");
        assert_eq!(
            format_input_error("localFetch", &unknown, false),
            json!({
                "kind":"octocode.toolError","version":1,"tool":"localFetch","error":"Unknown field(s): madeUp",
                "details":["Remove unknown field(s) from queries[0]: madeUp", "Run scheme localFetch --view query --compact to see valid fields."]
            })
        );
    }

    #[test]
    fn mcp_input_errors_point_at_the_tool_schema_not_the_cli() {
        let unknown = validate(
            "localFetch",
            json!({"queries":[
                {"path":"/tmp/a","goal":"test","reasoning":"Valid row."},
                {"path":"/tmp/a","madeUp":true,"goal":"test","reasoning":"Unknown field."}
            ]}),
        )
        .expect_err("unknown");
        let formatted = format_input_error("localFetch", &unknown, true);
        let details = formatted["details"].to_string();
        assert!(!details.contains("scheme"), "{details}");
        assert!(details.contains("localFetch inputSchema"), "{details}");
        assert!(details.contains("queries[1]"), "{details}");
    }

    #[test]
    fn lossless_numeric_and_boolean_strings_coerce_only_for_typed_fields() {
        let accepted = validate(
            "localFetch",
            json!({"queries":[
                {"path":"/tmp/a","startLine":"2","endLine":"10","goal":"test","reasoning":"Coerce."},
                {"path":"/tmp/a","fullContent":"false","goal":"test","reasoning":"Coerce."}
            ]}),
        )
        .expect("lossless strings coerce");
        assert_eq!(accepted["queries"][0]["startLine"], 2);
        assert_eq!(accepted["queries"][0]["endLine"], 10);
        assert_eq!(accepted["queries"][1]["fullContent"], false);
        let search = validate(
            "localSearch",
            json!({"queries":[{"path":"/tmp","searchText":"10","pageSize":"5","goal":"test","reasoning":"Coerce."}]}),
        )
        .expect("string fields keep their string");
        assert_eq!(search["queries"][0]["searchText"], "10");
        assert_eq!(search["queries"][0]["pageSize"], 5);
        let union = validate(
            "lspSearch",
            json!({"queries":[
                {"uri":"/tmp/a.rs","position":{"line":"3","character":"0"},"goal":"test","reasoning":"Coerce."},
                {"uri":"/tmp/a.rs","symbolName":"main","lineHint":"4","goal":"test","reasoning":"Coerce."}
            ]}),
        )
        .expect("union branches coerce their own typed fields");
        assert_eq!(union["queries"][0]["position"]["line"], 3);
        assert_eq!(union["queries"][0]["position"]["character"], 0);
        assert_eq!(union["queries"][1]["lineHint"], 4);
        for bad in [
            "02",
            "2.0",
            "1e3",
            " 2",
            "+2",
            "-0",
            "",
            "9007199254740993",
            "two",
        ] {
            assert!(
                validate(
                    "localFetch",
                    json!({"queries":[{"path":"/tmp/a","startLine":bad,"endLine":10,"goal":"test","reasoning":"No coercion."}]}),
                )
                .is_err(),
                "{bad:?} must not coerce"
            );
        }
        for bad in ["TRUE", "True", "1", "yes", " true"] {
            assert!(
                validate(
                    "localFetch",
                    json!({"queries":[{"path":"/tmp/a","fullContent":bad,"goal":"test","reasoning":"No coercion."}]}),
                )
                .is_err(),
                "{bad:?} must not coerce"
            );
        }
    }

    #[test]
    fn list_fields_sent_json_encoded_or_bare_are_repaired_before_validation() {
        let brief = json!({"goal":"test","reasoning":"Repair host encodings."});
        let row = |extra: Value| {
            let mut row = brief.clone();
            row.as_object_mut()
                .expect("row")
                .extend(extra.as_object().expect("extra").clone());
            row
        };
        let search = row(json!({"path":"/tmp","searchText":"x",
            "include":"[\"*.go\"]","exclude":"[\"*_test.go\"]",
            "excludeDir":"[\"node_modules\",\"dist\"]","contextLines":"2"}));
        let normalized = normalize_input(
            "localSearch",
            json!({"queries": serde_json::to_string(&json!([search])).expect("encode")}),
        );
        let query = &normalized["queries"][0];
        assert_eq!(query["include"], json!(["*.go"]));
        assert_eq!(query["exclude"], json!(["*_test.go"]));
        assert_eq!(query["excludeDir"], json!(["node_modules", "dist"]));
        assert_eq!(query["contextLines"], 2);
        validate("localSearch", normalized).expect("repaired input validates");

        let structure = normalize_input(
            "structureSearch",
            row(json!({"path":"/tmp","extensions":"[\"go\"]"})),
        );
        assert_eq!(structure["extensions"], json!(["go"]));
        let code = normalize_input(
            "ghSearchCode",
            json!({"queries":[row(json!({"owner":"o","keywords":"wrap_app_handling_exceptions"}))]}),
        );
        assert_eq!(
            code["queries"][0]["keywords"],
            json!(["wrap_app_handling_exceptions"])
        );
        let rows = normalize_input(
            "ghSearchCode",
            json!([row(json!({"owner":"o","keywords":"k"}))]),
        );
        assert_eq!(rows[0]["keywords"], json!(["k"]));

        // A field that also accepts a string keeps it; unknown tools pass through.
        let lsp = normalize_input(
            "lspSearch",
            row(
                json!({"uri":"/tmp/a.rs","symbolName":"main","lineHint":1,"rustContext":{"features":"all"}}),
            ),
        );
        assert_eq!(lsp["rustContext"]["features"], "all");
        let untouched = json!({"queries":"[\"*.go\"]"});
        assert_eq!(normalize_input("noSuchTool", untouched.clone()), untouched);
    }

    /// localFetch and ghGetFileContent accept host spellings of line ranges
    /// on every path (CLI and MCP both normalize through here).
    #[test]
    fn line_range_spellings_validate_after_normalization() {
        for (tool, extra) in [
            ("localFetch", json!({"path":"/tmp/a.rs"})),
            (
                "ghGetFileContent",
                json!({"owner":"o","repo":"r","path":"a.rs"}),
            ),
        ] {
            for (ranges, expected) in [
                (json!("70,130"), json!(["70-130"])),
                (json!([" 140-150"]), json!(["140-150"])),
                (json!(["248", "325"]), json!(["248-325"])),
                (json!([248, 325]), json!(["248-325"])),
            ] {
                let mut row = json!({"goal":"g","reasoning":"r","ranges":ranges});
                row.as_object_mut()
                    .expect("row")
                    .extend(extra.as_object().expect("extra").clone());
                let normalized = normalize_input(tool, json!({"queries":[row]}));
                assert_eq!(normalized["queries"][0]["ranges"], expected, "{tool}");
                validate(tool, normalized).expect("repaired ranges validate");
            }
        }
    }

    #[test]
    fn unrepairable_list_values_get_honest_array_hints() {
        let reject = |include: Value| {
            let error = validate(
                "localSearch",
                json!({"queries":[{"path":"/tmp","searchText":"x","include":include,"goal":"test","reasoning":"Hint."}]}),
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
            json!({"queries":[{"path":"/tmp","searchText":"foo","unique":"list","goal": "test", "reasoning":"List values."}]}),
        )
        .expect_err("unique:list needs matchOnly");
        let formatted = format_input_error("localSearch", &error, false);
        assert!(
            formatted["details"][0].as_str().is_some_and(
                |detail| detail.contains("unique:\"list\" requires resultView:\"matchOnly\"")
            ),
            "{formatted}"
        );
    }

    /// The MCP surface renders the same guidance from core Zod issues; the
    /// MCP parity suite runs these inputs through both surfaces.
    #[test]
    fn guidance_names_every_form_alias_and_core_required_message() {
        let details = |tool: &str, mut row: Value| {
            row["goal"] = json!("g");
            row["reasoning"] = json!("r");
            let error = validate(tool, json!({"queries":[row]})).expect_err(tool);
            format_input_error(tool, &error, true)["details"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join("\n")
        };
        let symbols = details(
            "astSearch",
            json!({"operation":"symbols","path":"/r","include":["*.ts"]}),
        );
        assert!(
            symbols.contains("it applies only with pattern or rule."),
            "{symbols}"
        );
        assert!(!symbols.contains("localSearch"), "in-tool field: {symbols}");
        let lsp = details(
            "lspSearch",
            json!({"path":"a.ts","symbolName":"x","lineHint":1}),
        );
        assert!(lsp.contains("did you mean 'uri'?"), "{lsp}");
        let owner = details("ghSearchCode", json!({"keywords":["x"]}));
        assert!(
            owner.contains(
                "Missing required field: owner (Set owner: code search cannot span all of GitHub"
            ),
            "{owner}"
        );
        let history = details(
            "ghSearchHistory",
            json!({"operation":"pullRequest","owner":"a","repo":"b","mergedAt":"2026-01-01"}),
        );
        assert!(history.contains("did you mean 'merged-at'?"), "{history}");
    }

    #[test]
    fn names_the_mode_of_a_field_declared_by_a_sibling_branch() {
        let error = validate(
            "artifactSearch",
            json!({"queries":[{"type":"npm","packageName":"zod","pageSize":3,"goal": "test", "reasoning":"Exact lookup."}]}),
        )
        .expect_err("pageSize is discovery-only");
        let formatted = format_input_error("artifactSearch", &error, false);
        assert_eq!(
            formatted["error"], "Unknown field(s): pageSize",
            "{formatted}"
        );
        assert_eq!(
            formatted["details"][0],
            "Remove 'pageSize' from queries[0]: it applies only with keywords."
        );
    }

    #[test]
    fn names_the_operation_that_declares_a_field_of_another_operation() {
        for (field, value, expected) in [
            (
                "review",
                json!("approved"),
                "Remove 'review' from queries[0]: it applies only with operation:\"pullRequest\".",
            ),
            (
                "state",
                json!("open"),
                "Remove 'state' from queries[0]: it applies only with operation:\"pullRequest\" or operation:\"issue\".",
            ),
        ] {
            let mut query = json!({
                "goal":"g","reasoning":"r","operation":"commit",
                "owner":"octocat","repo":"Hello-World"
            });
            query[field] = value;
            let error = validate("ghSearchHistory", json!({ "queries": [query] }))
                .expect_err("the field belongs to another operation");
            let formatted = format_input_error("ghSearchHistory", &error, false);
            assert_eq!(formatted["details"][0], expected, "{formatted}");
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
                .expect("accepted fixture for every tool")["input"]
                .clone();
            let object = query.as_object_mut().expect("query example");
            object.remove("reasoning");
            object.insert("reasonng".into(), json!("Exercise typo recovery."));
            let error =
                validate(name, json!({"queries":[query]})).expect_err("typo must be rejected");
            let formatted = format_input_error(name, &error, false).to_string();
            assert!(
                formatted.contains("did you mean 'reasoning'?"),
                "{name}: {formatted}"
            );
        }
    }

    /// Core authors the wording of cross-field clasify rejections; native
    /// must render the same text for the same input (fixture `messages`).
    #[test]
    fn renders_core_rejection_wording_for_parity_fixtures() {
        let fixtures: Value =
            serde_json::from_str(crate::contracts::generated::CONTRACT_FIXTURES_JSON)
                .expect("generated fixture JSON");
        let mut checked = 0;
        for fixture in fixtures.as_array().expect("fixture array") {
            let Some(expected) = fixture.get("messages") else {
                continue;
            };
            let error = crate::contracts::prepare_many_and_validate(
                fixture["tool"].as_str().expect("tool"),
                fixture["input"].clone(),
                PrepareOptions {
                    source_label: "fixture",
                },
            )
            .expect_err("a fixture with messages is rejected");
            let messages = error
                .issues
                .iter()
                .map(|issue| Value::String(issue.message.clone()))
                .collect::<Vec<_>>();
            assert_eq!(&Value::Array(messages), expected, "{}", fixture["id"]);
            checked += 1;
        }
        assert!(checked >= 5, "clasify wording fixtures: {checked}");
    }

    #[test]
    fn matches_generated_reference_corpus() {
        let fixtures: Value =
            serde_json::from_str(crate::contracts::generated::CONTRACT_FIXTURES_JSON)
                .expect("generated fixture JSON");
        for fixture in fixtures.as_array().expect("fixture array") {
            let result = prepare_and_validate(
                fixture["tool"].as_str().expect("tool"),
                fixture["input"].clone(),
                PrepareOptions {
                    source_label: "fixture",
                },
            );
            assert_eq!(
                result.is_ok(),
                fixture["accepted"].as_bool().expect("accepted"),
                "{}: {result:?}",
                fixture["id"],
            );
            if let Ok(normalized) = result {
                let expected = fixture["normalized"]
                    .get("queries")
                    .and_then(Value::as_array)
                    .and_then(|queries| queries.first())
                    .unwrap_or(&fixture["normalized"]);
                assert_eq!(&normalized, expected, "{}", fixture["id"]);
            }
        }
    }

    /// Field names agents sent in real sessions point at the accepted field,
    /// never at an unrelated near-spelling.
    #[test]
    fn observed_wrong_field_names_suggest_the_accepted_field() {
        let local_fetch = [
            "path",
            "startLine",
            "endLine",
            "matchString",
            "contextLines",
        ];
        assert_eq!(
            super::suggest_field("lineStart", &local_fetch),
            Some("startLine")
        );
        assert_eq!(
            super::suggest_field("lineEnd", &local_fetch),
            Some("endLine")
        );
        assert_eq!(
            super::suggest_field("searchText", &local_fetch),
            Some("matchString")
        );
        let local_search = ["searchText", "include", "regex", "hidden", "page", "goal"];
        for (guess, field) in [
            ("filePattern", "include"),
            ("fileFilter", "include"),
            ("includePattern", "include"),
            ("useRegex", "regex"),
            ("isRegex", "regex"),
            ("includeHidden", "hidden"),
        ] {
            assert_eq!(
                super::suggest_field(guess, &local_search),
                Some(field),
                "{guess}"
            );
        }
        assert_eq!(
            super::suggest_field("mode", &local_search),
            None,
            "not 'goal'"
        );
        let structure = ["path", "maxDepth", "debug", "names", "detail"];
        assert_eq!(super::suggest_field("depth", &structure), Some("maxDepth"));
        assert_eq!(super::suggest_field("pattern", &structure), Some("names"));
        assert_eq!(
            super::suggest_field("depth", &["debug", "detail"]),
            None,
            "a short name never matches an unrelated field three edits away"
        );
    }

    #[test]
    fn a_row_sent_to_the_wrong_tool_names_the_tool_that_owns_its_fields() {
        let error = validate(
            "localFetch",
            json!({"queries":[{"path":"package.json","searchText":"name","pageSize":3,"goal":"g","reasoning":"r"}]}),
        )
        .expect_err("localSearch fields");
        let formatted = format_input_error("localFetch", &error, false).to_string();
        assert!(
            formatted.contains("did you mean 'matchString'?"),
            "{formatted}"
        );
        assert!(
            formatted.contains("pageSize, searchText are localSearch fields"),
            "{formatted}"
        );

        let error = validate(
            "structureSearch",
            json!({"queries":[{"path":"packages","depth":2,"goal":"g","reasoning":"r"}]}),
        )
        .expect_err("misspelled maxDepth");
        let formatted = format_input_error("structureSearch", &error, false).to_string();
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
            json!({"queries":[{"path":"packages","operation":"files","pageSize":5,"goal":"g","reasoning":"r"}]}),
        )
        .expect_err("structureSearch operation");
        let formatted = format_input_error("astSearch", &error, false).to_string();
        assert!(
            formatted.contains("operation \\\"files\\\" is a structureSearch operation"),
            "{formatted}"
        );
    }

    #[test]
    fn an_oversized_batch_says_how_to_split_it() {
        let rows = (0..7)
            .map(|_| json!({"path":"package.json","goal":"g","reasoning":"r"}))
            .collect::<Vec<_>>();
        let error = validate("localFetch", json!({ "queries": rows })).expect_err("too many rows");
        let formatted = format_input_error("localFetch", &error, false).to_string();
        assert!(
            formatted.contains("split the batch into 2 calls"),
            "{formatted}"
        );
    }

    #[test]
    fn a_top_level_brief_is_moved_into_each_row() {
        let error = crate::contracts::prepare_many_and_validate(
            "localSearch",
            json!({"goal":"g","queries":[{"path":".","searchText":"x","goal":"g","reasoning":"r"}]}),
            PrepareOptions { source_label: "test" },
        )
        .expect_err("goal is per row");
        let formatted = format_input_error("localSearch", &error, false).to_string();
        assert!(
            formatted.contains("Move 'goal' into each queries[] row"),
            "{formatted}"
        );
    }
}
