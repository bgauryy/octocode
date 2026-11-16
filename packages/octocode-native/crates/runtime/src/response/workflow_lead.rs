//! Output guidance follows the normalized row outcome, before response paging.
//! Nonempty success keeps evidence, warnings and required `next` calls, but
//! drops advisory hints and related requests whose facts are already stated.
//! Empty and failed rows keep actionable recovery. A related request carrying
//! otherwise unstated identity, explicitly required by a warning, or reading
//! more of an asked item remains part of the answer until its owning data
//! states that fact directly.
use crate::id::ToolId;
use serde_json::{Map, Value};
use std::collections::HashMap;

/// Applies the guidance policy to each normalized result row. The shared
/// dispatch boundary states empty/error outcomes in `status`; successful
/// partial rows still carry their coverage in typed fields and warnings.
/// No tool-specific result shape or response-size target decides this rule.
pub fn retain_guidance(tool: ToolId, structured: &mut Value, request: &Value) {
    let asked = request["queries"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|row| crate::response::result::fresh_query_row(tool, row))
        .collect::<Vec<_>>();
    let Some(rows) = structured.get_mut("results").and_then(Value::as_array_mut) else {
        return;
    };
    for row in rows {
        if row.get("status").is_some() || row["data"].get("errorCode").is_some() {
            continue;
        }
        let Some(data) = row.get_mut("data").and_then(Value::as_object_mut) else {
            continue;
        };
        data.remove("hints");
        let Some(Value::Array(related)) = data.remove("related") else {
            continue;
        };
        let kept = related
            .into_iter()
            .filter(|call| answers(tool, data, request, call) || reads_more(tool, &asked, call))
            .collect::<Vec<_>>();
        if !kept.is_empty() {
            data.insert("related".to_owned(), Value::Array(kept));
        }
    }
}

/// The only number-valued request fields that name an item. Every other
/// number a schema declares selects a part (a line, a limit, a page) and is
/// never an identity; a guard test walks every tool's schema.
const IDENTITY_NUMBERS: [&str; 1] = ["number"];

/// Whether `call` is part of the answer of the row `data`: someone already
/// promised it (a note names its tool or the row's `related`, or the
/// producing tool's description promises the link), it carries an identity
/// neither the row nor its `request` states, or one the row's notes name.
/// Anything else repeats facts the row states and is dropped.
fn answers(tool: ToolId, data: &Map<String, Value>, request: &Value, call: &Value) -> bool {
    note_names_tool(data, call)
        || note_names_related(data)
        || promised_link(tool, call)
        || carried_identities(call).iter().any(|identity| {
            let exact = |text: &str| text == identity.as_str();
            !(holds(request, &exact) || stated(data, &|_| true, &exact))
                || stated(data, &|key| NOTES.contains(&key), &|text| {
                    mentions(text, identity)
                })
        })
}

/// Whether `text` mentions `identity` as a whole: a digit-only one as a
/// whole number (`#874`, not `1874`), any other between identifier or path
/// boundaries (`o` is not in `no body`, `lib.rs` is in `src/lib.rs`).
fn mentions(text: &str, identity: &str) -> bool {
    if identity.is_empty() {
        return false;
    }
    if identity.bytes().all(|byte| byte.is_ascii_digit()) {
        return text
            .split(|c: char| !c.is_ascii_digit())
            .any(|number| number == identity);
    }
    let inside = |c: char| c.is_alphanumeric() || c == '_' || c == '-';
    // A dot joins when a name runs on through it (`lib.rs.backup`), not as
    // the full stop that ends a sentence.
    let joined_after = |rest: &str| {
        let mut chars = rest.chars();
        match chars.next() {
            Some('.') => chars.next().is_some_and(inside),
            Some(c) => inside(c),
            None => false,
        }
    };
    let joined_before = |head: &str| {
        let mut chars = head.chars().rev();
        match chars.next() {
            Some('.') => chars.next().is_some_and(inside),
            Some(c) => inside(c),
            None => false,
        }
    };
    text.match_indices(identity).any(|(at, found)| {
        let head = text.get(..at).unwrap_or_default();
        let rest = text.get(at + found.len()..).unwrap_or_default();
        !joined_before(head) && !joined_after(rest)
    })
}

/// Whether `call` is a cross-tool link `tool`'s description promises (the
/// tool states it: [`ToolId::promised_links`]).
fn promised_link(tool: ToolId, call: &Value) -> bool {
    call["tool"]
        .as_str()
        .is_some_and(|target| tool.promised_links().iter().any(|to| to.as_str() == target))
}

/// Whether `call` asks one of the `asked` items (this request's own rows,
/// as fresh requests) for more of it: a call to the same tool each of whose
/// rows is an asked row with its part selector (the list the contract
/// enumerates, such as sections) changed or added and every other field as
/// asked or left out, no identity of it dropped. That is the way to the rest of this
/// answer; a call that changes any other field (the item, the operation, a
/// view) asks another question.
fn reads_more(tool: ToolId, asked: &[Value], call: &Value) -> bool {
    call["tool"] == tool.as_str()
        && call["input"]["queries"]
            .as_array()
            .filter(|rows| !rows.is_empty())
            .is_some_and(|rows| {
                rows.iter().all(|row| {
                    let row = crate::response::result::fresh_query_row(tool, row);
                    asked.iter().any(|asked| selects_more_of(tool, asked, &row))
                })
            })
}

/// Whether `row` is `asked` with other parts selected: it changes or adds a
/// part selector, keeps every non-list field it holds as asked (its other
/// lists, such as the `include` that narrows a `patches` read, ride along),
/// and keeps each identity the asked row carries ([`carried_identities`]).
fn selects_more_of(tool: ToolId, asked: &Value, row: &Value) -> bool {
    let (Some(asked_fields), Some(fields)) = (asked.as_object(), row.as_object()) else {
        return false;
    };
    let identities = serde_json::json!({"tool": tool.as_str(), "input": {"queries": [asked]}});
    let selectors = enumerated(tool);
    let selects = |field: &String, value: &Value| {
        value.as_array().is_some_and(|items| {
            selectors.get(field).is_some_and(|parts| {
                items.iter().all(|item| {
                    item.as_str()
                        .is_some_and(|item| parts.iter().any(|p| p == item))
                })
            })
        })
    };
    fields
        .iter()
        .any(|(field, value)| selects(field, value) && asked_fields.get(field) != Some(value))
        && fields
            .iter()
            .all(|(field, value)| value.is_array() || asked_fields.get(field) == Some(value))
        && carried_identities(&identities)
            .into_iter()
            .all(|identity| holds(row, &|text: &str| text == identity))
}

/// A note can promise a call by tool rather than by an item identity: its
/// selectors may live in arrays, which are deliberately not identities.
/// Only standalone prose references count, not source paths or identifiers
/// that happen to contain the tool name. Diagnostic messages can quote
/// source; their explicit recovery, rather than that explanation, names
/// the action to retain.
fn note_names_tool(data: &Map<String, Value>, call: &Value) -> bool {
    let Some(tool) = call["tool"].as_str() else {
        return false;
    };
    let names = |text: &str| names_word(text, tool);
    stated(data, &|key| NOTES.contains(&key), &names)
        || data
            .get("diagnostics")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|diagnostic| diagnostic["recovery"].as_str())
            .any(names)
}

/// A warning or notice that sends the reader to `related` (or `related[]`)
/// promises that row's own calls, whatever facts they repeat. Brackets trim
/// away, so `related[]` is the same word.
fn note_names_related(data: &Map<String, Value>) -> bool {
    stated(data, &|key| NOTES.contains(&key), &|text| {
        names_word(text, "related")
    })
}

/// Whether `text` holds `name` as a standalone word.
fn names_word(text: &str, name: &str) -> bool {
    text.split_whitespace().any(|word| {
        word.trim_matches(|c| {
            matches!(
                c,
                '`' | '\''
                    | '"'
                    | '('
                    | ')'
                    | '['
                    | ']'
                    | '{'
                    | '}'
                    | ','
                    | '.'
                    | ':'
                    | ';'
                    | '!'
                    | '?'
            )
        }) == name
    })
}

/// Whether a text scalar satisfies `matches`: identities require equality,
/// while a prose note can mention an identity inside a sentence.
fn holds(value: &Value, matches: &impl Fn(&str) -> bool) -> bool {
    match value {
        Value::String(text) => matches(text),
        Value::Number(number) => matches(&number.to_string()),
        Value::Array(items) => items.iter().any(|item| holds(item, matches)),
        Value::Object(map) => map.values().any(|item| holds(item, matches)),
        _ => false,
    }
}

/// The row members that say something about the row rather than answer it.
const NOTES: [&str; 2] = ["warnings", "notices"];

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
/// a view), and each number in an [`IDENTITY_NUMBERS`] field, outside lists (literals, globs and sections select; they name no
/// item). Paths, refs, SHAs and advisory ids are what a dropped call could
/// otherwise take away.
pub fn carried_identities(call: &Value) -> Vec<String> {
    fn collect(value: &Value, field: &str, named: Option<&Enumerated>, out: &mut Vec<String>) {
        match value {
            Value::String(text)
                if !text.is_empty()
                    && !named
                        .and_then(|fields| fields.get(field))
                        .is_some_and(|values| values.contains(text)) =>
            {
                out.push(text.clone());
            }
            Value::Number(number) if IDENTITY_NUMBERS.contains(&field) => {
                out.push(number.to_string());
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

#[cfg(test)]
#[path = "workflow_lead_tests.rs"]
mod tests;
