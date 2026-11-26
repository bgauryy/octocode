use super::sample::{placeholder, sample, sampled, schemas_at};
use super::{ContractValidationError, ValidationIssue, query_label, sibling_requirement};
use serde_json::Value;

/// Projects transport-neutral validation issues into the stable tool-error
/// envelope. `sent` is the request as the caller sent it: the example
/// shown is of the form its rejected row comes nearest. Every surface
/// receives the same cause and repair guidance.
#[must_use]
pub fn format_input_error(tool_name: &str, error: &ContractValidationError, sent: &Value) -> Value {
    // A request field inside a row explains every other issue of that row
    // (the row lacks its own fields because it was never a row).
    let misplaced = error
        .issues
        .iter()
        .filter(|issue| misplaced_request_field(tool_name, issue).is_some())
        .filter_map(|issue| issue.path.get(..2))
        .collect::<Vec<_>>();
    let renamed = renamed_row(tool_name, error, sent);
    let satisfied = sent_row(error, sent)
        .map(|rejected| satisfied_by_another_fix(tool_name, &rejected))
        .unwrap_or_default();
    let mut issues = error
        .issues
        .iter()
        .filter(|issue| {
            misplaced_request_field(tool_name, issue).is_some()
                || !misplaced.iter().any(|row| issue.path.starts_with(row))
        })
        // A rule the fix of another one satisfies is no cause of its own.
        .filter(|issue| !satisfied.contains(&issue.path))
        // A field a rename supplies is not also missing.
        .filter(|issue| {
            !(issue.rule_id == "schema.required"
                && renamed
                    .as_ref()
                    .is_some_and(|renamed| renamed.supplies(&issue.path)))
        })
        .collect::<Vec<_>>();
    // A request that mixes the form a field holds alone (`cursor`) with
    // queries is answered by that rule first: its rows' issues matter only
    // to a new request.
    if let Some(at) = issues
        .iter()
        .position(|issue| mixed_with_whole_request(tool_name, issue).is_some())
    {
        issues[..=at].rotate_right(1);
    }
    if let Some(at) = sent_value_lead(&issues) {
        issues[..=at].rotate_right(1);
    }
    // A value the input guard refused (a credential, an oversized fragment)
    // sits in a well-formed call: its cause names the whole repair.
    let value_refused = !issues.is_empty()
        && issues
            .iter()
            .all(|issue| issue.rule_id.starts_with("security."));
    // A source anchor or a time bound needs a caller-supplied value. The
    // diagnostic already names how to obtain or format it.
    let needs_input = issues.iter().any(|issue| {
        issue.rule_id == "schema.pattern"
            && issue
                .schema
                .as_ref()
                .and_then(|schema| schema["format"].as_str())
                == Some("date-time")
            || tool_name == "lspSearch"
                && issue.rule_id == "schema.required"
                && issue.path.get(2).is_some_and(|field| field == "line")
                && issue
                    .path
                    .get(1)
                    .and_then(|index| index.parse::<usize>().ok())
                    .and_then(|index| sent["queries"].get(index))
                    .is_some_and(|row| row["path"].is_string() || row["symbolName"].is_string())
    });
    let merged_state = issues
        .iter()
        .copied()
        .find(|issue| issue.rule_id == "history.pull-request-state");
    let details = issues
        .into_iter()
        .map(|issue| {
            let path = issue.path.join(".");
            // One wording for an unknown field, alone or beside other issues.
            if issue.rule_id == "schema.unknown-field" {
                return unknown_field_message(
                    tool_name,
                    issue,
                    Row {
                        unselected: unselected_row(error, issue),
                        operation: row_operation(issue, sent),
                        request: sent,
                    },
                );
            }
            if let Some(message) = mixed_with_whole_request(tool_name, issue) {
                return message;
            }
            if issue.rule_id == "schema.union"
                && let Some(forms) = forms_clause(tool_name, issue, &issue.path)
            {
                return format!("{path}: {}; {forms}.", issue.message);
            }
            let message = match required_guidance(issue) {
                Some(guidance) => format!("{} ({guidance})", issue.message),
                None => match picked_by(tool_name, issue, sent) {
                    Some(fields) => format!("{} ({} needs it)", issue.message, fields.join(", ")),
                    None => issue.message.clone(),
                },
            };
            if path.is_empty() {
                message
            } else {
                format!("{path}: {message}")
            }
        })
        .collect::<Vec<_>>();
    // A request field sent inside a row, or a refused value, has one repair,
    // stated whole: a fresh example beside it would point away from it.
    let reference = if !misplaced.is_empty() || value_refused || needs_input {
        Vec::new()
    } else if let Some(issue) = merged_state {
        merged_state_example(tool_name, sent, issue)
            .into_iter()
            .collect()
    } else {
        match renamed {
            None => call_reference(tool_name, sent_row(error, sent).as_ref()),
            // A value that does not carry over to the field meant: an
            // example without the field would run something else.
            Some(Renamed { request: None, .. }) => Vec::new(),
            Some(Renamed {
                request: Some(request),
                ..
            }) => match super::validate(tool_name, request.clone()) {
                Ok(_) => vec![format!("{EXAMPLE}{request}")],
                // What else the row gets wrong, in the renamed row's terms.
                Err(rest) => call_reference(tool_name, sent_row(&rest, &request).as_ref()),
            },
        }
    };
    issues_error(details, reference)
}

/// Retain the explicit merged selection and drop the conflicting state.
fn merged_state_example(tool: &str, sent: &Value, issue: &ValidationIssue) -> Option<String> {
    let [queries, index, fields @ .., _] = issue.path.as_slice() else {
        return None;
    };
    if queries != "queries" {
        return None;
    }
    let mut row = sent[queries].get(index.parse::<usize>().ok()?)?.clone();
    row.pointer_mut(&crate::json_pointer::of(fields))?
        .as_object_mut()?
        .shift_remove("state");
    let request = serde_json::json!({ "queries": [row] });
    super::validate(tool, request.clone()).ok()?;
    Some(format!("{EXAMPLE}{request}"))
}

/// The first rejected row with each field its issues refuse by one declared
/// name (`…; use '<field>'.`) sent under that name.
struct Renamed {
    /// The row's path (`queries`, `<n>`).
    row: Vec<String>,
    /// The declared fields the renames supply.
    fields: Vec<String>,
    /// The request of that row alone, each renamed field holding the value
    /// sent, as it is or in the one typed reading its schema gives
    /// ([`super::repair::lenient_value`]: the reading a declared field gets);
    /// `None` when a value carries over in neither.
    request: Option<Value>,
}

impl Renamed {
    /// Whether a rename supplies the field at `path`.
    fn supplies(&self, path: &[String]) -> bool {
        path.split_last()
            .is_some_and(|(field, row)| row == self.row.as_slice() && self.fields.contains(field))
    }
}

/// The one declared field an unknown field's `message` says to use
/// ([`unknown_field_message`]); `None` for a choice of several or any other
/// repair.
fn meant_field(message: &str) -> Option<&str> {
    let (_, after) = message.split_once("; use '")?;
    after
        .strip_suffix("'.")
        .filter(|field| !field.contains('\''))
}

/// [`Renamed`] of the first rejected row of `sent`; `None` when none of its
/// fields is refused by one declared name.
fn renamed_row(tool_name: &str, error: &ContractValidationError, sent: &Value) -> Option<Renamed> {
    let [queries, index, ..] = error.issues.first()?.path.as_slice() else {
        return None;
    };
    let row = sent
        .get(queries)?
        .get(index.parse::<usize>().ok()?)?
        .as_object()?;
    let renames = error
        .issues
        .iter()
        .filter(|issue| issue.rule_id == "schema.unknown-field")
        .filter_map(|issue| {
            let [at, n, field] = issue.path.as_slice() else {
                return None;
            };
            if at != queries || n != index {
                return None;
            }
            let message = unknown_field_message(
                tool_name,
                issue,
                Row {
                    unselected: unselected_row(error, issue),
                    operation: row_operation(issue, sent),
                    request: sent,
                },
            );
            Some((field.as_str(), meant_field(&message)?.to_owned()))
        })
        .collect::<Vec<_>>();
    if renames.is_empty() {
        return None;
    }
    let state = crate::id::ToolId::from_name(tool_name).map(crate::contracts::state_fields);
    // The call's brief and page position describe the call, not its form.
    let mut shown = row
        .iter()
        .filter(|(field, _)| {
            !crate::contracts::BRIEF_FIELDS.contains(&field.as_str())
                && !state.is_some_and(|state| state.contains(*field))
        })
        .map(|(field, value)| (field.clone(), value.clone()))
        .collect::<serde_json::Map<_, _>>();
    let mut carried = true;
    for (field, meant) in &renames {
        let Some(value) = row.get(*field) else {
            continue;
        };
        // In place, so the row reads as it was sent.
        let with = |shown: &serde_json::Map<String, Value>, value: &Value| {
            shown
                .iter()
                .map(|(name, held)| {
                    if name == field {
                        (meant.clone(), value.clone())
                    } else {
                        (name.clone(), held.clone())
                    }
                })
                .collect::<serde_json::Map<_, _>>()
        };
        let stands = |candidate: &serde_json::Map<String, Value>| match super::validate(
            tool_name,
            serde_json::json!({ "queries": [candidate] }),
        ) {
            Ok(_) => true,
            Err(error) => !error
                .issues
                .iter()
                .any(|issue| issue.path.get(2) == Some(meant)),
        };
        match readings(tool_name, meant, value)
            .iter()
            .map(|reading| with(&shown, reading))
            .find(stands)
        {
            Some(next) => shown = next,
            None => carried = false,
        }
    }
    Some(Renamed {
        row: vec![queries.clone(), index.clone()],
        fields: renames.into_iter().map(|(_, meant)| meant).collect(),
        request: carried.then(|| serde_json::json!({ "queries": [shown] })),
    })
}

/// `sent` as the row field `field` of `tool_name` may hold it: as it is,
/// then in the one typed reading each schema of the field gives.
fn readings(tool_name: &str, field: &str, sent: &Value) -> Vec<Value> {
    let mut readings = vec![sent.clone()];
    let Some(tool) = crate::id::ToolId::from_name(tool_name)
        .and_then(|id| crate::contracts::tool_contract(id).ok())
    else {
        return readings;
    };
    let schema = super::super::resolve_ref(&tool["querySchema"], &Value::Null);
    let defs = &schema["$defs"];
    for schema in schemas_at(schema, defs, &[field.to_owned()]) {
        if let Some(read) = super::repair::lenient_value(schema, defs, sent)
            && !readings.contains(&read)
        {
            readings.push(read);
        }
    }
    readings
}

/// The line a call reference starts with: one request the tool accepts.
const EXAMPLE: &str = "Example: ";

/// What repairs a rejected call when its causes alone do not: one request
/// the tool accepts, core's example of the form that takes the most fields
/// of the row sent, among the forms of the `operation` it names (the first
/// such form on a tie; with nothing sent, the tool's first form, the one
/// `schema <tool> --view variants` shows), in the row's own terms
/// ([`in_row_terms`]). Stated once per rejected call or row, after its
/// causes; empty for a name that is no tool, and left out where a misplaced
/// request field or a refused value (`security.*`) is the whole repair.
fn call_reference(tool_name: &str, rejected: Option<&Rejected<'_>>) -> Vec<String> {
    if tool_name == "clasify"
        && let Some(rejected) = rejected
        && rejected.faulted.iter().any(|field| field == "questions")
        && let Some(questions) = rejected.row.get("questions")
        && questions.as_object().is_none_or(|named| {
            named
                .values()
                .any(|item| item.is_string() || item.get("question").is_some())
        })
    {
        return question_example(rejected.row, questions)
            .into_iter()
            .collect();
    }
    // Fields another form admits are that form's: the row runs under it,
    // every field it asked kept, rather than stripped of them.
    if let Some(switched) = rejected.and_then(|rejected| switched_row(tool_name, rejected)) {
        return vec![format!(
            "{EXAMPLE}{}",
            serde_json::json!({ "queries": [switched] })
        )];
    }
    let row = rejected.map(|rejected| rejected.row);
    let example = rejected.and_then(|rejected| with_missing_fields(tool_name, rejected));
    let example = match example {
        Some(example) => example,
        None => {
            let form = tool_variants(tool_name)
                .and_then(|variants| nearest_form(variants, row)?.get("example"));
            let Some(form) = form else {
                return Vec::new();
            };
            in_row_terms(tool_name, form, rejected)
        }
    };
    // A required value the row never sent is shown only as a visible
    // placeholder ([`placeholder`]) or a literal its schema names. A number
    // has no placeholder: any number reads as a real one (issue `1`), so
    // such a refusal shows no example and names the field.
    if rejected.is_some_and(|rejected| {
        rejected.missing.iter().any(|field| {
            example
                .get(field)
                .is_some_and(|shown| shown.is_number() && !rejected.row.contains_key(field))
        })
    }) {
        return Vec::new();
    }
    // An example applies the fix its causes state. Where a cause states
    // the form to send and the example does not hold the value sent (a
    // number for an object of several members: which one?), only the cause
    // can state the fix.
    if rejected.is_some_and(|rejected| !rejected.holds_mistyped(&example)) {
        return Vec::new();
    }
    // A sample question in a form may illustrate its shape, but cannot
    // stand in for a named question the caller actually sent.
    if tool_name == "clasify"
        && let Some(named) = row
            .and_then(|row| row.get("questions"))
            .and_then(Value::as_object)
        && named.iter().any(|(name, original)| {
            let Some(shown) = example["questions"].get(name) else {
                return true;
            };
            original
                .get("instructions")
                .is_some_and(|text| shown.get("instructions") != Some(text))
        })
    {
        return Vec::new();
    }
    vec![format!(
        "{EXAMPLE}{}",
        serde_json::json!({ "queries": [example] })
    )]
}

/// Plain questions use the same yes/no form as classifier predicates. Keep
/// supplied names and typed questions; an unnamed list gets stable ordinal
/// names. Only replay this row: other rows in a batch may already have run.
fn question_example(row: &serde_json::Map<String, Value>, questions: &Value) -> Option<String> {
    let question = |value: &Value| match value {
        Value::String(text) => Some(serde_json::json!({"type":"noul", "instructions":text})),
        Value::Object(fields) if fields.contains_key("question") => {
            if fields.contains_key("instructions") {
                return None;
            }
            let mut fields = fields.clone();
            let text = fields.shift_remove("question")?;
            fields.insert("instructions".into(), text);
            Some(Value::Object(fields))
        }
        Value::Object(_) => Some(value.clone()),
        _ => None,
    };
    let mut named = serde_json::Map::new();
    match questions {
        Value::String(_) => {
            named.insert("q1".into(), question(questions)?);
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                named.insert(format!("q{}", index + 1), question(item)?);
            }
        }
        Value::Object(fields) => {
            for (key, item) in fields {
                named.insert(key.clone(), question(item)?);
            }
        }
        _ => return None,
    }
    let mut retry = row.clone();
    retry.insert("questions".into(), Value::Object(named));
    let request = serde_json::json!({"queries":[retry]});
    super::validate("clasify", request.clone()).ok()?;
    Some(format!("{EXAMPLE}{request}"))
}

/// The rejected row itself, when all it gets wrong is lacking required row
/// fields: each one set to a value its schema accepts ([`sample`]: a
/// placeholder for text, the first literal of a choice), every field sent
/// kept (its brief and page position aside). `None` when the row has any
/// other fault or the result is still refused.
fn with_missing_fields(tool_name: &str, rejected: &Rejected<'_>) -> Option<Value> {
    if rejected.faults.is_empty()
        || !rejected
            .faults
            .iter()
            .all(|fault| matches!(fault.as_slice(), [field] if !rejected.row.contains_key(field)))
    {
        return None;
    }
    let tool = crate::id::ToolId::from_name(tool_name)?;
    let contract = crate::contracts::tool_contract(tool).ok()?;
    let schema = super::super::resolve_ref(&contract["querySchema"], &Value::Null);
    let defs = &schema["$defs"];
    let state = crate::contracts::state_fields(tool);
    let mut shown = rejected
        .row
        .iter()
        .filter(|(field, _)| {
            !crate::contracts::BRIEF_FIELDS.contains(&field.as_str()) && !state.contains(*field)
        })
        .map(|(field, value)| (field.clone(), value.clone()))
        .collect::<serde_json::Map<_, _>>();
    for field in &rejected.missing {
        let value = schemas_at(schema, defs, std::slice::from_ref(field))
            .into_iter()
            .find_map(|schema| sample(schema, defs, field))?;
        shown.insert(field.clone(), value);
    }
    let accepted = |row: &serde_json::Map<String, Value>| {
        super::validate(tool_name, serde_json::json!({ "queries": [row] })).is_ok()
    };
    if !accepted(&shown) {
        return None;
    }
    // A field the fix of an earlier one makes needless is left out, the
    // later rules first: `owner`, which `repo` requires, also makes a
    // search runnable, so no keyword placeholder joins it.
    for field in rejected.missing.iter().rev() {
        let mut without = shown.clone();
        without.shift_remove(field);
        if accepted(&without) {
            shown = without;
        }
    }
    Some(Value::Object(shown))
}

/// The rejected row under the selector values that admit every field its
/// issues refuse ([`Rejected::switch`]), its brief and page position aside.
/// A field the switched form refuses (a list's `sort` by `pushedAt` under
/// a search) is left out while that leaves fewer issues; the fields the
/// switch is for always stay. `None` when no one switch admits them or the
/// switched row stays refused.
fn switched_row(tool_name: &str, rejected: &Rejected<'_>) -> Option<Value> {
    let switch = rejected.switch?;
    let tool = crate::id::ToolId::from_name(tool_name)?;
    let state = crate::contracts::state_fields(tool);
    let mut shown = rejected
        .row
        .iter()
        .filter(|(field, _)| {
            !crate::contracts::BRIEF_FIELDS.contains(&field.as_str()) && !state.contains(*field)
        })
        .map(|(field, value)| (field.clone(), value.clone()))
        .collect::<serde_json::Map<_, _>>();
    for (selector, literal) in switch {
        shown.insert(selector.clone(), literal.clone());
    }
    let issues = |row: &serde_json::Map<String, Value>| {
        super::validate(tool_name, serde_json::json!({ "queries": [row] }))
            .err()
            .map_or(0, |error| error.issues.len())
    };
    let mut left = issues(&shown);
    while left > 0 {
        let (fewer, count) = shown
            .keys()
            .filter(|field| !switch.contains_key(*field) && !rejected.faulted.contains(*field))
            .find_map(|field| {
                let mut without = shown.clone();
                without.shift_remove(field);
                let count = issues(&without);
                (count < left).then_some((without, count))
            })?;
        shown = fewer;
        left = count;
    }
    Some(Value::Object(shown))
}

/// The paths of the rejected row's missing-field issues that the repair
/// ([`with_missing_fields`]) leaves out: the fix of another rule already
/// satisfies them, so they are no cause of their own.
fn satisfied_by_another_fix(tool_name: &str, rejected: &Rejected<'_>) -> Vec<Vec<String>> {
    let Some(repaired) = with_missing_fields(tool_name, rejected) else {
        return Vec::new();
    };
    let satisfied = rejected
        .missing
        .iter()
        .filter(|field| repaired.get(field.as_str()).is_none())
        .map(|field| vec!["queries".to_owned(), rejected.index.clone(), field.clone()])
        .collect::<Vec<_>>();
    // Some fix is the cause the others give way to.
    if satisfied.len() < rejected.missing.len() {
        satisfied
    } else {
        Vec::new()
    }
}

/// The forms `tool_name` publishes (`schema <tool> --view variants`): each
/// one's `operations`, `fields`, `requires` and `example`.
fn tool_variants(tool_name: &str) -> Option<&'static [Value]> {
    let presentation = crate::contracts::cli_presentation().ok()?;
    let entry = presentation["tools"]
        .as_array()?
        .iter()
        .find(|entry| entry["name"] == tool_name)?;
    entry["views"]["variants"]["variants"]
        .as_array()
        .map(Vec::as_slice)
}

/// The row fields that picked the form a missing required field belongs
/// to: each one only forms that require the field take, sent in the row
/// that lacks it (clasify `perHit` needs `source`). `None` when the field
/// is required whatever the row sends, no sent field decided it, or the
/// issue already states why (the union names the sent field and the value
/// it requires, [`super::union`]): one reason, once.
fn picked_by(tool_name: &str, issue: &ValidationIssue, sent: &Value) -> Option<Vec<String>> {
    if issue.rule_id != "schema.required" {
        return None;
    }
    let [queries, index, field] = issue.path.as_slice() else {
        return None;
    };
    let row = sent
        .get(queries)?
        .get(index.parse::<usize>().ok()?)?
        .as_object()?;
    if row.contains_key(field) || issue.message != super::schema::missing_required_text(field) {
        return None;
    }
    let names = |variant: &Value, list: &str, name: &str| {
        variant[list]
            .as_array()
            .is_some_and(|names| names.iter().any(|known| known == name))
    };
    let operation = row.get("operation").and_then(Value::as_str);
    let variants = tool_variants(tool_name)?
        .iter()
        .filter(|variant| operation.is_none_or(|operation| names(variant, "operations", operation)))
        .collect::<Vec<_>>();
    let requires = |variant: &&Value| names(variant, "requires", field);
    if variants.iter().all(requires) {
        return None;
    }
    let picked = row
        .keys()
        .filter(|sent| {
            *sent != "operation"
                && !crate::contracts::BRIEF_FIELDS.contains(&sent.as_str())
                && variants
                    .iter()
                    .any(|variant| names(variant, "fields", sent))
                && variants
                    .iter()
                    .filter(|variant| names(variant, "fields", sent))
                    .all(requires)
        })
        .cloned()
        .collect::<Vec<_>>();
    (!picked.is_empty()).then_some(picked)
}

/// The row's own `value` of a field whose issues all lie inside it, as the
/// candidates an example may show, the plainest first: each key at fault
/// removed (`source.query.bogusField`); then with the keys the row lacks, filled with a visible placeholder
/// ([`placeholder`]) as text or a list of text, and an `http://` value
/// removed for its scheme spelled `https://` when its host is a name
/// ([`names_reachable_host`]). Empty when an issue names the
/// field itself or a fault is not an object key.
fn without_faults(value: &Value, faults: &[&[String]]) -> Vec<Value> {
    let mut plain = value.clone();
    let mut missing = Vec::new();
    let mut upgraded = Vec::new();
    for fault in faults {
        let Some((leaf, parents)) = fault.split_last() else {
            return Vec::new();
        };
        if parents.is_empty() {
            return Vec::new();
        }
        let mut at = &mut plain;
        for segment in &parents[1..] {
            at = match at {
                Value::Object(fields) => match fields.get_mut(segment) {
                    Some(next) => next,
                    None => return Vec::new(),
                },
                Value::Array(items) => match segment
                    .parse::<usize>()
                    .ok()
                    .and_then(|index| items.get_mut(index))
                {
                    Some(next) => next,
                    None => return Vec::new(),
                },
                _ => return Vec::new(),
            };
        }
        let Some(fields) = at.as_object_mut() else {
            return Vec::new();
        };
        let removed = fields.shift_remove(leaf);
        if let Some(Value::String(text)) = &removed
            && let Some(rest) = text.strip_prefix("http://")
            && names_reachable_host(text)
        {
            upgraded.push((fault.to_vec(), Value::String(format!("https://{rest}"))));
        }
        if removed.is_none() {
            missing.push(fault.to_vec());
        }
    }
    let set = |base: &Value, path: &[String], leaf: Value| {
        let mut value = base.clone();
        let mut at = &mut value;
        for segment in &path[1..path.len() - 1] {
            at = match at {
                Value::Object(fields) => fields.get_mut(segment)?,
                Value::Array(items) => items.get_mut(segment.parse::<usize>().ok()?)?,
                _ => return None,
            };
        }
        at.as_object_mut()?.insert(path.last()?.clone(), leaf);
        Some(value)
    };
    let mut candidates = vec![plain.clone()];
    let mut with_upgrades = plain.clone();
    for (path, scheme) in &upgraded {
        with_upgrades = set(&with_upgrades, path, scheme.clone()).unwrap_or(with_upgrades);
    }
    if !upgraded.is_empty() {
        candidates.push(with_upgrades);
    }
    for text in [true, false] {
        let mut shown = plain.clone();
        for path in &missing {
            let name = path.last().map(String::as_str).unwrap_or_default();
            let placeholder = if text {
                placeholder(name, &Value::String(String::new()))
            } else {
                placeholder(name, &serde_json::json!([""]))
            };
            if let Some(next) = placeholder.and_then(|leaf| set(&shown, path, leaf)) {
                shown = next;
            }
        }
        candidates.push(shown);
    }
    candidates
}

/// Whether `url` names its host: an address literal or `localhost` (RFC
/// 6761: always loopback) names a machine, which the runtime reaches only
/// when the address is public (`policy::network`, a layer above this one).
/// No example changes the scheme of such a url: the request could be
/// refused whatever its scheme.
fn names_reachable_host(url: &str) -> bool {
    let Ok(url) = url::Url::parse(url) else {
        return false;
    };
    match url.host() {
        Some(url::Host::Domain(name)) => {
            let name = name.trim_end_matches('.');
            name != "localhost" && !name.ends_with(".localhost")
        }
        Some(url::Host::Ipv4(_) | url::Host::Ipv6(_)) | None => false,
    }
}

/// `example` in the rejected row's own terms, so copying it runs what the
/// row asked: the row's value of each field the example sets that the row
/// sent without an issue (a graphBuild of `src` is shown a graphBuild of
/// `src`, not of the whole workspace), a placeholder or else the example's
/// value of a field at fault, no field the row did not send that the request runs without, and
/// each other field the row sent without an issue (a graphQuery `hubs` keeps
/// its `path`), apart from its brief ([`crate::contracts::BRIEF_FIELDS`]) and page position
/// (its state fields), which describe the call rather than its form. A
/// field at fault only by a value its message names the meaning of (`sort`
/// with `by: "modified"`, which means `modifiedAt`) is the row's own with
/// that value, so the example still asks what the row did. A text field at
/// fault as a whole shows a visible placeholder ([`placeholder`]), so the
/// example never pairs core's values with the row's own. Each change is
/// kept only while the request validates, so the example stays one the tool
/// accepts. A row at fault as a whole (two forms mixed) names no field at
/// fault: its own values stand wherever the request still validates, so a
/// commit row mixing `keywords` and `path` is shown its own repository's
/// search, not core's example repository. A field whose faults all lie
/// inside it is the row's own with those keys removed (a clasify
/// `source.query` keeps the tool it names). A required value the row lacks
/// is a visible placeholder ([`placeholder`]), never core's example value.
fn in_row_terms(tool_name: &str, example: &Value, rejected: Option<&Rejected<'_>>) -> Value {
    let (
        Some(Rejected {
            row,
            faulted,
            faults,
            meant,
            ..
        }),
        Some(fields),
    ) = (rejected, example.as_object())
    else {
        return example.clone();
    };
    let accepted = |candidate: &serde_json::Map<String, Value>| {
        super::validate(tool_name, serde_json::json!({ "queries": [candidate] })).is_ok()
    };
    let mut shown = fields.clone();
    for (field, value) in fields {
        let candidates = match row.get(field) {
            Some(own) if !faulted.contains(field) => vec![own.clone()],
            Some(own) => {
                let inside = faults
                    .iter()
                    .filter(|fault| fault.first() == Some(field))
                    .map(Vec::as_slice)
                    .collect::<Vec<_>>();
                // A value at fault as a whole shows a placeholder before
                // core's example value, which beside the row's own values
                // could name something that does not exist (core's owner
                // with the row's repository). Where the field takes no
                // placeholder (an object), the value sent in the shape the
                // field takes stands in.
                let mut candidates = if inside.iter().any(|fault| fault.len() == 1) {
                    Vec::new()
                } else {
                    sampled(tool_name, own, &inside)
                };
                candidates.extend(without_faults(own, &inside));
                if candidates.is_empty() {
                    candidates.extend(placeholder(field, value));
                }
                if candidates.is_empty() {
                    candidates.extend(sampled(tool_name, own, &inside));
                }
                candidates
            }
            // Left out, then a placeholder where the request needs it.
            None => std::iter::once(Value::Null)
                .chain(placeholder(field, value))
                .collect(),
        };
        for value in candidates {
            let mut candidate = shown.clone();
            if value.is_null() {
                candidate.shift_remove(field);
            } else {
                candidate.insert(field.clone(), value);
            }
            if accepted(&candidate) {
                shown = candidate;
                break;
            }
        }
    }
    let state = crate::id::ToolId::from_name(tool_name).map(crate::contracts::state_fields);
    let is_of_the_call = |field: &String| {
        crate::contracts::BRIEF_FIELDS.contains(&field.as_str())
            || state.is_some_and(|state| state.contains(field))
    };
    for (field, value) in row.iter() {
        if is_of_the_call(field) || shown.contains_key(field) || faulted.contains(field) {
            continue;
        }
        let mut candidate = shown.clone();
        candidate.insert(field.clone(), value.clone());
        if accepted(&candidate) {
            shown = candidate;
        }
    }
    // A field at fault that the example does not set stays in the example,
    // in a shape its schema accepts.
    for field in faulted {
        if shown.contains_key(field) || is_of_the_call(field) {
            continue;
        }
        let Some(own) = row.get(field) else {
            continue;
        };
        let inside = faults
            .iter()
            .filter(|fault| fault.first() == Some(field))
            .map(Vec::as_slice)
            .collect::<Vec<_>>();
        let mut candidates = sampled(tool_name, own, &inside);
        if !inside.iter().any(|fault| fault.len() == 1) {
            candidates.extend(without_faults(own, &inside));
        }
        for value in candidates {
            let mut candidate = shown.clone();
            candidate.insert(field.clone(), value);
            if accepted(&candidate) {
                shown = candidate;
                break;
            }
        }
    }
    // A field the row lacks that fields it sent need (`view` for `unique`):
    // the first literal its schema names that lets one of them stand, with
    // every sent field that then stands.
    let pending = row
        .iter()
        .filter(|(field, _)| {
            !is_of_the_call(field) && !shown.contains_key(*field) && !faulted.contains(field)
        })
        .collect::<Vec<_>>();
    for field in faulted {
        if pending.is_empty()
            || shown.contains_key(field)
            || row.contains_key(field)
            || is_of_the_call(field)
        {
            continue;
        }
        for literal in super::sample::literals(tool_name, std::slice::from_ref(field)) {
            let mut candidate = shown.clone();
            candidate.insert(field.clone(), literal);
            let alone = candidate.len();
            for (name, value) in &pending {
                let mut with = candidate.clone();
                with.insert((*name).clone(), (*value).clone());
                if accepted(&with) {
                    candidate = with;
                }
            }
            if candidate.len() > alone {
                shown = candidate;
                break;
            }
        }
    }
    // A field at fault by values whose meaning the messages name: the
    // row's own, each such value replaced by the literal it means.
    let mut repaired = Value::Object((*row).clone());
    for (path, literal) in meant {
        repaired = with_literal(repaired.clone(), path, literal).unwrap_or(repaired);
    }
    for field in faulted {
        let Some(value) = repaired.get(field) else {
            continue;
        };
        if !meant.iter().any(|(path, _)| path.first() == Some(field)) {
            continue;
        }
        let mut candidate = shown.clone();
        candidate.insert(field.clone(), value.clone());
        if accepted(&candidate) {
            shown = candidate;
        }
    }
    Value::Object(shown)
}

/// The form of `variants` that takes the most fields of `row`, among those
/// of the `operation` it names when any form has it; on a tie, the one whose
/// example shares the most of the row's values (a Maven row is shown the
/// Maven example, not the first form's npm one); then the first.
fn nearest_form<'a>(
    variants: &'a [Value],
    row: Option<&serde_json::Map<String, Value>>,
) -> Option<&'a Value> {
    let names = |variant: &Value, list: &str, name: &str| {
        variant[list]
            .as_array()
            .is_some_and(|names| names.iter().any(|known| known == name))
    };
    let operation = row
        .and_then(|row| row.get("operation"))
        .and_then(Value::as_str);
    let of_operation = variants
        .iter()
        .filter(|variant| {
            operation.is_some_and(|operation| names(variant, "operations", operation))
        })
        .collect::<Vec<_>>();
    let forms = if of_operation.is_empty() {
        variants.iter().collect()
    } else {
        of_operation
    };
    let taken = |variant: &Value| {
        row.map_or((0, 0), |row| {
            let fields = row
                .keys()
                .filter(|field| names(variant, "fields", field))
                .count();
            let shared = row
                .iter()
                .filter(|(field, value)| variant["example"].get(field.as_str()) == Some(value))
                .count();
            (fields, shared)
        })
    };
    let mut nearest: Option<(&Value, (usize, usize))> = None;
    for form in forms {
        let score = taken(form);
        if nearest.is_none_or(|(_, most)| score > most) {
            nearest = Some((form, score));
        }
    }
    nearest.map(|(form, _)| form)
}

/// A rejected row as it was sent, and the row fields its issues name.
struct Rejected<'a> {
    row: &'a serde_json::Map<String, Value>,
    /// The row's index in `queries`, as its issues' paths state it.
    index: String,
    /// The fields the row's issues name; none for an issue of the row as a
    /// whole, whose fields stand only where the request validates.
    faulted: Vec<String>,
    /// Each issue's path under the row (`source`, `query`, `bogusField`).
    faults: Vec<Vec<String>>,
    /// Each value outside its enum whose message names the one literal it
    /// means (`use "modifiedAt"`): its path under the row, and that literal.
    meant: Vec<(Vec<String>, Value)>,
    /// The row fields an issue names that the row lacks.
    missing: Vec<String>,
    /// Each text, number or boolean sent where another type is declared,
    /// whose message states the form to send (`…; send [\"x\"], not \"x\"`):
    /// its path under the row, and the value.
    mistyped: Vec<(Vec<String>, Value)>,
    /// The selector values (`{"operation":"search"}`) that admit every field
    /// the row's issues refuse, when each issue is such a field and one
    /// switch admits them all (`siblingSwitch`, set by union validation).
    switch: Option<&'a serde_json::Map<String, Value>>,
}

impl Rejected<'_> {
    /// Whether `example` (a row) holds each mistyped value where it was
    /// sent, as it is or inside the value shown there (`"x"` in `["x"]`,
    /// `5` as `"5"`).
    fn holds_mistyped(&self, example: &Value) -> bool {
        fn text(value: &Value) -> String {
            match value {
                Value::String(text) => text.clone(),
                other => other.to_string(),
            }
        }
        fn holds(shown: &Value, sent: &str) -> bool {
            match shown {
                Value::Array(items) => items.iter().any(|item| holds(item, sent)),
                Value::Object(fields) => fields.values().any(|value| holds(value, sent)),
                Value::Null => false,
                scalar => text(scalar) == sent,
            }
        }
        self.mistyped.iter().all(|(path, sent)| {
            example
                .pointer(&crate::json_pointer::of(path))
                .is_some_and(|shown| holds(shown, &text(sent)))
        })
    }
}

/// The one literal an enum issue's message says its value means
/// ([`super::literals::enum_message`]: `…; use "<literal>"; allowed: …`);
/// `None` for any other issue, or a choice of several.
fn meant_literal(issue: &ValidationIssue) -> Option<Value> {
    if issue.rule_id != "schema.enum" {
        return None;
    }
    let (_, after) = issue.message.split_once("; use \"")?;
    let (literal, _) = after.split_once("\"; allowed: ")?;
    (!literal.contains('"')).then(|| Value::String(literal.to_owned()))
}

/// `value` with `literal` at `path` under it (object keys, array indexes);
/// `None` when the path is not there.
fn with_literal(mut value: Value, path: &[String], literal: &Value) -> Option<Value> {
    let mut at = &mut value;
    for segment in path {
        at = match at {
            Value::Object(fields) => fields.get_mut(segment)?,
            Value::Array(items) => items.get_mut(segment.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    *at = literal.clone();
    Some(value)
}

/// The first rejected row of `sent`, read at the row index its first issue's
/// path states (`queries.<n>.…`), with the fields that row's issues name. An
/// input guard's path names a value inside its field (`keywords[]`,
/// `outer.key`): the field is the one at fault.
fn sent_row<'a>(error: &'a ContractValidationError, sent: &'a Value) -> Option<Rejected<'a>> {
    let path = &error.issues.first()?.path;
    let [queries, index, ..] = path.as_slice() else {
        return None;
    };
    if queries != "queries" {
        return None;
    }
    let row = sent["queries"]
        .get(index.parse::<usize>().ok()?)?
        .as_object()?;
    let mut faulted = Vec::new();
    let mut faults = Vec::new();
    let mut meant = Vec::new();
    let mut missing = Vec::new();
    let mut mistyped = Vec::new();
    let mut switches = Vec::new();
    for issue in &error.issues {
        if let [queries, at, field, ..] = issue.path.as_slice()
            && queries == "queries"
            && at == index
        {
            switches.push(
                issue
                    .schema
                    .as_ref()
                    .and_then(|schema| schema["siblingSwitch"].as_object()),
            );
            if let Some(literal) = meant_literal(issue) {
                meant.push((issue.path[2..].to_vec(), literal));
            }
            if issue.path.len() == 3 && !row.contains_key(field) && !missing.contains(field) {
                missing.push(field.clone());
            }
            if issue.rule_id == "schema.type"
                && issue.message.contains("send ")
                && let Some(sent) = issue.received.as_ref()
                && (sent.is_string() || sent.is_number() || sent.is_boolean())
            {
                mistyped.push((issue.path[2..].to_vec(), sent.clone()));
            }
            let field = field.split(['.', '[']).next().unwrap_or(field);
            faulted.push(field.to_owned());
            let mut under = vec![field.to_owned()];
            under.extend(issue.path[3..].iter().cloned());
            faults.push(under);
        }
    }
    let switch = switches
        .first()
        .copied()
        .flatten()
        .filter(|first| switches.iter().all(|switch| *switch == Some(*first)));
    Some(Rejected {
        row,
        index: index.clone(),
        faulted,
        faults,
        meant,
        missing,
        mistyped,
        switch,
    })
}

/// What an unknown field's message reads of the row holding it.
struct Row<'a> {
    /// The row lacks its form selector (`operation`): the fields of every
    /// form are its allowed ones ([`unselected_row`]).
    unselected: bool,
    /// The `operation` the row sent, when the request was sent.
    operation: Option<&'a str>,
    /// The request as sent (`null` when unknown).
    request: &'a Value,
}

/// The `operation` the row holding `issue` sent (`queries.<n>.…`).
fn row_operation<'a>(issue: &ValidationIssue, sent: &'a Value) -> Option<&'a str> {
    let [queries, index, ..] = issue.path.as_slice() else {
        return None;
    };
    (queries == "queries")
        .then(|| sent["queries"].get(index.parse::<usize>().ok()?)?["operation"].as_str())
        .flatten()
}

/// Whether some form of `tool_name` that runs `operation` takes `field`:
/// every form, when the row names no operation or no form lists it.
fn operation_takes(tool_name: &str, operation: Option<&str>, field: &str) -> bool {
    let Some(operation) = operation else {
        return true;
    };
    let variants = crate::contracts::cli_presentation()
        .ok()
        .and_then(|presentation| {
            presentation["tools"]
                .as_array()?
                .iter()
                .find(|entry| entry["name"] == tool_name)?["views"]["variants"]["variants"]
                .as_array()
        })
        .map_or(&[][..], Vec::as_slice);
    let names = |variant: &Value, list: &str, name: &str| {
        variant[list]
            .as_array()
            .is_some_and(|names| names.iter().any(|known| known == name))
    };
    let running = variants
        .iter()
        .filter(|variant| names(variant, "operations", operation))
        .collect::<Vec<_>>();
    running.is_empty()
        || running
            .iter()
            .any(|variant| names(variant, "fields", field))
}

/// The position of the issue that should lead when the first says only that
/// a form selector (`operation`) is missing: the same row's first wrong value.
/// That value is wrong under any form, and the fields a missing selector
/// leaves unknown are guesses from a form that was scored, not chosen.
fn sent_value_lead(issues: &[&ValidationIssue]) -> Option<usize> {
    let first = issues.first()?;
    let row = first.path.get(..first.path.len().checked_sub(1)?)?;
    let selector_missing = first.rule_id == "schema.required"
        && first
            .schema
            .as_ref()
            .is_some_and(|schema| schema.get("const").is_some() || schema.get("enum").is_some());
    selector_missing
        .then(|| {
            issues.iter().position(|issue| {
                issue.path.starts_with(row)
                    && !matches!(
                        issue.rule_id.as_str(),
                        "schema.required" | "schema.unknown-field"
                    )
            })
        })
        .flatten()
}

/// Whether the row holding `issue` lacks its form selector (`operation`):
/// the form its issues come from was then scored, not chosen, so its fields
/// are not the row's allowed fields.
fn unselected_row(error: &ContractValidationError, issue: &ValidationIssue) -> bool {
    let row = &issue.path[..issue.path.len().saturating_sub(1)];
    error.issues.iter().any(|other| {
        other.rule_id == "schema.required"
            && other.path.len() == row.len() + 1
            && other.path.starts_with(row)
            && other
                .schema
                .as_ref()
                .is_some_and(|schema| schema.get("const").is_some() || schema.get("enum").is_some())
    })
}

/// Words other tools and command lines use for a declared field, which no
/// spelling distance finds, each with the declared fields it may mean, most
/// likely first. Data only: a pair applies to a tool whose row declares the
/// target ([`unknown_field_message`]), and nothing is read by it.
pub(crate) const FOREIGN_WORDS: &[(&str, &[&str])] = &[
    ("pattern", &["matchString", "keywords", "nameRegex"]),
    ("query", &["matchString", "keywords"]),
    ("regex", &["matchString", "nameRegex"]),
    ("keywords", &["matchString"]),
    ("matchString", &["keywords"]),
    ("branch", &["ref"]),
    ("tag", &["ref"]),
    ("sha", &["ref"]),
    ("commit", &["ref"]),
    ("commitSha", &["ref"]),
    ("glob", &["include"]),
    ("globs", &["include"]),
    ("lang", &["fileType", "parser"]),
    ("language", &["fileType", "parser"]),
    ("file", &["path"]),
    ("filename", &["path"]),
    ("dir", &["path"]),
    ("directory", &["path"]),
    ("folder", &["path"]),
    ("lines", &["ranges"]),
    ("pr", &["number"]),
    ("issue", &["number"]),
    ("since", &["modifiedAt", "filters"]),
    ("until", &["modifiedAt", "filters"]),
    ("maxResults", &["limit"]),
    ("code", &["content"]),
    ("snippet", &["content"]),
    ("context", &["background", "contextLines"]),
    ("baseBranch", &["baseRef"]),
    ("headBranch", &["headRef"]),
    ("repository", &["repo"]),
];

/// The declared fields `sent` may mean as another tool's word
/// ([`FOREIGN_WORDS`]), in any case or separator style.
fn foreign_word(sent: &str) -> &'static [&'static str] {
    FOREIGN_WORDS
        .iter()
        .find(|(word, _)| super::repair::same_name(word, sent))
        .map_or(&[], |(_, meant)| meant)
}

/// The one line an unknown field is refused with: where it goes (the form
/// that declares it, beside `queries`, inside the rows, inside the row field
/// that holds it), else the declared field to use ([`FOREIGN_WORDS`], the
/// nearest spelling, a name sharing its words), else the allowed fields. In
/// a row without its selector, the fields of every form are the allowed
/// ones.
fn unknown_field_message(
    tool_name: &str,
    issue: &ValidationIssue,
    Row {
        unselected,
        operation,
        request,
    }: Row<'_>,
) -> String {
    // A field of the request itself (`queries` beside `cursor`) is said of
    // the request, never of a row.
    let query = match issue.path.as_slice() {
        [_] => "the request".to_owned(),
        path => query_label(path),
    };
    let field = issue.path.last().map(String::as_str).unwrap_or("unknown");
    if let Some(alone) = misplaced_request_field(tool_name, issue) {
        return if alone {
            format!(
                "'{field}' is a whole request, not a query field: send {{\"{field}\":\"…\"}} alone, as the reply's next gives it, without queries."
            )
        } else {
            format!("'{field}' is a request field: put it beside queries, not in {query}.")
        };
    }
    // Beside the field a request holds alone (`cursor`), a request field (an
    // empty `queries` too) is no form's: a resumed call is that field and
    // nothing else.
    if let [_] = issue.path.as_slice()
        && let Some(alone) = whole_request_field(tool_name).filter(|alone| alone != field)
        && (request.get(&alone).is_some()
            || issue
                .schema
                .as_ref()
                .and_then(|schema| schema["sentFields"].as_array())
                .is_some_and(|sent| sent.iter().any(|sent| *sent == *alone)))
        && sibling_requirement(issue).is_none()
    {
        return format!(
            "'{field}' is not allowed beside {alone} in the request. {}",
            alone_rule(&alone, &[field])
        );
    }
    if let Some(requires) = sibling_requirement(issue) {
        // Beside the field a request holds alone (`cursor`): a resumed call
        // is that field and nothing else.
        if let [_] = issue.path.as_slice()
            && let Some(alone) = whole_request_field(tool_name).filter(|alone| alone != field)
        {
            return format!(
                "'{field}' is not allowed beside {alone}: send {{\"{alone}\":\"…\"}} alone, as the reply's next gives it; '{field}' requires {requires}, in a new request."
            );
        }
        return format!(
            "'{field}' is not allowed in this form of {query}; it requires {requires}.{}",
            other_form_hint(field)
        );
    }
    let row_fields = if issue.path.len() == 3 {
        authored_row_fields(tool_name)
    } else {
        Vec::new()
    };
    let known = if unselected && !row_fields.is_empty() {
        row_fields.iter().map(String::as_str).collect::<Vec<_>>()
    } else {
        issue
            .schema
            .as_ref()
            .and_then(|schema| schema["knownFields"].as_array())
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
    };
    // A field the object already carries is never the one meant instead.
    let sent = issue
        .schema
        .as_ref()
        .and_then(|schema| schema["sentFields"].as_array())
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>();
    let candidates = known
        .iter()
        .copied()
        .chain(row_fields.iter().map(String::as_str))
        .filter(|name| !sent.contains(name))
        .collect::<Vec<_>>();
    // A field of the query a `{tool, query}` value holds, sent beside its
    // tool: it goes inside that query.
    if let Some(message) = tool_query_field(issue, request, &known, field, &query) {
        return message;
    }
    // A row field a caller authors, sent beside `queries`: it goes in the
    // rows.
    if let [_] = issue.path.as_slice()
        && authored_row_fields(tool_name)
            .iter()
            .any(|name| name == field)
    {
        return format!(
            "'{field}' goes inside each query: {{\"queries\":[{{…,\"{field}\":\"…\"}}]}}."
        );
    }
    // Another tool's word for a field this row declares.
    if let Some(meant) = foreign_word(field)
        .iter()
        .find(|meant| candidates.contains(meant) && operation_takes(tool_name, operation, meant))
    {
        return format!("Unknown field '{field}' in {query}; use '{meant}'.");
    }
    // The nearest spelling, else a declared field the sent name is part of
    // or holds (`context` of `contextLines`).
    let mut near = super::repair::nearest(field, candidates.iter().copied());
    if near.is_empty() {
        near = super::repair::sharing_words(
            field,
            candidates
                .iter()
                .copied()
                .filter(|name| operation_takes(tool_name, operation, name)),
        );
    }
    match near.as_slice() {
        [] => {}
        [meant] => return format!("Unknown field '{field}' in {query}; use '{meant}'."),
        [rest @ .., last] => {
            let rest = rest
                .iter()
                .map(|name| format!("'{name}'"))
                .collect::<Vec<_>>()
                .join(", ");
            return format!("Unknown field '{field}' in {query}; use {rest} or '{last}'.");
        }
    }
    match owning_row_fields(tool_name, &row_fields, field).as_slice() {
        [] => {}
        [owner] => {
            let form = owner_form(tool_name, owner, field);
            return format!(
                "'{field}' is a field of {owner}, not of {query}: send {owner}:{form}."
            );
        }
        [rest @ .., last] => {
            let form = owner_form(tool_name, last, field);
            return format!(
                "'{field}' is a field of {} or {last}, not of {query}: send it inside the one meant, as {last}:{form}.",
                rest.join(", ")
            );
        }
    }
    if let [_, _, _] = issue.path.as_slice() {
        let owners = partner_tools_declaring(tool_name, field);
        if !owners.is_empty() {
            return format!(
                "Unknown field '{field}' in {query}: it is a {} field, not a {tool_name} one. Allowed fields: {}.",
                owners.join(" or "),
                known.join(", ")
            );
        }
    }
    if let Some(forms) = forms_clause(
        tool_name,
        issue,
        &issue.path[..issue.path.len().saturating_sub(1)],
    ) {
        return format!("Unknown field '{field}' in {query}; {forms}.");
    }
    format!(
        "Unknown field '{field}' in {query}. Allowed fields: {}.",
        known.join(", ")
    )
}

/// The forms a value at `at` chose none of (`[["tool","query"],["url"]]`,
/// set by the union check), named in one clause with an example value of the
/// field from core's examples: `it takes tool and query, or url, e.g. {…}`.
fn forms_clause(tool_name: &str, issue: &ValidationIssue, at: &[String]) -> Option<String> {
    let forms = issue.schema.as_ref()?.get("forms")?.as_array()?;
    let named = forms
        .iter()
        .map(|form| {
            form.as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(" and ")
        })
        .collect::<Vec<_>>()
        .join(", or ");
    let examples = core_examples(tool_name);
    let example = field_example(&examples, at)
        .or_else(|| form_example(&examples, forms))
        .map(|example| format!(", e.g. {example}"))
        .unwrap_or_default();
    Some(format!("it takes {named}{example}"))
}

/// Core's request examples of `tool_name`, one per form of its rows.
fn core_examples(tool_name: &str) -> Vec<Value> {
    crate::contracts::cli_presentation()
        .ok()
        .and_then(|presentation| {
            let entry = presentation["tools"]
                .as_array()?
                .iter()
                .find(|entry| entry["name"] == tool_name)?;
            let variants = entry["views"]["variants"]["variants"].as_array()?;
            Some(
                variants
                    .iter()
                    .filter_map(|variant| variant.get("example").cloned())
                    .collect(),
            )
        })
        .unwrap_or_default()
}

/// The value the first example holding row field `at[2]` gives at `at`
/// (`queries.N.<field>…`). Below the row field the path was valid up to
/// here, so a key the example lacks is a record's own key or a list index:
/// its first entry stands for it.
fn field_example(examples: &[Value], at: &[String]) -> Option<String> {
    let [queries, _, field, within @ ..] = at else {
        return None;
    };
    if queries != "queries" {
        return None;
    }
    examples.iter().find_map(|example| {
        let mut value = example.get(field)?;
        for key in within {
            value = value.get(key.as_str()).or_else(|| match value {
                Value::Object(entries) => entries.values().next(),
                Value::Array(items) => items.first(),
                _ => None,
            })?;
        }
        Some(value.to_string())
    })
}

/// An object in any example whose fields are exactly one of `forms`, for a
/// field no example sets (`assets.<key>.source` takes the forms `source`
/// shows).
fn form_example(examples: &[Value], forms: &[Value]) -> Option<String> {
    fn find<'a>(value: &'a Value, forms: &[Value]) -> Option<&'a Value> {
        match value {
            Value::Object(entries) => {
                let fits = forms.iter().filter_map(Value::as_array).any(|form| {
                    form.len() == entries.len()
                        && form
                            .iter()
                            .filter_map(Value::as_str)
                            .all(|field| entries.contains_key(field))
                });
                if fits {
                    return Some(value);
                }
                entries.values().find_map(|entry| find(entry, forms))
            }
            Value::Array(items) => items.iter().find_map(|item| find(item, forms)),
            _ => None,
        }
    }
    examples
        .iter()
        .find_map(|example| find(example, forms))
        .map(Value::to_string)
}

/// Every field a caller may author in `tool_name`'s rows, across its forms:
/// a misspelt field may be meant for another form than the one it matched.
fn authored_row_fields(tool_name: &str) -> Vec<String> {
    let Some(tool) = crate::id::ToolId::from_name(tool_name) else {
        return Vec::new();
    };
    let Ok(contract) = crate::contracts::tool_contract(tool) else {
        return Vec::new();
    };
    let query = &contract["querySchema"];
    let private = crate::contracts::state_fields(tool);
    crate::contracts::schema_facts::property_names(query, &query["$defs"])
        .into_iter()
        .filter(|name| !private.contains(*name))
        .map(str::to_owned)
        .collect()
}

/// The tools `tool_name`'s description names whose rows declare `field`: the
/// tools it works with (graphBuild builds the graph graphQuery reads), where
/// a field of the other one is sent to it.
fn partner_tools_declaring(tool_name: &str, field: &str) -> Vec<&'static str> {
    let Some(description) = crate::contracts::cli_presentation()
        .ok()
        .and_then(|presentation| {
            presentation["tools"]
                .as_array()?
                .iter()
                .find(|entry| entry["name"] == tool_name)
        })
        .and_then(|entry| entry["mcp"]["description"].as_str())
    else {
        return Vec::new();
    };
    let bytes = description.as_bytes();
    let named = |name: &str| {
        description.match_indices(name).any(|(at, _)| {
            !at.checked_sub(1)
                .and_then(|before| bytes.get(before))
                .is_some_and(u8::is_ascii_alphanumeric)
                && !bytes
                    .get(at + name.len())
                    .is_some_and(u8::is_ascii_alphanumeric)
        })
    };
    crate::id::ToolId::ALL
        .iter()
        .map(|tool| tool.as_str())
        .filter(|name| *name != tool_name && named(name))
        .filter(|name| authored_row_fields(name).iter().any(|known| known == field))
        .collect()
}

/// The row fields (of any form) whose object value declares `field`, such as
/// astSearch `rule` for `pattern`, or each of structureSearch's date ranges
/// for `from`: every one, so none is named as the only place it goes.
fn owning_row_fields<'a>(tool_name: &str, row_fields: &'a [String], field: &str) -> Vec<&'a str> {
    let Some(contract) = crate::id::ToolId::from_name(tool_name)
        .and_then(|tool| crate::contracts::tool_contract(tool).ok())
    else {
        return Vec::new();
    };
    let (query, defs) = (&contract["querySchema"], &contract["querySchema"]["$defs"]);
    let facts = &crate::contracts::schema_facts::property_schemas;
    row_fields
        .iter()
        .map(String::as_str)
        .filter(|name| {
            facts(query, defs, name).into_iter().any(|schema| {
                crate::contracts::schema_facts::property_names(schema, defs).contains(field)
            })
        })
        .collect()
}

/// The row field `owner`'s object as a misplaced `field`'s hint shows it:
/// What to send instead of a field whose form is the wrong one, when the
/// call most likely meant a neighbouring form: `symbolFuzzy` searches the
/// workspace by name, and a symbol of one known file is `symbolName` with
/// `path`. Empty for any other field.
fn other_form_hint(field: &str) -> &'static str {
    match field {
        "symbolFuzzy" => {
            " To anchor a symbol of one file, send symbolName and path (and line) instead."
        }
        _ => "",
    }
}

/// `field` with every member the object requires whatever its form, in
/// declared order (`{"by":…,"order":…}` for a sort that requires `by`), so
/// the form the hint names is one the tool accepts.
fn owner_form(tool_name: &str, owner: &str, field: &str) -> String {
    use crate::contracts::schema_facts::{always_required, property_order, property_schemas};
    let mut members: Vec<String> = Vec::new();
    if let Some(contract) = crate::id::ToolId::from_name(tool_name)
        .and_then(|tool| crate::contracts::tool_contract(tool).ok())
    {
        let (query, defs) = (&contract["querySchema"], &contract["querySchema"]["$defs"]);
        let schemas = property_schemas(query, defs, owner);
        if let Some((first, rest)) = schemas.split_first() {
            let others: Vec<Vec<&str>> = rest
                .iter()
                .map(|schema| always_required(schema, defs))
                .collect();
            let required: Vec<&str> = always_required(first, defs)
                .into_iter()
                .filter(|name| others.iter().all(|other| other.contains(name)))
                .collect();
            members = property_order(first, defs)
                .into_iter()
                .filter(|name| *name == field || required.contains(name))
                .map(str::to_owned)
                .collect();
        }
    }
    if !members.iter().any(|name| name == field) {
        members.push(field.to_owned());
    }
    let shown: Vec<String> = members
        .into_iter()
        .map(|name| format!("{}:…", Value::String(name)))
        .collect();
    format!("{{{}}}", shown.join(","))
}

/// For an unknown `field` beside the tool of a `{tool, query}` value (a
/// clasify `source`), which the named tool's query declares: where it goes.
/// `known` are the fields the value's object declares, `query` its label.
fn tool_query_field(
    issue: &ValidationIssue,
    request: &Value,
    known: &[&str],
    field: &str,
    query: &str,
) -> Option<String> {
    let (tool_field, query_field) = super::tool_query_pair(known)?;
    let [queries, _, within @ .., _] = issue.path.as_slice() else {
        return None;
    };
    if queries != "queries" || within.is_empty() {
        return None;
    }
    let holder = request
        .pointer(&crate::json_pointer::of(
            &issue.path[..issue.path.len() - 1],
        ))?
        .as_object()?;
    let tool = holder.get(tool_field)?.as_str()?;
    let declares = crate::id::ToolId::from_name(tool)
        .is_some_and(|id| crate::contracts::query_declares(id, field));
    let at = within.join(".");
    declares.then(|| {
        format!(
            "'{field}' is a field of the {tool} query, not of {query}: send {at}.{query_field}.{field}, as {at}:{{\"{tool_field}\":\"{tool}\",\"{query_field}\":{{\"{field}\":…}}}}."
        )
    })
}

/// A request that mixes the form a field holds alone (`cursor`) with
/// another form's fields: a resumed call is that field and nothing else.
fn mixed_with_whole_request(tool_name: &str, issue: &ValidationIssue) -> Option<String> {
    if issue.rule_id != "schema.union-mixed" || !issue.path.is_empty() {
        return None;
    }
    let alone = whole_request_field(tool_name)?;
    let forms = issue.schema.as_ref()?["forms"].as_array()?;
    let fields = |form: &Value| {
        form.as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let others = forms
        .iter()
        .map(fields)
        .find(|form| *form != [alone.as_str()])?;
    forms
        .iter()
        .any(|form| fields(form) == [alone.as_str()])
        .then(|| {
            let others = others.iter().map(String::as_str).collect::<Vec<_>>();
            alone_rule(&alone, &others)
        })
}

/// The rule of the field a request holds alone (`cursor`), against the
/// fields `others` of a new request.
fn alone_rule(alone: &str, others: &[&str]) -> String {
    let others = others
        .iter()
        .map(|field| format!("'{field}'"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "A {alone} call takes the {alone} alone: send {{\"{alone}\":\"…\"}} as the reply's next gives it, without {others}; a new request sends {others} without {alone}."
    )
}

/// The field `tool_name`'s request envelope holds alone in one of its forms
/// (`cursor`), when exactly one form is a single field.
fn whole_request_field(tool_name: &str) -> Option<String> {
    let tool = crate::id::ToolId::from_name(tool_name)?;
    let input = &crate::contracts::tool_contract(tool).ok()?["inputSchema"];
    let mut alone = input["anyOf"].as_array()?.iter().filter_map(|form| {
        let form = crate::contracts::resolve_ref(form, &input["$defs"]);
        let properties = form["properties"].as_object()?;
        let mut names = properties.keys();
        match (names.next(), names.next()) {
            (Some(name), None) => Some(name.clone()),
            _ => None,
        }
    });
    let field = alone.next()?;
    alone.next().is_none().then_some(field)
}

/// For an unknown field directly in a query row (`queries.N.field`) that
/// `tool_name`'s request envelope declares: whether its envelope form holds
/// that field alone (`{cursor}`), or beside `queries` (`responseChars`).
fn misplaced_request_field(tool_name: &str, issue: &ValidationIssue) -> Option<bool> {
    let [queries, _, field] = issue.path.as_slice() else {
        return None;
    };
    if issue.rule_id != "schema.unknown-field" || queries != "queries" {
        return None;
    }
    let tool = crate::id::ToolId::from_name(tool_name)?;
    let input = &crate::contracts::tool_contract(tool).ok()?["inputSchema"];
    input["anyOf"].as_array()?.iter().find_map(|form| {
        let form = crate::contracts::resolve_ref(form, &input["$defs"]);
        let properties = form["properties"].as_object()?;
        properties
            .contains_key(field.as_str())
            .then(|| properties.len() == 1)
    })
}

/// The error envelope every surface returns for a call that never reached a
/// tool: the error and, when there are more, its other repair details. The
/// caller knows the tool; `error` (not `results`) marks the envelope.
pub fn tool_error(error: &str, details: Vec<String>) -> Value {
    let mut value = serde_json::json!({ "error": error });
    if !details.is_empty() {
        value["details"] = serde_json::json!(details);
    }
    value
}

/// Repair details as one error: the first names the fix, the rest follow,
/// then the call's `reference` ([`call_reference`]).
fn issues_error(mut details: Vec<String>, reference: Vec<String>) -> Value {
    let first = if details.is_empty() {
        "Invalid input.".to_owned()
    } else {
        details.remove(0)
    };
    details.extend(reference);
    tool_error(&first, details)
}

/// Core-authored guidance for a required field the request gets wrong: the
/// field's contract `x-requiredMessage`, rendered after `Missing required
/// field: <name>` as MCP renders the core message, and after a value outside
/// the field's range (`Number is outside the allowed range`), where the
/// writer needs the same help finding the value.
fn required_guidance(issue: &ValidationIssue) -> Option<&str> {
    if !matches!(issue.rule_id.as_str(), "schema.required" | "schema.range") {
        return None;
    }
    issue.schema.as_ref()?.get("x-requiredMessage")?.as_str()
}
