//! The input gate: the one place a caller's query is checked, and the only
//! place a [`CheckedQuery`] is made. Every tool runs on a `CheckedQuery`, so a
//! query that skipped this gate cannot reach a tool.
//!
//! The walk sanitizes the owned query in place: a string is replaced only by
//! its redacted form, and an entry the policy refuses (a blocked key, an
//! over-long string, a too-large array) is removed where it stands. Nothing is
//! rebuilt or cloned, so a clean query costs one scan of its strings.

use std::ops::Deref;

use serde_json::{Map, Value};

use super::ContentSecurity;
use crate::contracts::ValidationIssue;

/// Security ceiling, not a data cap: an input string past it is refused at
/// the gate (paths, patterns and keywords are far shorter), so no caller
/// value can force unbounded scanning or redaction work.
const MAX_STRING_LENGTH: usize = 10_000;
/// Contract `limits.maxInputArrayItems`.
const MAX_ARRAY_LENGTH: usize = crate::id::MAX_INPUT_ARRAY_ITEMS;
/// Security ceiling, not a data cap: JSON nesting deeper than this is
/// refused, so a hostile query cannot exhaust the stack of the recursive
/// walk. No tool input nests anywhere near it.
const MAX_DEPTH: usize = 20;

/// A query row that passed [`ContentSecurity::check_input`]: its keys are
/// safe, its strings are within bounds, and it holds no credential outside
/// the evidence a policy may redact in place.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(transparent)]
pub(crate) struct CheckedQuery(Value);

impl CheckedQuery {
    pub(crate) fn into_value(self) -> Value {
        self.0
    }
}

#[cfg(test)]
impl CheckedQuery {
    /// `query` through the gate with [`InputPolicy::Query`]; panics when the
    /// gate refuses it, so a test row is always one a caller could send.
    pub(crate) fn checked(query: Value) -> Self {
        ContentSecurity::new()
            .check_input(query, InputPolicy::Query)
            .unwrap_or_else(|issues| panic!("the input gate refused a test query: {issues:?}"))
    }
}

impl std::borrow::Borrow<Value> for CheckedQuery {
    fn borrow(&self) -> &Value {
        &self.0
    }
}

impl Deref for CheckedQuery {
    type Target = Value;

    fn deref(&self) -> &Value {
        &self.0
    }
}

/// What a row may contain beyond the standard bounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InputPolicy {
    /// Every string is bounded and a credential-shaped one is refused:
    /// redacting it would silently change what the tool reads, and leak the
    /// placeholder into continuations.
    Query,
    /// A clasify row: its `content` may be as long as `content_limit` serialized
    /// characters, and credential-shaped text inside `content` or `questions`
    /// is redacted in place (it is evidence, not an address).
    ClasifyEvidence { content_limit: usize },
}

/// The facts one walk found.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Verdict {
    pub(crate) is_valid: bool,
    pub(crate) has_secrets: bool,
    pub warnings: Vec<String>,
    /// Dotted paths of the string leaves whose value sanitization rewrote
    /// (`matchString`, `keywords[]`, `outer.key`).
    pub(crate) secret_fields: Vec<String>,
}

impl Verdict {
    fn clean() -> Self {
        Self {
            is_valid: true,
            has_secrets: false,
            warnings: Vec::new(),
            secret_fields: Vec::new(),
        }
    }

    fn refused(&mut self, warning: impl Into<String>) {
        self.is_valid = false;
        self.warnings.push(warning.into());
    }

    fn invalid(warning: impl Into<String>) -> Self {
        let mut verdict = Self::clean();
        verdict.refused(warning);
        verdict
    }

    /// Fold a nested object's verdict into this one, naming its secret
    /// fields with `field` and its warnings with `warning`.
    fn absorb(
        &mut self,
        nested: Self,
        field: impl Fn(&str) -> String,
        warning: impl Fn(&str) -> String,
    ) {
        self.has_secrets |= nested.has_secrets;
        self.is_valid &= nested.is_valid;
        self.secret_fields
            .extend(nested.secret_fields.iter().map(|name| field(name)));
        self.warnings
            .extend(nested.warnings.iter().map(|text| warning(text)));
    }
}

/// A walk's verdict and the query as the walk left it: refused entries
/// removed, credential-shaped strings redacted.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ValidationResult {
    pub(crate) sanitized_params: Map<String, Value>,
    verdict: Verdict,
}

impl Deref for ValidationResult {
    type Target = Verdict;

    fn deref(&self) -> &Verdict {
        &self.verdict
    }
}

impl ContentSecurity {
    /// The one input gate. `query` is sanitized in place; the result is the
    /// proof every tool runs on, or the field issues (relative to the row)
    /// that refused it.
    pub(crate) fn check_input(
        &self,
        query: Value,
        policy: InputPolicy,
    ) -> Result<CheckedQuery, Vec<ValidationIssue>> {
        let content_limit = match policy {
            InputPolicy::Query => None,
            InputPolicy::ClasifyEvidence { content_limit } => Some(content_limit),
        };
        let checked = self.walk(query, content_limit);
        let issues = input_issues(&checked, content_limit.is_some());
        if issues.is_empty() {
            Ok(CheckedQuery(Value::Object(checked.sanitized_params)))
        } else {
            Err(issues)
        }
    }

    /// The walk over a borrowed query, for inspecting a verdict.
    #[cfg(test)]
    pub(crate) fn validate_input_parameters(&self, params: &Value) -> ValidationResult {
        self.walk(params.clone(), None)
    }

    /// The walk with a clasify `content` allowance, for inspecting a verdict.
    #[cfg(test)]
    pub(crate) fn validate_content_parameters(
        &self,
        params: &Value,
        content_limit: usize,
    ) -> ValidationResult {
        self.walk(params.clone(), Some(content_limit))
    }

    fn walk(&self, params: Value, content_limit: Option<usize>) -> ValidationResult {
        let Value::Object(mut object) = params else {
            return ValidationResult {
                sanitized_params: Map::new(),
                verdict: Verdict::invalid("Invalid parameters: must be an object"),
            };
        };
        if let (Some(limit), Some(state)) = (content_limit, object.get("content"))
            && state.to_string().chars().count() > limit
        {
            return ValidationResult {
                sanitized_params: Map::new(),
                verdict: Verdict::invalid(format!(
                    "Parameter content exceeds maximum serialized length ({limit} characters)"
                )),
            };
        }
        let verdict = self.walk_object(&mut object, 0, MAX_STRING_LENGTH, content_limit);
        ValidationResult {
            sanitized_params: object,
            verdict,
        }
    }

    /// A state allowance applies at the declared root only; its already bounded
    /// subtree inherits it without granting it to unrelated nested `content` keys.
    fn walk_object(
        &self,
        object: &mut Map<String, Value>,
        depth: usize,
        limit: usize,
        content_limit: Option<usize>,
    ) -> Verdict {
        if depth > MAX_DEPTH {
            object.clear();
            return Verdict::invalid("Maximum nesting depth exceeded");
        }
        let mut out = Verdict::clean();
        object.retain(|key, value| {
            // Keys also become provider input and diagnostic field paths.
            // Reject credentials here; renaming keys would change the data.
            if self.sanitize_text(key, None).has_secrets {
                out.refused("A parameter key contains a credential; remove it before retrying");
                out.has_secrets = true;
                return false;
            }
            // Ordinary parameter limits count UTF-16 units. The complete state
            // was bounded in Unicode scalar values; allow both UTF-16 units of
            // an astral character without rejecting valid evidence.
            let limit = if key == "content" {
                content_limit.map_or(limit, |bound| bound.saturating_mul(2))
            } else {
                limit
            };
            if key.trim().is_empty() {
                out.refused(format!("Invalid parameter key: {key}"));
                return false;
            }
            if matches!(key.as_str(), "__proto__" | "constructor" | "prototype") {
                out.refused(format!("Dangerous parameter key blocked: {key}"));
                return false;
            }
            match value {
                Value::String(text) => {
                    if text.encode_utf16().count() > limit {
                        out.refused(format!(
                            "Parameter {key} exceeds maximum length ({limit} characters)"
                        ));
                        return false;
                    }
                    let result = self.sanitize_text(text, None);
                    if result.has_secrets {
                        out.has_secrets = true;
                        out.secret_fields.push(key.clone());
                        for secret in result.secrets_detected {
                            out.warnings
                                .push(format!("Secrets detected in {key}: {secret}"));
                        }
                    }
                    *text = result.content;
                    true
                }
                Value::Array(values) => self.walk_array(key, values, depth, limit, &mut out),
                Value::Object(nested) => {
                    let result = self.walk_object(nested, depth + 1, limit, None);
                    out.absorb(
                        result,
                        |field| format!("{key}.{field}"),
                        |warning| format!("Invalid nested object in parameter {key}: {warning}"),
                    );
                    true
                }
                _ => true,
            }
        });
        out.warnings.sort();
        out.warnings.dedup();
        out.secret_fields.sort();
        out.secret_fields.dedup();
        out
    }

    /// Sanitizes parameter `key`'s items in place; `false` (with a warning in
    /// `out`) when the array must go whole: too deep or too many items.
    fn walk_array(
        &self,
        key: &str,
        values: &mut Vec<Value>,
        depth: usize,
        limit: usize,
        out: &mut Verdict,
    ) -> bool {
        if depth > MAX_DEPTH {
            out.refused("Maximum nesting depth exceeded");
            return false;
        }
        if values.len() > MAX_ARRAY_LENGTH {
            out.refused(format!(
                "Parameter {key} array exceeds maximum length (100 items)"
            ));
            return false;
        }
        let mut item_secrets = false;
        values.retain_mut(|item| match item {
            Value::String(text) if text.encode_utf16().count() > limit => {
                out.refused(format!(
                    "Parameter {key}[] exceeds maximum length ({limit} characters)"
                ));
                false
            }
            Value::String(text) => {
                let result = self.sanitize_text(text, None);
                item_secrets |= result.has_secrets;
                *text = result.content;
                true
            }
            Value::Object(nested) => {
                let result = self.walk_object(nested, depth + 1, limit, None);
                out.absorb(
                    result,
                    |field| format!("{key}[].{field}"),
                    |warning| format!("{key}[]: {warning}"),
                );
                true
            }
            // Nested arrays recurse, so string leaves at any array depth are
            // sanitized.
            Value::Array(inner) => self.walk_array(key, inner, depth + 1, limit, out),
            _ => true,
        });
        if item_secrets {
            out.has_secrets = true;
            out.secret_fields.push(format!("{key}[]"));
        }
        true
    }
}

/// A walk's refusals as field issues, so they isolate and render like any
/// other invalid field. Only clasify evidence may be redacted in place: any
/// other rewritten value, including a clasify `source`, would silently change
/// what a tool reads and leak the placeholder into continuations.
fn input_issues(checked: &ValidationResult, clasify: bool) -> Vec<ValidationIssue> {
    let issue = |rule_id: &str, field: Option<&str>, message: String| ValidationIssue {
        rule_id: rule_id.into(),
        path: field.map(str::to_owned).into_iter().collect(),
        message,
        schema: None,
        received: None,
    };
    let mut issues = Vec::new();
    if !checked.is_valid {
        issues.extend(
            checked
                .warnings
                .iter()
                .filter(|warning| !warning.starts_with("Secrets detected"))
                .map(|warning| {
                    let hint = if warning.contains("exceeds maximum length") {
                        "; send a shorter, distinctive fragment"
                    } else {
                        ""
                    };
                    issue("security.input", None, format!("{warning}{hint}"))
                }),
        );
    }
    let redactable = |field: &str| {
        clasify
            && ["content", "questions"].iter().any(|root| {
                field == *root
                    || field.starts_with(&format!("{root}."))
                    || field.starts_with(&format!("{root}["))
            })
    };
    issues.extend(
        checked
            .secret_fields
            .iter()
            .filter(|field| !redactable(field))
            .map(|field| {
                issue(
                    "security.credential",
                    Some(field),
                    "Value looks like a credential and was not run (its redacted form would match unrelated text); search a non-secret fragment instead, such as its prefix or the identifier that holds it".into(),
                )
            }),
    );
    issues
}
