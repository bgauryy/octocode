//! An ISO 8601 duration sent without its sign (`P1D`) where the field's
//! pattern takes a duration only back from now (`-P1D`) has one reading:
//! the row runs with the signed duration, and the note says the form to
//! send.
use super::ValidationIssue;
use serde_json::Value;

/// Repair a value its field's pattern refused that is a duration the same
/// pattern takes once signed: the row's index (when in a row) and the note.
pub(super) fn repair(
    input: &mut Value,
    issue: &ValidationIssue,
) -> Option<(Option<usize>, String)> {
    if issue.rule_id != "schema.pattern" {
        return None;
    }
    let sent = issue.received.as_ref()?.as_str()?;
    if !sent.starts_with('P') {
        return None;
    }
    let pattern = issue.schema.as_ref()?.get("pattern")?.as_str()?;
    let signed = format!("-{sent}");
    if !crate::contracts::validate::pattern_matches(pattern, &signed) {
        return None;
    }
    let slot = input.pointer_mut(&crate::json_pointer::of(&issue.path))?;
    if slot.as_str() != Some(sent) {
        return None;
    }
    *slot = Value::from(signed.as_str());
    let (index, label) = match issue.path.as_slice() {
        [queries, index, at @ ..] if queries == "queries" && !at.is_empty() => {
            (index.parse::<usize>().ok(), at.join("."))
        }
        at => (None, at.join(".")),
    };
    Some((
        index,
        format!(
            "read `{label}: \"{sent}\"` as `{label}: \"{signed}\"`, a duration back from now; send `{signed}` next time."
        ),
    ))
}
