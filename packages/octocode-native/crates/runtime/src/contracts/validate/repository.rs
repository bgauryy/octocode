//! Repository coordinates a row spells another way. A row field whose
//! schema declares `x-joinedWith: <partner>` (core metadata: the GitHub
//! `owner` field, joined with `repo`) is the first half of a
//! `first/second` pair. Input validation turns three spellings into issues
//! whose schema carries the exact fix as `setFields` (field → value; `null`
//! removes the field), which the repair applies as written:
//!
//! - one half holding the whole pair (`owner:"a/b"`, or `repo:"a/b"`): the
//!   halves, when each is a valid value of its field and the other field is
//!   absent or already equal to its half;
//! - the second half ending in a clone URL's `.git` (`repo:"is.git"`): the
//!   name without it, which is what the repository is called;
//! - a repository web URL as the whole value of a pair field or of the
//!   row's `path` (`https://github.com/o/r/blob/<ref>/<path>`,
//!   `https://raw.githubusercontent.com/o/r/<ref>/<path>`): the pair, and the
//!   ref and file path in the fields GitHub's own API names `ref` and `path`
//!   (when the row declares them), and a `#L3-L9` line anchor as the span of
//!   the row's span field (`ranges:["3-9"]`). A ref may hold `/`, so a URL
//!   with a file path after a ref that is no full commit SHA (or with a
//!   `?query` or `#fragment` the fields cannot carry) is read
//!   approximately: the issue names those fields in `approximateFields`;
//! - a pull request, issue, commit or compare URL
//!   (`https://github.com/o/r/pull/7`, `/issues/7`, `/commit/<sha>`,
//!   `/compare/<base>...<head>`): the pair, the row selector whose literal
//!   the URL's kind spells (`operation:"pullRequest"`), and the item in the
//!   fields GitHub's API names `number`, `ref`, or `base` and `head`; a row
//!   form without such a selector and fields takes no such URL.
//!
//! A required or pattern issue at a field another issue's fix sets is
//! settled by that fix and dropped, so the error names one fix.

mod web_url;
use super::schema::{ValidationMode, validate_with_mode};
use super::{ValidationIssue, union};
use serde_json::{Map, Value, json};
use web_url::WebUrl;

/// The row fields a web URL's ref and file path go to: GitHub's names.
const REF_FIELD: &str = "ref";
const PATH_FIELD: &str = "path";
/// The row fields a pull request or issue number and a comparison's two
/// refs go to: GitHub's names.
const NUMBER_FIELD: &str = "number";
const BASE_FIELD: &str = "base";
const HEAD_FIELD: &str = "head";
/// A clone URL's suffix, never part of a repository's name.
const CLONE_SUFFIX: &str = ".git";

/// A declared `first/second` pair of row fields, with their schemas.
struct Pair<'a> {
    first: &'a str,
    second: &'a str,
    first_schema: &'a Value,
    second_schema: &'a Value,
}

/// Adds the coordinate issues of the row `object` at `path` and settles
/// what their fixes cover (see the module note).
pub(super) fn settle(
    root: &Value,
    properties: &Map<String, Value>,
    object: &Map<String, Value>,
    path: &[String],
    issues: &mut Vec<ValidationIssue>,
) {
    let pairs = pairs(root, properties);
    if pairs.is_empty() {
        return;
    }
    let speculative = union::speculative();
    for pair in &pairs {
        for field in [pair.first, pair.second, PATH_FIELD] {
            let Some(text) = object
                .get(field)
                .filter(|_| properties.contains_key(field))
                .and_then(Value::as_str)
            else {
                continue;
            };
            // A pair field never holds a `/`, so a URL there needs no scheme;
            // a file path may begin with a host-like directory, so it does.
            let Some(url) = WebUrl::parse(text, field != PATH_FIELD) else {
                continue;
            };
            let span = super::schema::span_field(root, properties);
            issues.extend(url.issue(pair, properties, object, path, field, span.as_deref()));
        }
        if let Some(text) = object.get(pair.second).and_then(Value::as_str)
            && !text.contains('/')
            && let Some(name) = text.strip_suffix(CLONE_SUFFIX)
            && accepts(root, pair.second_schema, name)
        {
            issues.push(fixed_issue(
                "schema.clone-suffix",
                field_path(path, pair.second),
                format!(
                    "\"{text}\" ends with a clone URL's {CLONE_SUFFIX}; send {}:\"{name}\"",
                    pair.second
                ),
                vec![(pair.second.to_owned(), Some(json!(name)))],
                Vec::new(),
                &json!(text),
                speculative,
            ));
        }
    }
    if speculative {
        return;
    }
    for issue in issues.iter_mut() {
        for pair in &pairs {
            split_pair(root, pair, object, path, issue);
        }
    }
    let covered = issues
        .iter()
        .flat_map(|issue| {
            let parent = &issue.path[..issue.path.len().saturating_sub(1)];
            set_fields(issue)
                .into_iter()
                .flat_map(Map::keys)
                .map(move |field| field_path(parent, field))
        })
        .collect::<Vec<_>>();
    issues.retain(|issue| {
        !(matches!(issue.rule_id.as_str(), "schema.required" | "schema.pattern")
            && set_fields(issue).is_none()
            && covered.contains(&issue.path))
    });
}

/// The fix an issue carries: the row fields to set (`null`: remove).
pub(super) fn set_fields(issue: &ValidationIssue) -> Option<&Map<String, Value>> {
    issue.schema.as_ref()?.get("setFields")?.as_object()
}

/// The pairs `properties` declares: a field naming its partner field.
fn pairs<'a>(root: &'a Value, properties: &'a Map<String, Value>) -> Vec<Pair<'a>> {
    properties
        .iter()
        .filter_map(|(first, schema)| {
            let first_schema = super::super::resolve_ref(schema, &root["$defs"]);
            let (second, second_schema) =
                properties.get_key_value(first_schema.get("x-joinedWith")?.as_str()?)?;
            Some(Pair {
                first,
                second,
                first_schema,
                second_schema: super::super::resolve_ref(second_schema, &root["$defs"]),
            })
        })
        .collect()
}

/// A pattern issue of one pair field holding `a/b`: carries the split.
fn split_pair(
    root: &Value,
    pair: &Pair,
    object: &Map<String, Value>,
    path: &[String],
    issue: &mut ValidationIssue,
) {
    let Some(field) = issue.path.last() else {
        return;
    };
    let other = match field.as_str() {
        _ if issue.rule_id != "schema.pattern" || issue.path[..issue.path.len() - 1] != *path => {
            return;
        }
        name if name == pair.first => pair.second,
        name if name == pair.second => pair.first,
        _ => return,
    };
    let Some((first, second)) = object
        .get(field)
        .and_then(Value::as_str)
        .and_then(|text| text.split_once('/'))
        .filter(|(first, second)| !first.is_empty() && !second.contains('/'))
    else {
        return;
    };
    let second = second.strip_suffix(CLONE_SUFFIX).unwrap_or(second);
    let half = if other == pair.first { first } else { second };
    if !accepts(root, pair.first_schema, first)
        || !accepts(root, pair.second_schema, second)
        || object
            .get(other)
            .is_some_and(|value| value.as_str() != Some(half))
    {
        return;
    }
    let fields = json!({ pair.first: first, pair.second: second });
    if let Some(schema) = issue.schema.as_mut().and_then(Value::as_object_mut) {
        schema.insert("setFields".into(), fields);
        issue.message = format!(
            "{}; send {}:\"{first}\", {}:\"{second}\"",
            issue.message, pair.first, pair.second
        );
    }
}

/// Whether `schema` accepts the string `text`.
fn accepts(root: &Value, schema: &Value, text: &str) -> bool {
    let _speculation = union::Speculation::enter();
    validate_with_mode(
        root,
        schema,
        &mut json!(text),
        &mut Vec::new(),
        ValidationMode::Input,
    )
    .is_ok()
}

fn field_path(parent: &[String], field: &str) -> Vec<String> {
    let mut path = parent.to_vec();
    path.push(field.to_owned());
    path
}

/// An issue whose schema carries its fix (`setFields`), unless speculative.
fn fixed_issue(
    rule_id: &str,
    path: Vec<String>,
    message: String,
    fields: Vec<(String, Option<Value>)>,
    approximate: Vec<String>,
    received: &Value,
    speculative: bool,
) -> ValidationIssue {
    let fix = (!speculative && !fields.is_empty()).then(|| {
        let set = fields
            .into_iter()
            .map(|(field, value)| (field, value.unwrap_or(Value::Null)))
            .collect::<Map<_, _>>();
        json!({"setFields": set, "approximateFields": approximate})
    });
    ValidationIssue {
        rule_id: rule_id.to_owned(),
        path,
        message,
        received: (!speculative).then(|| received.clone()),
        schema: fix,
    }
}

#[cfg(test)]
mod tests;
