//! Shared assertions for typed continuations and complete evidence page walks.
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "fixtures for tests, compiled only for them (`test`, `test-support`): a broken fixture fails its test at once"
)]
use super::{partial_reasons, publish_for_assertion, validate_output, validate_test_input};
use serde_json::Value;

/// No text of `response` says what a partial reason means: a row carries the
/// code, and what each code means is said once, by the output reference.
pub fn assert_no_reason_meaning(response: &Value) {
    fn texts<'a>(value: &'a Value, out: &mut Vec<&'a str>) {
        match value {
            Value::String(text) => out.push(text),
            Value::Array(items) => items.iter().for_each(|item| texts(item, out)),
            Value::Object(map) => map.values().for_each(|item| texts(item, out)),
            _ => {}
        }
    }
    let meanings = partial_reasons()
        .and_then(Value::as_object)
        .expect("the embedded contract declares every partial reason");
    let mut said = Vec::new();
    texts(response, &mut said);
    for (code, meaning) in meanings {
        let meaning = meaning.as_str().filter(|meaning| !meaning.is_empty());
        let explained = meaning.and_then(|meaning| said.iter().find(|text| text.contains(meaning)));
        assert!(
            explained.is_none(),
            "a response explains {code}: {explained:?}\n{response}"
        );
    }
}

pub fn assert_output_contract(tool: &str, output: &Value) {
    let published = publish_for_assertion(tool, output);
    validate_output(tool, &published)
        .unwrap_or_else(|error| panic!("{tool} output contract: {error:?}\n{published}"));
    assert_leads_valid(output);
    assert_one_completion_rule(tool, &published);
    assert_related_promise_has_calls(tool, output);
}

/// A success row keeps what its warnings and notices promise: a note that
/// sends the reader to `related` comes with calls there, and one that names
/// a `next.<channel>` call comes with that call. `output` is judged as
/// given: the response stage reads and removes the producers' marks
/// ([`crate::response::result::Continuation::required`]), so a served row
/// cannot pass the stage's rule a second time (FB6 reflection: a note that
/// names a field the row lacks costs the reader a guess).
pub fn assert_related_promise_has_calls(tool: &str, output: &Value) {
    let names_related = |text: &str| {
        text.split_whitespace()
            .any(|word| word.trim_matches(|c: char| !c.is_alphanumeric()) == "related")
    };
    {
        let stage = "as given";
        for row in output["results"].as_array().into_iter().flatten() {
            let data = &row["data"];
            if row.get("status").is_some() || data.get("errorCode").is_some() {
                continue;
            }
            let notes: Vec<&str> = ["warnings", "notices"]
                .iter()
                .flat_map(|note| data[note].as_array().into_iter().flatten())
                .filter_map(Value::as_str)
                .collect();
            assert!(
                !notes.iter().any(|text| names_related(text))
                    || data["related"]
                        .as_array()
                        .is_some_and(|calls| !calls.is_empty()),
                "{tool} ({stage}): a note promises `related` but the row carries none\n{row}"
            );
            for channel in notes.iter().flat_map(|text| next_channels(text)) {
                assert!(
                    data["next"].get(channel).is_some(),
                    "{tool} ({stage}): a note names next.{channel} but the row carries none\n{row}"
                );
            }
        }
    }
}

/// The `next.<channel>` calls `text` names, as `next.nextPage` or
/// `` `next.expandScan` ``.
fn next_channels(text: &str) -> impl Iterator<Item = &str> {
    text.match_indices("next.").filter_map(move |(at, _)| {
        let before = text[..at].chars().next_back();
        if before.is_some_and(|c| c.is_alphanumeric() || c == '.') {
            return None;
        }
        let rest = &text[at + "next.".len()..];
        let end = rest
            .find(|c: char| !c.is_ascii_alphanumeric())
            .unwrap_or(rest.len());
        (end > 0 && rest.starts_with(|c: char| c.is_ascii_lowercase())).then(|| &rest[..end])
    })
}

/// One completion rule on the wire: a served block states no `hasMore` and
/// no `countScope` (whether more follows is the reply's top-level `next` to
/// say, and a `total` is stated only when exact), and no reply carries the
/// former `responsePagination` block. Checked on the published form, on
/// every page a walk reads.
pub fn assert_one_completion_rule(tool: &str, published: &Value) {
    fn visit(tool: &str, value: &Value) {
        match value {
            Value::Object(map) => {
                for (key, child) in map {
                    assert!(
                        !matches!(key.as_str(), "hasMore" | "countScope"),
                        "{tool}: a served block states {key}: {value}"
                    );
                    // Calls are inputs to run unchanged, not evidence.
                    if !matches!(key.as_str(), "next" | "calls" | "related") {
                        visit(tool, child);
                    }
                }
            }
            Value::Array(items) => items.iter().for_each(|item| visit(tool, item)),
            _ => {}
        }
    }
    assert!(
        published.get("responsePagination").is_none(),
        "{tool}: the reply's continuation is its top-level next: {published}"
    );
    for row in published["results"].as_array().into_iter().flatten() {
        visit(tool, &row["data"]);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Lead {
    pub channel: String,
    pub name: String,
    pub tool: String,
    pub query: Value,
    pub kind: String,
}

/// Every call under `output` with the channel that holds it. A call the
/// runtime still shapes states its `kind`; a served one does not, and its
/// channel says it: `related` starts new work, any other continues.
pub fn leads(output: &Value) -> Vec<Lead> {
    fn visit(value: &Value, channel: &str, name: &str, out: &mut Vec<Lead>) {
        if let (Some(tool), Some(input)) = (
            value["tool"].as_str(),
            value.get("input").filter(|input| input.is_object()),
        ) {
            let kind = match value.get("kind") {
                Some(kind) => kind.as_str().unwrap_or_default(),
                None if channel == "related" => "related",
                None => "continue",
            };
            assert!(matches!(kind, "continue" | "related"), "{value}");
            out.push(Lead {
                channel: channel.into(),
                name: name.into(),
                tool: tool.into(),
                query: input.clone(),
                kind: kind.into(),
            });
            return;
        }
        match value {
            Value::Object(map) => {
                for (key, child) in map {
                    if key == "input" {
                        continue;
                    }
                    let next_channel = if matches!(key.as_str(), "next" | "related") {
                        key.as_str()
                    } else {
                        channel
                    };
                    visit(child, next_channel, key, out);
                }
            }
            Value::Array(items) => {
                for (index, item) in items.iter().enumerate() {
                    visit(item, channel, &index.to_string(), out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    visit(output, "", "", &mut out);
    out
}

pub fn assert_leads_valid(output: &Value) {
    for lead in leads(output) {
        if lead.channel == "next" {
            assert_eq!(lead.kind, "continue", "{lead:?}");
        }
        if lead.channel == "related" {
            assert_eq!(lead.kind, "related", "{lead:?}");
        }
        validate_test_input(&lead.tool, &lead.query)
            .unwrap_or_else(|error| panic!("Invalid typed request: {lead:?}: {error:?}"));
    }
}

/// `needle` appears nowhere in `value`: in no string leaf and no object key.
pub fn assert_absent(value: &Value, needle: &str) {
    fn find<'a>(value: &'a Value, needle: &str) -> Option<&'a str> {
        match value {
            Value::String(text) => text.contains(needle).then_some(text.as_str()),
            Value::Array(items) => items.iter().find_map(|item| find(item, needle)),
            Value::Object(map) => map.iter().find_map(|(key, child)| {
                if key.contains(needle) {
                    Some(key.as_str())
                } else {
                    find(child, needle)
                }
            }),
            _ => None,
        }
    }
    if let Some(hit) = find(value, needle) {
        panic!("`{needle}` present in `{hit}`:\n{value}");
    }
}

/// Every string under a `path` key of `value`, at any depth (leads included),
/// in document order.
pub fn paths_in(value: &Value) -> Vec<String> {
    fn visit(value: &Value, out: &mut Vec<String>) {
        match value {
            Value::Array(items) => items.iter().for_each(|item| visit(item, out)),
            Value::Object(map) => {
                for (key, child) in map {
                    if key == "path"
                        && let Some(path) = child.as_str()
                    {
                        out.push(path.to_owned());
                    }
                    visit(child, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    visit(value, &mut out);
    out
}

/// `value` with every object's keys sorted: two calls are the same call when
/// their canonical texts are equal.
pub fn canonical(value: &Value) -> String {
    fn sorted(value: &Value) -> Value {
        match value {
            Value::Array(items) => Value::Array(items.iter().map(sorted).collect()),
            Value::Object(map) => {
                let mut keys = map.keys().collect::<Vec<_>>();
                keys.sort();
                Value::Object(
                    keys.into_iter()
                        .map(|key| (key.clone(), sorted(&map[key])))
                        .collect(),
                )
            }
            other => other.clone(),
        }
    }
    sorted(value).to_string()
}

/// An lspSearch page's location rows, each with its path: rows grouped under
/// `files[].matches`, then any listed flat in `matches` (a row naming no path
/// stays flat). Empty when the page has neither.
pub fn location_rows(row: &Value) -> Vec<Value> {
    let grouped = row["files"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|file| {
            let path = file["path"].clone();
            file["matches"]
                .as_array()
                .expect("grouped file rows")
                .iter()
                .map(move |site| {
                    let mut site = site.clone();
                    site["path"] = path.clone();
                    site
                })
        });
    let flat = row["matches"].as_array().into_iter().flatten().cloned();
    grouped.chain(flat).collect()
}

#[path = "walker.rs"]
pub mod walker;
pub use walker::{Page, Walk, Walker};

/// Select an explicitly related request by target and its complete input.
/// The predicate sees the envelope, so batched requests retain every row.
pub fn related_call<'a>(
    data: &'a Value,
    tool: &str,
    accepts: impl Fn(&Value) -> bool,
) -> &'a Value {
    find_related_call(data, tool, accepts)
        .unwrap_or_else(|| panic!("No matching related {tool} request in {data}"))
}

pub fn find_related_call<'a>(
    data: &'a Value,
    tool: &str,
    accepts: impl Fn(&Value) -> bool,
) -> Option<&'a Value> {
    data["related"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|call| {
            call.get("kind").is_none_or(|kind| kind == "related")
                && call["tool"] == tool
                && accepts(&call["input"])
        })
}
