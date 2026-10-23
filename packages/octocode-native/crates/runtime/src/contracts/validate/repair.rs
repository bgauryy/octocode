//! The corrected call of a rejected input: the one `repair` hint of a
//! validation error. It is derived from the issues alone, so no tool or field
//! name appears here. Exact fixes keep what the caller asked:
//!
//! - an issue whose schema carries its fix (`setFields`: row fields to set
//!   or remove) gets exactly that: a joined `owner/repo` pair split, a
//!   repository web URL's parts, a `.git` suffix dropped (see
//!   `super::repository`), the omitted selector of the one form a field
//!   belongs to (`nameRegex` → `operation:"files"`);
//! - a path spelled from the current directory (`./src`) becomes the
//!   same path without it, when the field accepts that;
//! - a scalar the enum does not list becomes the one listed value it means
//!   (`super::enum_value`: case, separators, an on/off word, an
//!   abbreviation, a near spelling; `entryType:"file"` → `"f"`,
//!   `regex:true` → `"rust"`);
//! - an unknown field set to `true` whose name one row field's enum lists
//!   becomes that value (`merged:true` → `state:"merged"`);
//! - an unknown field named for a kind of value one row field takes
//!   (`x-valueKinds`: `branch:"main"` → `ref:"main"`) becomes that field with
//!   its value, unless the row already sets it;
//! - a number outside its bounds is clamped to the nearest bound, except a
//!   size or count of zero or less (a field whose minimum is positive). When
//!   the field's default is that minimum it is set to it (exact); else it is
//!   dropped, so the default applies (lossy, named). A field the row requires
//!   names what to read, so a value below its minimum gets no lead: any
//!   bound would be a guess, and the message names the allowed range;
//! - a typed value sent as JSON text becomes that value, after validation,
//!   and a whole number or boolean sent to a text field its text (`3` →
//!   `"3"`);
//! - a literal string becomes a single-item array only when that array
//!   satisfies the field schema;
//! - a line-range item spelled another way becomes its `start-end` span
//!   (`"3:6"`, `[3,6]` → `"3-6"`);
//! - a list sent to a row field that takes one value, every item of which
//!   that field accepts, becomes one row per item (the row's other fields
//!   copied into each), unless the row sets a field the value anchors
//!   (`x-anchoredBy`: a `line` names where one `symbolName` is);
//! - a field that applies only with a sibling the row lacks is removed (it
//!   had no effect);
//! - a `start…`/`end…` pair of line numbers becomes the one span of the
//!   row's span field (`startLine:5, endLine:9` → `ranges:["5-9"]`);
//! - an envelope field the rows declare moves into every row that lacks it,
//!   and one a row sets moves beside `queries` (rows that set it agree);
//! - a bare array of rows becomes the `{"queries":[...]}` envelope;
//! - a row that is itself an envelope (`{"queries":[...], ...}`) is replaced
//!   by its rows: its envelope fields (`responseLength`, …) move beside
//!   `queries` and its other fields into each of its rows.
//!
//! The issues those edits reveal (the lifted rows are validated for the
//! first time) are repaired in a further pass, so one lead fixes all. An
//! issue at a path another edit of the same pass sets (a missing `repo`
//! the `owner:"a/b"` split fills) needs no fix of its own.
//!
//! Dropping any other unknown field, an array value the schema's enum does
//! not list (the field goes with its last value), or a field of another
//! form when the row's own selector picks its form, loses a constraint the
//! caller set, so that repair is lossy: it names the dropped fields and the
//! lead must say so. A fix that reads a value approximately (a URL's ref,
//! which may hold `/`) names those fields too. An unknown field whose name
//! is a near miss of a known one, or that a known field expresses
//! (`x-covers`), is never dropped (the caller meant that one).
//! Any other issue (a missing field, an unlisted scalar no rule maps) needs
//! a value only the caller knows, so the call has no lead and the issue's
//! message names the fix. The repaired input must also pass the caller's
//! admission, so a lead never names a call that is rejected again.

mod apply;
mod fixes;
mod respell;
use apply::{apply, pointer};
use fixes::fix;
use respell::{
    bound_value, dot_relative, kind_respelling, line_range, name_respelling, span_respelling,
};
use serde_json::Value;

use super::ValidationIssue;

/// One edit at a path (object keys and array indices).
enum Edit {
    Set(Vec<String>, Value),
    Remove(Vec<String>),
    /// A constraint the caller set, removed: the repair is lossy.
    Drop(Vec<String>),
    /// An envelope field moved into every row that lacks it.
    IntoRows(String),
    /// A bare array of rows wrapped as the `queries` envelope.
    Wrap,
    /// Row `index` replaced by its own rows; its `envelope` fields move
    /// beside `queries`, its other fields into each lifted row.
    Flatten {
        index: usize,
        envelope: Vec<String>,
    },
    /// A row field at `path` (`queries.N.field`) that the envelope takes,
    /// moved beside `queries`.
    Lift(Vec<String>),
    /// Row `index` sent a list to `field`, which takes one value: one row
    /// per item, each with the row's other fields.
    Split {
        index: usize,
        field: String,
    },
    /// A field removed and the field that says it set beside it.
    Respell {
        remove: Vec<String>,
        set: Vec<String>,
        value: Value,
    },
    /// Fields of the object at `parent` set together (`None`: removed);
    /// `approximate` names those whose value is a best reading.
    Fields {
        parent: Vec<String>,
        fields: Vec<(String, Option<Value>)>,
        approximate: Vec<String>,
    },
}

impl Edit {
    /// The paths this edit gives a value (or removes as part of a fix).
    fn sets(&self) -> Vec<Vec<String>> {
        let child = |parent: &[String], field: &str| {
            let mut path = parent.to_vec();
            path.push(field.to_owned());
            path
        };
        match self {
            Edit::Set(path, _) => vec![path.clone()],
            Edit::Respell { set, .. } => vec![set.clone()],
            Edit::Lift(path) => path
                .last()
                .map(|field| vec![field.clone()])
                .into_iter()
                .collect(),
            Edit::Fields { parent, fields, .. } => fields
                .iter()
                .map(|(field, _)| child(parent, field))
                .collect(),
            _ => Vec::new(),
        }
    }
}

/// Further passes for issues a pass's edits reveal.
const PASSES: usize = 3;

/// A repaired input, the caller's fields it dropped, and the fields it set
/// to a best reading (both empty: exact).
#[derive(Debug, PartialEq)]
pub(crate) struct Repair {
    pub input: Value,
    pub dropped: Vec<String>,
    pub(crate) approximate: Vec<String>,
}

/// `input` with every issue fixed, or `None` when any issue has no fix or
/// `check` refuses the result with no issue left to fix. `check` returns
/// the issues of a candidate (empty: refused for a reason no edit fixes);
/// the issues a pass reveals are fixed by the next. A relocation the rows
/// do not accept falls back to dropping the field (lossy, named at the
/// caller's envelope path).
pub(crate) fn repair_input(
    input: &Value,
    issues: &[ValidationIssue],
    check: impl Fn(&Value) -> Result<(), Vec<ValidationIssue>>,
) -> Option<Repair> {
    repair_pass(input, issues, &check, PASSES)
}

fn repair_pass(
    input: &Value,
    issues: &[ValidationIssue],
    check: &dyn Fn(&Value) -> Result<(), Vec<ValidationIssue>>,
    passes: usize,
) -> Option<Repair> {
    if issues.is_empty() || passes == 0 {
        return None;
    }
    let fixes = issues
        .iter()
        .map(|issue| fix(input, issue))
        .collect::<Vec<_>>();
    // An issue with no fix of its own is settled when another edit of this
    // pass sets its path (the missing `repo` an `owner:"a/b"` split sets).
    let set = fixes
        .iter()
        .flatten()
        .flat_map(Edit::sets)
        .collect::<Vec<_>>();
    let edits = issues
        .iter()
        .zip(fixes)
        .map(|(issue, fix)| match fix {
            Some(edit) => Some(Some(edit)),
            None => set.contains(&issue.path).then_some(None),
        })
        .collect::<Option<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    let relocated: Vec<&str> = edits
        .iter()
        .filter_map(|edit| match edit {
            Edit::IntoRows(field) => Some(field.as_str()),
            _ => None,
        })
        .collect();
    let attempt = |preference: Preference| {
        let repair = apply(input, &edits, preference)?;
        match check(&repair.input) {
            Ok(()) => Some(repair),
            Err(revealed) => {
                let mut rest = repair_pass(&repair.input, &revealed, check, passes - 1)?;
                for (named, earlier) in [
                    (&mut rest.dropped, repair.dropped),
                    (&mut rest.approximate, repair.approximate),
                ] {
                    named.extend(earlier);
                    named.sort();
                    named.dedup();
                }
                Some(rest)
            }
        }
    };
    // A relocated field a later pass drops from the rows was not accepted
    // there: drop it where the caller set it instead, so the lead names it.
    let rejected_relocation = |repair: &Repair| {
        repair.dropped.iter().any(|path| {
            let path: Vec<&str> = path.split('.').collect();
            matches!(path.as_slice(), ["queries", _, field] if relocated.contains(field))
        })
    };
    // The preferred form first, then the fallback of the edits that have one.
    let mut refused = None;
    for relocate in if relocated.is_empty() {
        vec![true]
    } else {
        vec![true, false]
    } {
        let Some(repair) = attempt(Preference { relocate }) else {
            continue;
        };
        if relocate && rejected_relocation(&repair) {
            refused.get_or_insert(repair);
            continue;
        }
        return Some(repair);
    }
    refused
}

/// Which form of the edits that have a fallback a pass applies: preferred,
/// an envelope field relocates into the rows (else it is dropped).
#[derive(Clone, Copy)]
struct Preference {
    relocate: bool,
}

#[cfg(test)]
mod tests;
