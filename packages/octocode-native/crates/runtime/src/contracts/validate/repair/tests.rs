use super::*;
use serde_json::json;
mod envelope;
mod fields;
mod values;

fn issue(
    rule: &str,
    path: &[&str],
    schema: Option<Value>,
    received: Option<Value>,
) -> ValidationIssue {
    ValidationIssue {
        rule_id: rule.into(),
        path: path.iter().map(|s| (*s).to_owned()).collect(),
        message: String::new(),
        schema,
        received,
    }
}

fn always(_: &Value) -> Result<(), Vec<ValidationIssue>> {
    Ok(())
}

/// `accepts` as a check that refuses with nothing left to fix.
fn only(accepts: impl Fn(&Value) -> bool) -> impl Fn(&Value) -> Result<(), Vec<ValidationIssue>> {
    move |candidate| accepts(candidate).then_some(()).ok_or_else(Vec::new)
}
