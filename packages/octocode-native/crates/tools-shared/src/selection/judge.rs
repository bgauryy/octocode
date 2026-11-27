//! Judging a row's units: every unit is asked the caller's questions, in as
//! few provider requests as keep each answer about its own unit.
use serde_json::{Value, json};

mod requests;
mod units;

pub use units::{Plan, Spend, Unit, UnitAnswers, UnitCall};

fn template() -> Option<&'static Value> {
    octocode_runtime_base::contracts::parsed_contract()
        .ok()?
        .get("classifierTemplate")
}

/// The template's scope for a section question (core
/// `CLASSIFIER_TEMPLATE.sectionQuestion`): answer about the target section,
/// its context entries being background only. A section state carries it as
/// its `question`, as a shared hit state carries [`target_question`].
pub fn section_question() -> Option<&'static Value> {
    template()?
        .get("sectionQuestion")
        .filter(|question| question.is_string())
}

/// Compress only inline source coordinates, after candidate admission. Inclusive
/// runs preserve every coordinate, including omission markers and repeated lines.
fn provider_state(state: &Value) -> std::borrow::Cow<'_, Value> {
    let Some(lines) = state.get("sourceLines").and_then(Value::as_array) else {
        return std::borrow::Cow::Borrowed(state);
    };
    let mut ranges: Vec<[u64; 2]> = Vec::new();
    for value in lines {
        let Some(line) = value.as_u64() else {
            return std::borrow::Cow::Borrowed(state);
        };
        if let Some(last) = ranges.last_mut()
            && line > 0
            && last[1] > 0
            && last[1].checked_add(1) == Some(line)
        {
            last[1] = line;
        } else {
            ranges.push([line, line]);
        }
    }
    let mut compact = state.clone();
    if let Some(fields) = compact.as_object_mut() {
        fields.shift_remove("sourceLines");
        fields.insert("sourceLineRanges".into(), json!(ranges));
    }
    std::borrow::Cow::Owned(compact)
}

#[cfg(test)]
mod provider_tests;
#[cfg(test)]
mod units_tests;
