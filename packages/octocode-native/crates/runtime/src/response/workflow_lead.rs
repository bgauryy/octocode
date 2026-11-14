//! The rules for a row's `related[]`. A complete row answered its
//! question: it keeps every batch for the first available tool its own
//! tool's main workflow leads to ([`ToolId::workflow_leads`]), and drops the
//! other optional calls, each one schema-documented request away. No fact
//! lives only in a dropped call: a call that carries an identity the row
//! does not state (a merge base, a later batch of reads), or one a note of
//! the row names (the read a warning points to), is part of the answer and
//! stays ([`carried_identities`]). A row that did not answer whole (failed,
//! empty, out of range, partial, or with items its selection left unshown)
//! keeps every related call: there they are its recovery. `next` is never
//! touched: it is the continuation contract.
use crate::id::ToolId;
use serde_json::{Map, Value};
use std::collections::HashMap;

/// Applies the rule to each row of `structured`'s `results`. `request`
/// holds the queries the rows answer: what they name (an owner, a repo, a
/// path) the call states, so a lead repeating it carries no new fact.
pub fn keep_workflow_lead(tool: ToolId, structured: &mut Value, request: &Value) {
    let Some(rows) = structured.get_mut("results").and_then(Value::as_array_mut) else {
        return;
    };
    for row in rows {
        if !answered_whole(row) {
            continue;
        }
        let Some(data) = row.get_mut("data").and_then(Value::as_object_mut) else {
            continue;
        };
        let Some(Value::Array(related)) = data.remove("related") else {
            continue;
        };
        // Select the preferred available tool, then keep every ordered
        // batch for it: repeated identities do not mean repeated work.
        let lead = tool
            .workflow_leads()
            .iter()
            .find(|target| related.iter().any(|call| call["tool"] == target.as_str()));
        let kept = related
            .into_iter()
            .filter(|call| {
                lead.is_some_and(|target| call["tool"] == target.as_str())
                    || answers(data, request, call)
            })
            .collect::<Vec<_>>();
        if !kept.is_empty() {
            data.insert("related".to_owned(), Value::Array(kept));
        }
    }
}

/// Whether `call` is part of the answer of the row `data`: it carries an
/// identity neither the row nor its `request` states, or one the row's
/// notes name.
fn answers(data: &Map<String, Value>, request: &Value, call: &Value) -> bool {
    carried_identities(call).into_iter().any(|identity| {
        let exact = |text: &str| text == identity;
        !(holds(request, &exact) || stated(data, &|_| true, &exact))
            || stated(data, &|key| NOTES.contains(&key), &|text| {
                text.contains(identity)
            })
    })
}

/// Whether a text scalar satisfies `matches`: identities require equality,
/// while a prose note can mention an identity inside a sentence.
fn holds(value: &Value, matches: &impl Fn(&str) -> bool) -> bool {
    match value {
        Value::String(text) => matches(text),
        Value::Array(items) => items.iter().any(|item| holds(item, matches)),
        Value::Object(map) => map.values().any(|item| holds(item, matches)),
        _ => false,
    }
}

/// The row members that say something about the row rather than answer it.
const NOTES: [&str; 3] = ["warnings", "hints", "notices"];

/// Whether some text under `data` (outside its calls) satisfies `matches`,
/// searching only members `within` accepts at the top level.
fn stated(
    data: &Map<String, Value>,
    within: &impl Fn(&str) -> bool,
    matches: &impl Fn(&str) -> bool,
) -> bool {
    data.iter()
        .filter(|(key, _)| !matches!(key.as_str(), "related" | "next") && within(key))
        .any(|(_, value)| holds(value, matches))
}

/// The identities a call carries: each text value of its query rows that
/// is no value its tool's schema enumerates for that property (an operation,
/// a view), outside lists (literals, globs and sections select; they name no
/// item). Paths, refs, SHAs and advisory ids are what a dropped call could
/// otherwise take away.
pub fn carried_identities(call: &Value) -> Vec<&str> {
    fn collect<'a>(
        value: &'a Value,
        field: &str,
        named: Option<&Enumerated>,
        out: &mut Vec<&'a str>,
    ) {
        match value {
            Value::String(text)
                if !text.is_empty()
                    && !named
                        .and_then(|fields| fields.get(field))
                        .is_some_and(|values| values.contains(text)) =>
            {
                out.push(text);
            }
            Value::Object(map) => map
                .iter()
                .for_each(|(field, item)| collect(item, field, named, out)),
            _ => {}
        }
    }
    let named = call["tool"]
        .as_str()
        .and_then(ToolId::from_name)
        .map(enumerated);
    let mut out = Vec::new();
    for row in call["input"]["queries"].as_array().into_iter().flatten() {
        collect(row, "", named, &mut out);
    }
    out
}

type Enumerated = HashMap<String, Vec<String>>;

/// Every text value `tool`'s query schema enumerates (`enum`, `const`), by property,
/// through the shared definitions its `$ref`s name (validation reads the
/// same ones). A path spelled `content` is not the `view` selector.
fn enumerated(tool: ToolId) -> &'static Enumerated {
    static VALUES: [std::sync::OnceLock<Enumerated>; ToolId::ALL.len()] =
        [const { std::sync::OnceLock::new() }; ToolId::ALL.len()];
    VALUES[tool.index()].get_or_init(|| {
        fn walk<'a>(
            value: &'a Value,
            shared: &'a Value,
            field: &'a str,
            seen: &mut std::collections::HashSet<(&'a str, &'a str)>,
            out: &mut Enumerated,
        ) {
            match value {
                Value::Object(map) => {
                    for (key, item) in map {
                        match (key.as_str(), item) {
                            ("enum", Value::Array(values)) => out
                                .entry(field.to_owned())
                                .or_default()
                                .extend(values.iter().filter_map(Value::as_str).map(str::to_owned)),
                            ("const", Value::String(value)) => {
                                out.entry(field.to_owned()).or_default().push(value.clone());
                            }
                            ("properties", Value::Object(properties)) => {
                                for (field, schema) in properties {
                                    walk(schema, shared, field, seen, out);
                                }
                            }
                            ("$ref", Value::String(reference)) => {
                                if let Some(name) = reference.strip_prefix("#/$defs/")
                                    && seen.insert((name, field))
                                {
                                    walk(&shared[name], shared, field, seen, out);
                                }
                            }
                            _ => walk(item, shared, field, seen, out),
                        }
                    }
                }
                Value::Array(items) => {
                    for item in items {
                        walk(item, shared, field, seen, out);
                    }
                }
                _ => {}
            }
        }
        let mut out = HashMap::new();
        if let (Ok(contract), Ok(whole)) = (
            crate::contracts::tool_contract(tool),
            crate::contracts::parsed_contract(),
        ) {
            walk(
                &contract["querySchema"],
                &whole["$defs"],
                "",
                &mut std::collections::HashSet::new(),
                &mut out,
            );
        }
        out
    })
}

/// Whether `row` answered its question whole: no failure or empty status,
/// no `errorCode`, not partial, and no item its selection judged without
/// showing it (`unknownItems`, `relatedItems`) or left partial.
fn answered_whole(row: &Value) -> bool {
    if row.get("status").is_some() {
        return false;
    }
    let data = &row["data"];
    let selection = &data["selection"];
    let unshown = |field: &str| selection[field].as_u64().is_some_and(|count| count > 0);
    data.get("errorCode").is_none()
        && data["isPartial"] != Value::Bool(true)
        && (selection.is_null()
            || (selection["status"] == "complete"
                && !unshown("unknownItems")
                && !unshown("relatedItems")))
}

#[cfg(test)]
#[path = "workflow_lead_tests.rs"]
mod tests;
