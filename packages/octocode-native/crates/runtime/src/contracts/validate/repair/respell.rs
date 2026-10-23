use super::super::ValidationIssue;
use super::{Edit, pointer};
use serde_json::{Value, json};

/// A `start…`/`end…` pair of line numbers (`startLine`/`endLine`,
/// `lineStart`/`lineEnd`) the row's span field (`ranges`) spells exactly:
/// the start becomes that field's one span, the end goes. `None` when the
/// pair is incomplete or not an ascending pair of positive integers, or the
/// row already sets the span field.
pub(super) fn span_respelling(input: &Value, issue: &ValidationIssue) -> Option<Edit> {
    let span = issue.schema.as_ref()?.get("spanField")?.as_str()?;
    let (field, parent) = issue.path.split_last()?;
    let row = pointer(input, parent)?.as_object()?;
    if row.contains_key(span) {
        return None;
    }
    let side = |name: &str| {
        let lower = name.to_ascii_lowercase();
        [("start", true), ("end", false)]
            .into_iter()
            .find_map(|(word, start)| {
                let stem = lower
                    .strip_prefix(word)
                    .or_else(|| lower.strip_suffix(word))?;
                Some((start, stem.to_owned()))
            })
    };
    let (is_start, stem) = side(field)?;
    let (_, other_value) = row.iter().find(|(name, _)| {
        name.as_str() != field.as_str()
            && side(name).is_some_and(|(start, other)| start != is_start && other == stem)
    })?;
    let line = |value: &Value| value.as_u64().filter(|line| *line > 0);
    let (start, end) = if is_start {
        (line(&row[field.as_str()])?, line(other_value)?)
    } else {
        (line(other_value)?, line(&row[field.as_str()])?)
    };
    if start > end {
        return None;
    }
    Some(if is_start {
        let mut set = parent.to_vec();
        set.push(span.to_owned());
        Edit::Respell {
            remove: issue.path.clone(),
            set,
            value: serde_json::json!([format!("{start}-{end}")]),
        }
    } else {
        Edit::Remove(issue.path.clone())
    })
}

/// An unknown field set to `true` whose name the enum of one row field lists
/// (`nameFields`): that field set to the name (`merged:true` →
/// `state:"merged"`). `None` when the row already sets that field.
pub(super) fn name_respelling(input: &Value, issue: &ValidationIssue) -> Option<Edit> {
    let schema = issue.schema.as_ref()?;
    let [target] = schema.get("nameFields")?.as_array()?.as_slice() else {
        return None;
    };
    let (field, parent) = issue.path.split_last()?;
    let row = pointer(input, parent)?.as_object()?;
    let target = target.as_str()?;
    if row.get(field) != Some(&Value::Bool(true)) || row.contains_key(target) {
        return None;
    }
    let mut set = parent.to_vec();
    set.push(target.to_owned());
    Some(Edit::Respell {
        remove: issue.path.clone(),
        set,
        value: Value::from(field.as_str()),
    })
}

/// An unknown field named for a kind of value one row field takes
/// (`kindFields`, from the field's `x-valueKinds`): that field set to the
/// value (`branch:"main"` → `ref:"main"`). `None` when the row already sets
/// that field, so a conflicting value is never overwritten.
pub(super) fn kind_respelling(input: &Value, issue: &ValidationIssue) -> Option<Edit> {
    let schema = issue.schema.as_ref()?;
    let [target] = schema.get("kindFields")?.as_array()?.as_slice() else {
        return None;
    };
    let (field, parent) = issue.path.split_last()?;
    let row = pointer(input, parent)?.as_object()?;
    let target = target.as_str()?;
    if row.contains_key(target) {
        return None;
    }
    let mut set = parent.to_vec();
    set.push(target.to_owned());
    Some(Edit::Respell {
        remove: issue.path.clone(),
        set,
        value: row.get(field)?.clone(),
    })
}

/// A line-range item (`ranges.N`, whose schema is the line-range pattern)
/// spelled another way, or one such scalar sent for the whole list: its
/// `start-end` span (`"3:6"` → `"3-6"`, `ranges:"3:6"` → `["3-6"]`).
pub(super) fn line_range(issue: &ValidationIssue) -> Option<Edit> {
    if !matches!(issue.rule_id.as_str(), "schema.pattern" | "schema.type") {
        return None;
    }
    let schema = issue.schema.as_ref()?;
    let received = issue.received.as_ref()?;
    let is_range = |schema: &Value| {
        schema.get("pattern") == Some(&json!(super::super::shape::LINE_RANGE_PATTERN))
    };
    let span = super::super::shape::line_range_reading(received)?;
    if is_range(schema) {
        return Some(Edit::Set(issue.path.clone(), json!(span)));
    }
    (is_range(schema.get("items")?) && !received.is_array())
        .then(|| Edit::Set(issue.path.clone(), json!([span])))
}

/// A path spelled from the current directory (`./src`, `././a.rs`) that the
/// field's pattern rejects, when the same path without the `./` is
/// accepted: that path, which names the same file.
pub(super) fn dot_relative(issue: &ValidationIssue) -> Option<Edit> {
    if issue.rule_id != "schema.pattern" {
        return None;
    }
    let schema = issue.schema.as_ref()?;
    let text = issue.received.as_ref()?.as_str()?;
    let mut bare = text.strip_prefix("./")?;
    while let Some(rest) = bare.strip_prefix("./") {
        bare = rest;
    }
    let mut value = Value::from(bare);
    super::super::validate_input_schema(schema, schema, &mut value, &mut Vec::new()).ok()?;
    Some(Edit::Set(issue.path.clone(), value))
}

/// A bound as the number the schema wrote: integral bounds stay integers.
pub(super) fn bound_value(bound: f64) -> Value {
    if bound.fract() == 0.0 && bound.abs() < 9.0e15 {
        Value::from(bound as i64)
    } else {
        Value::from(bound)
    }
}
