use super::artifact::validate_artifact_registry_url;
use super::github_search::{GithubSearchKind, validate_github_search_queries};
use super::history::{validate_history_keyword_scope, validate_history_repository_scope};
use super::local_search::validate_local_search_queries;
use super::lsp::validate_lsp_queries;
use super::{ContractValidationError, internal, issue};
use crate::contracts::validate::clasify;
use crate::contracts::validate::content;
use crate::contracts::validate::depth_range;
use crate::contracts::validate::qualifiers;
use crate::contracts::validate::structural_rule;
use serde_json::Value;

type RowNormalizer = fn(&mut Value, &Value) -> Result<(), ContractValidationError>;

pub(super) fn apply_normalization_rules(
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

pub(super) fn apply_validation_rules(
    rules: &Value,
    input: &Value,
) -> Result<(), ContractValidationError> {
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
            Some("clasify_box_image") => clasify::validate_box_image(input),
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

pub(super) fn query_values(input: &Value) -> impl Iterator<Item = (usize, &Value)> {
    input["queries"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
}
