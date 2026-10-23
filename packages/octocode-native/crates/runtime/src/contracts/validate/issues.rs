use super::{ContractValidationError, ValidationIssue};
use crate::contracts::validate::repository;
use crate::contracts::validate::union;
use serde_json::Value;

/// Where an unknown field sits: `queries[N]` (the row's 0-based output
/// `index`) for a bulk row, else the object path (`questions.0`), so a
/// root-level clasify matrix is not called a query row.
pub(super) fn query_label(path: &[String]) -> String {
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
pub(super) fn sibling_requirement(issue: &ValidationIssue) -> Option<String> {
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

pub(super) fn issue(
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

pub(super) fn schema_issue(
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

pub(super) fn internal(message: String) -> ContractValidationError {
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
pub(super) fn name_field(issue: &ValidationIssue) -> Option<String> {
    match issue.schema.as_ref()?["nameFields"].as_array()?.as_slice() {
        [only] => Some(format!("{}:\"{}\"", only.as_str()?, issue.path.last()?)),
        _ => None,
    }
}

/// What an unknown field most likely meant: the field its value names, the
/// field its name is a kind of value of, the value its name names, else the
/// nearest known spelling.
pub(super) fn field_guess(issue: &ValidationIssue, field: &str, known: &[&str]) -> Option<String> {
    value_field(issue)
        .map(str::to_owned)
        .or_else(|| kind_field(issue))
        .or_else(|| name_field(issue))
        .or_else(|| suggest_field(field, known).map(str::to_owned))
}

/// A field another form of the row declares: when the fix sets that form's
/// omitted selector (`setFields`), either fix is the caller's to choose.
pub(super) fn sibling_message(
    issue: &ValidationIssue,
    field: &str,
    query: &str,
    requires: &str,
) -> String {
    if repository::set_fields(issue).is_some() {
        format!("'{field}' in {query} applies only with {requires}: set it, or remove '{field}'.")
    } else {
        format!("Remove '{field}' from {query}: it applies only with {requires}.")
    }
}

/// Words that qualify a field name without changing what it names
/// (`depth` → `maxDepth`, `isRegex` → `regex`); any other word does
/// (`lines` is no `contextLines`, `length` no `responseLength`).
const QUALIFIER_WORDS: [&str; 4] = ["max", "min", "is", "include"];

/// The field `known` most likely meant by `unknown`: a known field that
/// prefixes it, one that adds or drops a qualifier word, or the nearest
/// spelling. The edit budget scales with the name so a short guess
/// (`depth`, `mode`, `name`) never lands on an unrelated field: a name of
/// four letters or fewer needs one edit and the same first letter.
pub(super) fn suggest_field<'a>(unknown: &str, known: &[&'a str]) -> Option<&'a str> {
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
    // other, a qualifier word added or dropped in front.
    if let Some(suffix) = known
        .iter()
        .copied()
        .filter(|k| {
            let k = k.to_lowercase();
            let (long, short) = if k.len() > lowered.len() {
                (k.as_str(), lowered.as_str())
            } else {
                (lowered.as_str(), k.as_str())
            };
            short.len() >= 4
                && long
                    .strip_suffix(short)
                    .is_some_and(|word| QUALIFIER_WORDS.contains(&word))
        })
        .min_by_key(|k| k.len().abs_diff(unknown.len()))
    {
        return Some(suffix);
    }
    let length = unknown.chars().count();
    let short = length <= 4;
    let max_dist = if short { 1 } else { (length / 3).clamp(2, 3) };
    let first = |name: &str| name.chars().next().map(|c| c.to_ascii_lowercase());
    known
        .iter()
        .filter(|k| !short || first(k) == first(unknown))
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
