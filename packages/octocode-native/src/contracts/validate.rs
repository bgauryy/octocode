mod content;
mod union;
use regex::Regex;
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
            details.push(format!(
                "Remove unknown field(s) from query {query}: {field}"
            ));
        }
        details.push(format!(
            "Run tools {tool_name} --scheme --brief to see valid fields."
        ));
        return serde_json::json!({"kind":"octocode.toolError","version":1,"tool":tool_name,"error":format!("Unknown field(s): {}", fields.join(", ")),"details":details});
    }
    let details = error
        .issues
        .iter()
        .map(|issue| {
            let path = issue.path.join(".");
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
            Some("clamp_fields") => clamp_fields(input, &rule["args"]),
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

fn clamp_fields(input: &mut Value, args: &Value) {
    let Some(fields) = args["fields"].as_array() else {
        return;
    };
    for spec in fields {
        let (Some(field), Some(minimum), Some(maximum)) = (
            spec["field"].as_str(),
            spec["min"].as_f64(),
            spec["max"].as_f64(),
        ) else {
            continue;
        };
        if spec["scope"] == "envelope" {
            clamp_field(input, field, minimum, maximum);
        }
        if spec["scope"] == "query"
            && let Some(queries) = input.get_mut("queries").and_then(Value::as_array_mut)
        {
            for query in queries {
                clamp_field(query, field, minimum, maximum);
            }
        }
    }
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
            Some("disabled_field") => validate_disabled_field(input, &rule["args"], &rule["id"]),
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

fn validate_disabled_field(
    input: &Value,
    args: &Value,
    rule_id: &Value,
) -> Result<(), ContractValidationError> {
    let Some(field) = args["field"].as_str() else {
        return Err(internal("disabled_field requires field".into()));
    };
    for (index, query) in query_values(input) {
        if query.get(field).is_some() {
            return Err(issue(
                rule_id.as_str().unwrap_or("disabled-field"),
                vec!["queries".into(), index.to_string(), field.into()],
                format!("{field} is disabled in this build"),
            ));
        }
    }
    Ok(())
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
        if query.get("operation").and_then(Value::as_str) != Some("commits") {
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
        if let Some(pattern) = pattern.and_then(Value::as_str) {
            let exempt = matches!(
                query.get("langType").and_then(Value::as_str),
                Some("php" | "bash" | "sh" | "zsh")
            );
            if !exempt && two_dollar_meta.is_match(pattern) {
                return Err(issue(
                    "local-search.two-dollar-meta",
                    prefix("pattern"),
                    "two-dollar metavariables match nothing",
                ));
            }
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

fn clamp_field(value: &mut Value, field: &str, minimum: f64, maximum: f64) {
    let Some(number) = value.get(field).and_then(Value::as_f64) else {
        return;
    };
    if !number.is_finite() {
        return;
    }
    let clamped = number.clamp(minimum, maximum);
    let result = if clamped.fract() == 0.0 && clamped <= i64::MAX as f64 {
        serde_json::Number::from(clamped as i64)
    } else if let Some(number) = serde_json::Number::from_f64(clamped) {
        number
    } else {
        return;
    };
    value[field] = Value::Number(result);
}

fn validate_schema(
    root: &Value,
    schema: &Value,
    value: &mut Value,
    path: &mut Vec<String>,
) -> Result<(), ContractValidationError> {
    if let Some(forbidden) = schema.get("not") {
        let mut candidate = value.clone();
        if validate_schema(root, forbidden, &mut candidate, &mut path.clone()).is_ok() {
            return Err(issue(
                "schema.not",
                path.clone(),
                "Value matches a forbidden schema",
            ));
        }
    }
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        let target = root
            .pointer(reference.strip_prefix('#').unwrap_or(reference))
            .ok_or_else(|| {
                issue(
                    "schema.unsupported-ref",
                    path.clone(),
                    format!("Unresolved schema reference: {reference}"),
                )
            })?;
        return validate_schema(root, target, value, path);
    }
    let union = schema
        .get("oneOf")
        .and_then(Value::as_array)
        .map(|branches| (branches, true))
        .or_else(|| {
            schema
                .get("anyOf")
                .and_then(Value::as_array)
                .map(|branches| (branches, false))
        });
    if let Some((branches, exclusive)) = union {
        return union::validate(root, branches, exclusive, value, path);
    }

    if let Some(constant) = schema.get("const")
        && value != constant
    {
        return Err(schema_issue(
            "schema.const",
            path.clone(),
            "Unexpected constant value",
            schema,
            value,
        ));
    }
    if let Some(values) = schema.get("enum").and_then(Value::as_array)
        && !values.contains(value)
    {
        return Err(schema_issue(
            "schema.enum",
            path.clone(),
            "Value is outside the allowed enum",
            schema,
            value,
        ));
    }
    match schema.get("type").and_then(Value::as_str) {
        Some("object") => validate_object(root, schema, value, path),
        Some("array") => validate_array(root, schema, value, path),
        Some("string") => validate_string(schema, value, path),
        Some("integer") => validate_number(schema, value, path, true),
        Some("number") => validate_number(schema, value, path, false),
        Some("boolean") if !value.is_boolean() => Err(schema_issue(
            "schema.type",
            path.clone(),
            "Expected boolean",
            schema,
            value,
        )),
        Some("null") if !value.is_null() => Err(schema_issue(
            "schema.type",
            path.clone(),
            "Expected null",
            schema,
            value,
        )),
        Some("boolean" | "null") | None => Ok(()),
        Some(other) => Err(issue(
            "schema.unsupported-type",
            path.clone(),
            format!("Unsupported schema type: {other}"),
        )),
    }
}

fn validate_object(
    root: &Value,
    schema: &Value,
    value: &mut Value,
    path: &mut Vec<String>,
) -> Result<(), ContractValidationError> {
    let received = value.clone();
    let object = value.as_object_mut().ok_or_else(|| {
        schema_issue(
            "schema.type",
            path.clone(),
            "Expected object",
            schema,
            &received,
        )
    })?;
    let properties = schema.get("properties").and_then(Value::as_object);
    let mut issues = Vec::new();
    if let Some(name_schema) = schema.get("propertyNames") {
        for key in object.keys() {
            let mut name = Value::String(key.clone());
            let mut name_path = path.clone();
            name_path.push(key.clone());
            if let Err(error) = validate_schema(root, name_schema, &mut name, &mut name_path) {
                issues.extend(error.issues);
            }
        }
    }
    if schema.get("additionalProperties") == Some(&Value::Bool(false)) {
        for key in object.keys() {
            if !properties.is_some_and(|known| known.contains_key(key)) {
                let mut field_path = path.clone();
                field_path.push(key.clone());
                issues.extend(
                    issue(
                        "schema.unknown-field",
                        field_path,
                        format!("Unknown field: {key}"),
                    )
                    .issues,
                );
            }
        }
    }
    if let Some(additional) = schema
        .get("additionalProperties")
        .filter(|value| value.is_object())
    {
        for (key, field) in object.iter_mut() {
            if properties.is_some_and(|known| known.contains_key(key)) {
                continue;
            }
            path.push(key.clone());
            let result = validate_schema(root, additional, field, path);
            path.pop();
            if let Err(error) = result {
                issues.extend(error.issues);
            }
        }
    }
    for required in schema
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        if !object.contains_key(required) {
            let mut field_path = path.clone();
            field_path.push(required.to_owned());
            issues.extend(
                schema_issue(
                    "schema.required",
                    field_path,
                    format!("Missing required field: {required}"),
                    properties
                        .and_then(|items| items.get(required))
                        .unwrap_or(&Value::Null),
                    &Value::Null,
                )
                .issues,
            );
        }
    }
    if let Some(properties) = properties {
        for (name, field_schema) in properties {
            if !object.contains_key(name)
                && let Some(default) = field_schema.get("default")
            {
                object.insert(name.clone(), default.clone());
            }
            if let Some(field) = object.get_mut(name) {
                path.push(name.clone());
                let result = validate_schema(root, field_schema, field, path);
                path.pop();
                if let Err(error) = result {
                    issues.extend(error.issues);
                }
            }
        }
    }
    if issues.is_empty() {
        Ok(())
    } else {
        Err(ContractValidationError { issues })
    }
}

fn validate_array(
    root: &Value,
    schema: &Value,
    value: &mut Value,
    path: &mut Vec<String>,
) -> Result<(), ContractValidationError> {
    let received = value.clone();
    let array = value.as_array_mut().ok_or_else(|| {
        schema_issue(
            "schema.type",
            path.clone(),
            "Expected array",
            schema,
            &received,
        )
    })?;
    let mut issues = Vec::new();
    if let Err(error) = check_size(schema, array.len(), path) {
        issues.extend(error.issues);
    }
    if let Some(items) = schema.get("items") {
        for (index, item) in array.iter_mut().enumerate() {
            path.push(index.to_string());
            let result = validate_schema(root, items, item, path);
            path.pop();
            if let Err(error) = result {
                issues.extend(error.issues);
            }
        }
    }
    if issues.is_empty() {
        Ok(())
    } else {
        Err(ContractValidationError { issues })
    }
}

fn validate_string(
    schema: &Value,
    value: &Value,
    path: &[String],
) -> Result<(), ContractValidationError> {
    let string = value.as_str().ok_or_else(|| {
        schema_issue(
            "schema.type",
            path.to_vec(),
            "Expected string",
            schema,
            value,
        )
    })?;
    check_size(schema, string.chars().count(), path)?;
    if let Some(pattern) = schema.get("pattern").and_then(Value::as_str) {
        let regex = Regex::new(pattern).map_err(|error| {
            issue(
                "schema.unsupported-pattern",
                path.to_vec(),
                error.to_string(),
            )
        })?;
        if !regex.is_match(string) {
            return Err(issue(
                "schema.pattern",
                path.to_vec(),
                "String does not match required pattern",
            ));
        }
    }
    if schema.get("format").and_then(Value::as_str) == Some("uri") && Url::parse(string).is_err() {
        return Err(issue("schema.uri", path.to_vec(), "Expected a valid URI"));
    }
    Ok(())
}

fn validate_number(
    schema: &Value,
    value: &Value,
    path: &[String],
    integer: bool,
) -> Result<(), ContractValidationError> {
    let number = value.as_f64().ok_or_else(|| {
        schema_issue(
            "schema.type",
            path.to_vec(),
            "Expected number",
            schema,
            value,
        )
    })?;
    if integer && number.fract() != 0.0 {
        return Err(issue("schema.integer", path.to_vec(), "Expected integer"));
    }
    if schema
        .get("minimum")
        .and_then(Value::as_f64)
        .is_some_and(|minimum| number < minimum)
        || schema
            .get("maximum")
            .and_then(Value::as_f64)
            .is_some_and(|maximum| number > maximum)
    {
        return Err(schema_issue(
            "schema.range",
            path.to_vec(),
            "Number is outside the allowed range",
            schema,
            value,
        ));
    }
    Ok(())
}

fn check_size(
    schema: &Value,
    length: usize,
    path: &[String],
) -> Result<(), ContractValidationError> {
    let minimum = schema
        .get("minLength")
        .or_else(|| schema.get("minItems"))
        .and_then(Value::as_u64);
    let maximum = schema
        .get("maxLength")
        .or_else(|| schema.get("maxItems"))
        .and_then(Value::as_u64);
    if minimum.is_some_and(|bound| length < bound as usize)
        || maximum.is_some_and(|bound| length > bound as usize)
    {
        return Err(issue(
            "schema.size",
            path.to_vec(),
            "Value length is outside the allowed range",
        ));
    }
    Ok(())
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
                    || ["owner", "path", "extension", "filename", "language"]
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

#[cfg(test)]
mod tests {
    use super::{format_input_error, validate};
    use crate::contracts::{PrepareOptions, prepare_and_validate};
    use serde_json::{Value, json};

    #[test]
    fn validates_local_fetch_and_applies_schema_defaults() {
        let output =
            validate("localFetch", json!({"queries":[{"path":"/tmp/a"}]})).expect("valid query");
        assert_eq!(output["queries"][0]["goal"], Value::Null);
    }

    #[test]
    fn rejects_local_fetch_relations_and_unknown_fields() {
        let relation = validate(
            "localFetch",
            json!({"queries":[{"path":"/tmp/a","fullContent":true,"limit":2}]}),
        )
        .expect_err("invalid relation");
        assert_eq!(
            format_input_error("localFetch", &relation),
            json!({
                "kind":"octocode.toolError", "version":1, "tool":"localFetch",
                "error":"Check the query fields.",
                "details":["queries.0: Unrecognized key: \"limit\""]
            })
        );
        assert!(
            validate(
                "localFetch",
                json!({"queries":[{"path":"/tmp/a","wat":true}]})
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
            json!({"queries":[{"path":"/tmp/a","startLine":5,"endLine":2}]}),
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
            json!({"queries":[{"path":"/tmp/a","madeUp":true}]}),
        )
        .expect_err("unknown");
        assert_eq!(
            format_input_error("localFetch", &unknown),
            json!({
                "kind":"octocode.toolError","version":1,"tool":"localFetch","error":"Unknown field(s): madeUp",
                "details":["Remove unknown field(s) from query 1: madeUp", "Run tools localFetch --scheme --brief to see valid fields."]
            })
        );
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
                assert_eq!(normalized, fixture["normalized"], "{}", fixture["id"]);
            }
        }
    }
}
