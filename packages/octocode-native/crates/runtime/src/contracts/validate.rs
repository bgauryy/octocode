mod content;
mod schema;
mod union;
use regex::Regex;
use schema::validate_schema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt::{Display, Formatter};
use url::Url;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractValidationError {
    pub issues: Vec<ValidationIssue>,
}

/// Projects transport-neutral validation issues into the stable CLI tool-error
/// envelope used by the native CLI and thin adapters.
#[must_use]
pub fn format_input_error(tool_name: &str, error: &ContractValidationError) -> Value {
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
            let query = issue
                .path
                .get(1)
                .and_then(|v| v.parse::<usize>().ok())
                .map_or(1, |v| v + 1);
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
            let suggestion = suggest_field(field, &known);
            let msg = match suggestion {
                Some(s) => format!(
                    "Remove unknown field '{field}' from query {query} (did you mean '{s}'?)"
                ),
                None => format!("Remove unknown field(s) from query {query}: {field}"),
            };
            details.push(msg);
        }
        details.push(format!(
            "Run scheme {tool_name} --view query --compact to see valid fields."
        ));
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
            if path.is_empty() {
                issue.message.clone()
            } else {
                format!("{path}: {}", issue.message)
            }
        })
        .collect::<Vec<_>>();
    serde_json::json!({"kind":"octocode.toolError","version":1,"tool":tool_name,"error":"Check the query fields.","details":details})
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

/// Validates and applies JSON-Schema defaults from the generated canonical
/// contract. Runtime-only relation rules are applied after structural parsing.
pub fn validate(tool_name: &str, mut input: Value) -> Result<Value, ContractValidationError> {
    let contract = super::parsed_contract().map_err(|error| internal(error.to_string()))?;
    let tool = contract["tools"]
        .as_array()
        .and_then(|tools| tools.iter().find(|tool| tool["name"] == tool_name))
        .ok_or_else(|| {
            issue(
                "contract.unknown-tool",
                vec![],
                format!("Unknown tool: {tool_name}"),
            )
        })?;
    apply_normalization_rules(&tool["rules"], &mut input)?;
    let mut issues = Vec::new();
    let item_schema = tool["inputSchema"]
        .pointer("/properties/queries/items")
        .unwrap_or(&tool["querySchema"]);
    if let Some(queries) = input.get_mut("queries").and_then(Value::as_array_mut) {
        for (index, query) in queries.iter_mut().enumerate() {
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
    let contract = super::parsed_contract().map_err(|error| internal(error.to_string()))?;
    let tool = contract["tools"]
        .as_array()
        .and_then(|tools| tools.iter().find(|tool| tool["name"] == tool_name))
        .ok_or_else(|| {
            issue(
                "contract.unknown-tool",
                vec![],
                format!("Unknown tool: {tool_name}"),
            )
        })?;
    let schema = &tool["outputSchema"];
    let mut candidate = output.clone();
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
            Some("github_search_runnable") => validate_github_search_queries(input),
            Some("ast_rewrite_apply") => validate_ast_rewrite_queries(input),
            Some("local_search_mode") => validate_local_search_queries(input),
            Some("ast_topology") => validate_topology_queries(input),
            Some("history_keyword_scope") => validate_history_keyword_scope(input),
            Some("lsp_rust_context") => validate_lsp_queries(input),
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

fn validate_history_content_selection(input: &Value) -> Result<(), ContractValidationError> {
    for (index, query) in query_values(input) {
        let Some(patches) = query.pointer("/content/patches").and_then(Value::as_object) else {
            continue;
        };
        let selected = patches.get("mode").and_then(Value::as_str) == Some("selected");
        let nonempty = |field: &str| {
            patches
                .get(field)
                .and_then(Value::as_array)
                .is_some_and(|v| !v.is_empty())
        };
        let has_selection = nonempty("files") || nonempty("ranges");
        if selected && !has_selection {
            return Err(issue(
                "history.content-selection",
                vec![
                    "queries".into(),
                    index.to_string(),
                    "content".into(),
                    "patches".into(),
                    "files".into(),
                ],
                "selected patch mode requires non-empty files or ranges",
            ));
        }
        if !selected && has_selection {
            return Err(issue(
                "history.content-selection",
                vec![
                    "queries".into(),
                    index.to_string(),
                    "content".into(),
                    "patches".into(),
                    "mode".into(),
                ],
                "patch files and ranges require selected mode",
            ));
        }
    }
    Ok(())
}

fn validate_ast_rewrite_rules(input: &Value) -> Result<(), ContractValidationError> {
    const RULE_FIELDS: [&str; 12] = [
        "pattern", "kind", "regex", "inside", "has", "precedes", "follows", "all", "any", "not",
        "matches", "stopBy",
    ];
    fn check_rule(rule: &Value, path: &mut Vec<String>) -> Result<(), ContractValidationError> {
        let Some(object) = rule.as_object() else {
            return Ok(());
        };
        if object.keys().all(|key| key == "stopBy") {
            return Err(issue(
                "ast-rewrite.rule-matcher",
                path.clone(),
                "rule must contain at least one matcher",
            ));
        }
        for field in ["inside", "has", "precedes", "follows", "not"] {
            if let Some(child) = object.get(field) {
                path.push(field.into());
                let result = check_rule(child, path);
                path.pop();
                result?;
            }
        }
        for field in ["all", "any"] {
            if let Some(children) = object.get(field).and_then(Value::as_array) {
                for (index, child) in children.iter().enumerate() {
                    path.extend([field.into(), index.to_string()]);
                    let result = check_rule(child, path);
                    path.truncate(path.len() - 2);
                    result?;
                }
            }
        }
        if let Some(child) = object.get("stopBy").filter(|v| v.is_object()) {
            path.push("stopBy".into());
            let result = check_rule(child, path);
            path.pop();
            result?;
        }
        Ok(())
    }
    for (index, query) in query_values(input) {
        let mut roots: Vec<(&str, &Value)> = Vec::new();
        if let Some(rule) = query.get("rule").filter(|v| v.is_object()) {
            roots.push(("rule", rule));
        }
        for field in ["constraints", "utils"] {
            if let Some(values) = query.get(field).and_then(Value::as_object) {
                roots.extend(values.values().map(|v| (field, v)));
            }
        }
        for (field, rule) in roots {
            if rule
                .as_object()
                .is_some_and(|o| o.keys().all(|k| RULE_FIELDS.contains(&k.as_str())))
            {
                check_rule(
                    rule,
                    &mut vec!["queries".into(), index.to_string(), field.into()],
                )?;
            }
        }
        if query.get("ruleKind").and_then(Value::as_str) == Some("experimental") {
            let has_rewrite = query
                .get("transform")
                .and_then(Value::as_object)
                .is_some_and(|values| {
                    values
                        .values()
                        .any(|v| v.as_object().is_some_and(|o| o.contains_key("rewrite")))
                });
            if !has_rewrite {
                return Err(issue(
                    "ast-rewrite.experimental-rewrite",
                    vec!["queries".into(), index.to_string(), "transform".into()],
                    "experimental rules require a rewrite transformation",
                ));
            }
        }
    }
    Ok(())
}

fn query_values(input: &Value) -> impl Iterator<Item = (usize, &Value)> {
    input["queries"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
}

fn validate_history_keyword_scope(input: &Value) -> Result<(), ContractValidationError> {
    for (index, query) in query_values(input) {
        let has_keywords = query
            .get("keywords")
            .and_then(Value::as_array)
            .is_some_and(|v| !v.is_empty());
        if !has_keywords {
            continue;
        }
        if query.get("operation").and_then(Value::as_str) != Some("commit") {
            continue;
        }
        for field in ["path", "branch", "base", "head", "includeDiff"] {
            if query.get(field).is_some_and(|v| v != &Value::Bool(false)) {
                return Err(issue(
                    "history.keyword-scope",
                    vec!["queries".into(), index.to_string(), field.into()],
                    format!("Commit-message keywords cannot be combined with {field}"),
                ));
            }
        }
    }
    Ok(())
}

fn validate_topology_queries(input: &Value) -> Result<(), ContractValidationError> {
    for (index, query) in query_values(input) {
        let operation = query.get("operation").and_then(Value::as_str);
        if !matches!(
            operation,
            Some("dependencies" | "dependents" | "path" | "reachability" | "cycles" | "deadCode")
        ) {
            continue;
        }
        let absolute = |value: Option<&Value>| {
            value.and_then(Value::as_str).is_some_and(|s| {
                s.starts_with('/')
                    || s.starts_with("\\\\")
                    || (s.len() > 2
                        && s.as_bytes()[1] == b':'
                        && matches!(s.as_bytes()[2], b'/' | b'\\'))
            })
        };
        let rooted = query
            .get("path")
            .and_then(Value::as_str)
            .is_some_and(|s| !s.is_empty())
            || absolute(query.get("file"))
            || absolute(query.get("target"))
            || query
                .get("entrypoints")
                .and_then(Value::as_array)
                .is_some_and(|values| values.iter().any(|v| absolute(Some(v))));
        if !rooted {
            return Err(issue(
                "ast-search.topology",
                vec!["queries".into(), index.to_string(), "path".into()],
                "path is required unless an absolute path can infer the repository root",
            ));
        }
    }
    Ok(())
}

fn validate_lsp_queries(input: &Value) -> Result<(), ContractValidationError> {
    for (index, query) in query_values(input) {
        let prefix = |field: &str| vec!["queries".into(), index.to_string(), field.into()];
        if let Some(context) = query.get("rustContext").and_then(Value::as_object) {
            if context.get("procMacros") == Some(&Value::Bool(true))
                && context.get("buildScripts") != Some(&Value::Bool(true))
            {
                return Err(issue(
                    "lsp.proc-macros",
                    prefix("rustContext"),
                    "procMacros requires buildScripts:true",
                ));
            }
            if !query
                .get("uri")
                .and_then(Value::as_str)
                .is_some_and(|uri| uri.to_lowercase().ends_with(".rs"))
            {
                return Err(issue(
                    "lsp.rust-uri",
                    prefix("rustContext"),
                    "rustContext requires a Rust .rs uri",
                ));
            }
        }
        let operation = query.get("operation").and_then(Value::as_str).unwrap_or("");
        if operation == "workspaceSymbol" {
            if query.get("position").is_some() || query.get("lineHint").is_some() {
                return Err(issue(
                    "lsp.workspace-anchor",
                    prefix("position"),
                    "workspaceSymbol does not accept a position or lineHint",
                ));
            }
            if query
                .get("symbolName")
                .and_then(Value::as_str)
                .is_none_or(str::is_empty)
            {
                return Err(issue(
                    "lsp.workspace-symbol",
                    prefix("symbolName"),
                    "Set symbolName for workspaceSymbol",
                ));
            }
            if query.get("uri").is_none() && query.get("workspaceRoot").is_none() {
                return Err(issue(
                    "lsp.workspace-root",
                    prefix("workspaceRoot"),
                    "Set uri or workspaceRoot for workspaceSymbol",
                ));
            }
            continue;
        }
        if query.get("uri").is_none() {
            return Err(issue(
                "lsp.uri",
                prefix("uri"),
                "Set uri for file-scoped operations",
            ));
        }
        if matches!(operation, "documentSymbols" | "diagnostic") {
            continue;
        }
        let has_name = query
            .get("symbolName")
            .and_then(Value::as_str)
            .is_some_and(|v| !v.is_empty())
            || query.get("lineHint").is_some();
        let has_position = query.get("position").is_some();
        if has_name && has_position {
            return Err(issue(
                "lsp.anchor-exclusive",
                prefix("position"),
                "Use either symbolName+lineHint or position, not both",
            ));
        }
        if !has_position
            && query
                .get("symbolName")
                .and_then(Value::as_str)
                .is_none_or(str::is_empty)
        {
            return Err(issue(
                "lsp.symbol-anchor",
                prefix("symbolName"),
                "Set symbolName for anchored operations",
            ));
        }
        if !has_position && query.get("lineHint").and_then(Value::as_i64).is_none() {
            return Err(issue(
                "lsp.line-anchor",
                prefix("lineHint"),
                "Set lineHint for anchored operations",
            ));
        }
    }
    Ok(())
}

fn validate_local_search_queries(input: &Value) -> Result<(), ContractValidationError> {
    let two_dollar_meta = Regex::new(r"(?:^|[^$])\$\$[A-Z_][A-Z0-9_]*")
        .map_err(|error| internal(error.to_string()))?;
    for (index, query) in query_values(input) {
        let prefix = |field: &str| vec!["queries".into(), index.to_string(), field.into()];
        if query.get("mode").and_then(Value::as_str) != Some("structural") {
            if query.get("pattern").is_some() || query.get("rule").is_some() {
                return Err(issue(
                    "local-search.lexical-structural-field",
                    prefix(if query.get("pattern").is_some() {
                        "pattern"
                    } else {
                        "rule"
                    }),
                    "pattern and rule require structural mode",
                ));
            }
            if query
                .get("searchText")
                .and_then(Value::as_str)
                .is_none_or(str::is_empty)
            {
                return Err(issue(
                    "local-search.search-text",
                    prefix("searchText"),
                    "searchText is required unless structural mode",
                ));
            }
            let is_match_only = query.get("output").and_then(Value::as_str) == Some("matchOnly")
                || query.get("resultView").and_then(Value::as_str) == Some("matchOnly");
            if query.get("matchWindow").is_some() && !is_match_only {
                return Err(issue(
                    "local-search.match-window",
                    prefix("matchWindow"),
                    "matchWindow requires matchOnly output",
                ));
            }
            if matches!(
                query.get("unique").and_then(Value::as_str),
                Some("list" | "count")
            ) && !is_match_only
            {
                return Err(issue(
                    "local-search.unique",
                    prefix("unique"),
                    "unique requires matchOnly output",
                ));
            }
            continue;
        }
        if query.get("snapshot").is_some() {
            return Err(issue(
                "local-search.structural-snapshot",
                prefix("snapshot"),
                "Remove the lexical snapshot in structural mode",
            ));
        }
        let pattern = query.get("pattern");
        let rule = query.get("rule");
        if pattern.is_none() && rule.is_none() {
            return Err(issue(
                "local-search.structural-selector",
                prefix("pattern"),
                "structural mode requires pattern or rule",
            ));
        }
        if pattern.is_some() && rule.is_some() {
            return Err(issue(
                "local-search.structural-exclusive",
                prefix("rule"),
                "pattern and rule are mutually exclusive",
            ));
        }
        if pattern
            .or(rule)
            .and_then(Value::as_str)
            .is_some_and(|v| v.trim().is_empty())
        {
            return Err(issue(
                "local-search.structural-blank",
                prefix(if pattern.is_some() { "pattern" } else { "rule" }),
                "Structural selector must not be blank",
            ));
        }
        for (field, invalid) in [
            (
                "wholeWord",
                query.get("wholeWord") == Some(&Value::Bool(true)),
            ),
            (
                "invertMatch",
                query.get("invertMatch") == Some(&Value::Bool(true)),
            ),
            (
                "captureText",
                query.get("captureText") == Some(&Value::Bool(true)),
            ),
            ("matchWindow", query.get("matchWindow").is_some()),
        ] {
            if invalid {
                return Err(issue(
                    "local-search.structural-field",
                    prefix(field),
                    format!("{field} is not valid in structural mode"),
                ));
            }
        }
        for (field, default) in [
            ("regex", "smart"),
            ("caseMode", "smart"),
            ("multiline", "off"),
        ] {
            if query
                .get(field)
                .and_then(Value::as_str)
                .is_some_and(|v| v != default)
            {
                return Err(issue(
                    "local-search.structural-field",
                    prefix(field),
                    format!("{field} is not valid in structural mode"),
                ));
            }
        }
        if query
            .get("output")
            .and_then(Value::as_str)
            .is_some_and(|v| !matches!(v, "content" | "countMatches" | "files"))
        {
            return Err(issue(
                "local-search.structural-output",
                prefix("output"),
                "unsupported structural output",
            ));
        }
        if query
            .get("unique")
            .and_then(Value::as_str)
            .is_some_and(|v| v != "off")
        {
            return Err(issue(
                "local-search.structural-unique",
                prefix("unique"),
                "unique is not valid in structural mode",
            ));
        }
        if let Some(pattern) = pattern.and_then(Value::as_str)
            && two_dollar_meta.is_match(pattern)
        {
            return Err(issue(
                "local-search.two-dollar-meta",
                prefix("pattern"),
                "two-dollar metavariables match nothing",
            ));
        }
    }
    Ok(())
}

fn apply_observed_defaults(defaults: &Value, query: &mut Value) {
    let Some(candidates) = defaults.as_array() else {
        return;
    };
    for candidate in candidates {
        let applies = candidate["when"].as_object().is_some_and(|selectors| {
            selectors
                .iter()
                .all(|(field, expected)| query.get(field) == Some(expected))
        }) && candidate["present"].as_array().is_some_and(|fields| {
            fields
                .iter()
                .filter_map(Value::as_str)
                .all(|field| query.get(field).is_some())
        }) && candidate["absent"].as_array().is_some_and(|fields| {
            fields
                .iter()
                .filter_map(Value::as_str)
                .all(|field| query.get(field).is_none())
        });
        if !applies {
            continue;
        }
        if let Some(values) = candidate["values"].as_object() {
            for (path, value) in values {
                let segments = path.split('.').collect::<Vec<_>>();
                insert_default(query, &segments, value);
            }
        }
        break;
    }
}

fn insert_default(target: &mut Value, path: &[&str], value: &Value) {
    let Some((head, tail)) = path.split_first() else {
        return;
    };
    let Some(object) = target.as_object_mut() else {
        return;
    };
    if tail.is_empty() {
        object
            .entry((*head).to_owned())
            .or_insert_with(|| value.clone());
        return;
    }
    if let Some(child) = object.get_mut(*head) {
        insert_default(child, tail, value);
    }
}

fn validate_artifact_queries(input: &Value) -> Result<(), ContractValidationError> {
    let queries = input["queries"].as_array().ok_or_else(|| {
        issue(
            "schema.queries",
            vec!["queries".into()],
            "Expected queries array",
        )
    })?;
    for (index, query) in queries.iter().enumerate() {
        let prefix = vec!["queries".into(), index.to_string()];
        let exact = query.get("packageName").is_some();
        let discovery = query.get("keywords").is_some();
        if exact == discovery {
            return Err(issue(
                "artifact.mode",
                prefix,
                "Set exactly one of packageName or keywords",
            ));
        }
        if exact
            && ["pageSize", "cursor"]
                .iter()
                .any(|field| query.get(field).is_some())
        {
            return Err(issue(
                "artifact.exact-pagination",
                prefix,
                "pageSize and cursor apply only to keyword discovery",
            ));
        }
        if let Some(cursor) = query.get("cursor").and_then(Value::as_str)
            && cursor.trim().is_empty()
        {
            return Err(issue(
                "artifact.blank-cursor",
                prefix,
                "cursor must not be blank",
            ));
        }
        if let Some(registry) = query.get("registry").and_then(Value::as_str) {
            if query.get("type").and_then(Value::as_str) != Some("npm") {
                return Err(issue(
                    "artifact.registry-type",
                    prefix,
                    "registry is supported only for type:npm",
                ));
            }
            let parsed = Url::parse(registry).map_err(|_| {
                issue(
                    "artifact.registry-url",
                    prefix.clone(),
                    "Use an HTTP(S) registry URL without credentials, query, or fragment",
                )
            })?;
            if !matches!(parsed.scheme(), "http" | "https")
                || !parsed.username().is_empty()
                || parsed.password().is_some()
                || parsed.query().is_some()
                || parsed.fragment().is_some()
            {
                return Err(issue(
                    "artifact.registry-url",
                    prefix,
                    "Use an HTTP(S) registry URL without credentials, query, or fragment",
                ));
            }
        }
    }
    Ok(())
}

fn validate_github_search_queries(input: &Value) -> Result<(), ContractValidationError> {
    let Some(queries) = input["queries"].as_array() else {
        return Ok(());
    };
    for (index, query) in queries.iter().enumerate() {
        let operation = query.get("operation").and_then(Value::as_str);
        let has_text = |field: &str| {
            query
                .get(field)
                .and_then(Value::as_str)
                .is_some_and(|value| !value.trim().is_empty())
        };
        let has_terms = |field: &str| {
            query
                .get(field)
                .and_then(Value::as_array)
                .is_some_and(|values| {
                    values
                        .iter()
                        .any(|value| value.as_str().is_some_and(|text| !text.trim().is_empty()))
                })
        };
        let runnable = match operation {
            Some("code") => {
                has_terms("keywords")
                    || ["path", "extension", "filename", "language"]
                        .iter()
                        .any(|field| has_text(field))
            }
            Some("repositories") => {
                has_terms("keywords")
                    || has_terms("topics")
                    || [
                        "owner",
                        "language",
                        "stars",
                        "forks",
                        "goodFirstIssues",
                        "updated",
                        "created",
                        "size",
                        "visibility",
                        "license",
                    ]
                    .iter()
                    .any(|field| has_text(field))
                    || query.get("archived").is_some_and(Value::is_boolean)
            }
            _ => true,
        };
        if !runnable {
            return Err(issue(
                "gh-search.runnable-constraint",
                vec!["queries".into(), index.to_string(), "operation".into()],
                format!(
                    "{} needs at least one search term or scope filter",
                    operation.unwrap_or("search")
                ),
            ));
        }
        // Code search must be scoped to an owner: the public contract states code
        // "cannot wildcard repositories", so an unscoped code query (which the
        // provider would run across all of GitHub) is rejected here rather than
        // silently returning global noise.
        if operation == Some("code") && !has_text("owner") {
            return Err(issue(
                "gh-search.code-scope",
                vec!["queries".into(), index.to_string(), "owner".into()],
                "code search requires an owner (optionally with repo); it cannot wildcard across all of GitHub".to_owned(),
            ));
        }
    }
    Ok(())
}

fn validate_ast_rewrite_queries(input: &Value) -> Result<(), ContractValidationError> {
    let Some(queries) = input["queries"].as_array() else {
        return Ok(());
    };
    for (index, query) in queries.iter().enumerate() {
        let prefix = vec!["queries".into(), index.to_string()];
        let apply = query.get("apply") == Some(&Value::Bool(true));
        let hashes_empty = query
            .get("expectedHashes")
            .and_then(Value::as_object)
            .is_none_or(serde_json::Map::is_empty);
        if apply && hashes_empty {
            return Err(issue(
                "ast-rewrite.apply-hashes",
                prefix,
                "apply requires non-empty expectedHashes copied from preview",
            ));
        }
        if apply && query.get("snapshot").is_none() {
            return Err(issue(
                "ast-rewrite.apply-snapshot",
                prefix,
                "apply requires the snapshot copied from preview",
            ));
        }
        if !apply && query.get("selectedMatchIds").is_some() {
            return Err(issue(
                "ast-rewrite.selected-apply-only",
                prefix,
                "selectedMatchIds is apply-only",
            ));
        }
        if !apply && query.get("postconditions").is_some() {
            return Err(issue(
                "ast-rewrite.postconditions-apply-only",
                prefix,
                "postconditions are apply-only",
            ));
        }
    }
    Ok(())
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

/// Return the closest name from `known` that differs from `unknown` by at most
/// `max_dist` edits (Levenshtein distance), or `None` if no match is close enough.
fn suggest_field<'a>(unknown: &str, known: &[&'a str]) -> Option<&'a str> {
    const MAX_DIST: usize = 3;
    known
        .iter()
        .filter_map(|&k| {
            let d = levenshtein(unknown, k);
            (d <= MAX_DIST).then_some((d, k))
        })
        .min_by_key(|(d, _)| *d)
        .map(|(_, k)| k)
}

/// Classic Wagner-Fischer Levenshtein distance, O(m*n) time, O(min(m,n)) space.
fn levenshtein(a: &str, b: &str) -> usize {
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
    use super::{format_input_error, validate};
    use crate::contracts::{PrepareOptions, prepare_and_validate};
    use serde_json::{Value, json};

    #[test]
    fn pure_clasify_requires_correlation_and_preserves_provider_entries() {
        let query = json!({"id":"decision","reasoning":"Decide the next evidence read.","resources":[{"id":"source","context": {"value": {"observation": true}},"maxChars":80000}], "questions":[{"id":"answer","question": {
            "type": "noul", "instructions": {"prompt":"Assess supplied state"}, "criteria":{"true":null,"false":null}
        }}]});
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
        for field in ["model", "goal", "debug", "route", "sources"] {
            let mut invalid = query.clone();
            invalid[field] = json!("not part of the pure protocol");
            assert!(
                prepare_and_validate("clasify", invalid, PrepareOptions::default()).is_err(),
                "{field}"
            );
        }
        for field in ["id", "reasoning", "resources", "questions"] {
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
            "blank semantic reasoning"
        );
    }

    #[test]
    fn validates_local_fetch_and_applies_schema_defaults() {
        let output = validate(
            "localFetch",
            json!({"queries":[{"path":"/tmp/a","reasoning":"Read the fixture."}]}),
        )
        .expect("valid query");
        assert_eq!(output["queries"][0]["goal"], Value::Null);
    }

    #[test]
    fn rejects_local_fetch_relations_and_unknown_fields() {
        let relation = validate(
            "localFetch",
            json!({"queries":[{"path":"/tmp/a","fullContent":true,"chunkSize":2,"reasoning":"Read the complete fixture."}]}),
        )
        .expect_err("invalid relation");
        // `chunkSize` is a valid chunk control, mutually exclusive with
        // fullContent, so the relation is rejected as a field conflict rather
        // than an unknown field.
        assert_eq!(
            format_input_error("localFetch", &relation),
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
                json!({"queries":[{"path":"/tmp/a","wat":true,"reasoning":"Exercise unknown-field validation."}]})
            )
            .is_err()
        );
    }

    #[test]
    fn rejects_repo_scoped_code_wildcards_before_provider_io() {
        let error = validate(
            "ghSearch",
            json!({"queries":[{
                "operation":"code",
                "owner":"octocode",
                "repo":"octocode",
                "reasoning":"Reject a repo-wide wildcard."
            }]}),
        )
        .expect_err("owner/repo alone is not a runnable code search");
        assert!(
            error
                .issues
                .iter()
                .any(|issue| issue.rule_id == "gh-search.runnable-constraint")
        );

        validate(
            "ghSearch",
            json!({"queries":[{
                "operation":"code",
                "owner":"octocode",
                "repo":"octocode",
                "path":"src",
                "reasoning":"Run a path-bounded code search."
            }]}),
        )
        .expect("path is an explicit code-search narrowing filter");
    }

    #[test]
    fn rejects_unscoped_code_search_that_would_wildcard_all_of_github() {
        let error = validate(
            "ghSearch",
            json!({"queries":[{
                "operation":"code",
                "keywords":["isEmptyArray"],
                "reasoning":"A keyword-only code search must not run globally."
            }]}),
        )
        .expect_err("code search without an owner is a global wildcard");
        assert!(
            error
                .issues
                .iter()
                .any(|issue| issue.rule_id == "gh-search.code-scope")
        );
        // owner alone (no repo) is a legitimate org-wide code search.
        validate(
            "ghSearch",
            json!({"queries":[{
                "operation":"code",
                "owner":"sindresorhus",
                "keywords":["isEmptyArray"],
                "reasoning":"Owner-scoped code search is allowed."
            }]}),
        )
        .expect("owner-scoped code search is runnable");
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
            json!({"queries":[{"path":"/tmp/a","startLine":5,"endLine":2,"reasoning":"Exercise range validation."}]}),
        )
        .expect_err("range");
        assert_eq!(
            format_input_error("localFetch", &range),
            json!({
                "kind":"octocode.toolError","version":1,"tool":"localFetch","error":"Check the query fields.",
                "details":["queries.0.endLine: Set endLine greater than or equal to startLine."]
            })
        );
        let unknown = validate(
            "localFetch",
            json!({"queries":[{"path":"/tmp/a","madeUp":true,"reasoning":"Exercise unknown-field validation."}]}),
        )
        .expect_err("unknown");
        assert_eq!(
            format_input_error("localFetch", &unknown),
            json!({
                "kind":"octocode.toolError","version":1,"tool":"localFetch","error":"Unknown field(s): madeUp",
                "details":["Remove unknown field(s) from query 1: madeUp", "Run scheme localFetch --view query --compact to see valid fields."]
            })
        );
    }

    #[test]
    fn suggests_reasoning_for_a_typo_across_every_tool_shape() {
        // The enforcement IR carries no presentation examples; the accepted
        // parity fixtures are the generated per-tool query corpus instead.
        let fixtures: Value =
            serde_json::from_str(include_str!("generated/contract-fixtures.json"))
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
            let formatted = format_input_error(name, &error).to_string();
            assert!(
                formatted.contains("did you mean 'reasoning'?"),
                "{name}: {formatted}"
            );
        }
    }

    #[test]
    fn matches_generated_reference_corpus() {
        let fixtures: Value =
            serde_json::from_str(include_str!("generated/contract-fixtures.json"))
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
}
