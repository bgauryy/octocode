//! Output-contract, lead and page-walk checks, one copy for both kinds of
//! octocode-native test: the unit tests reach it as `tools::test_support::checks`,
//! the integration tests include this file by path as `support::checks`. The
//! including module brings `parsed_contract`, `validate` and `validate_output`
//! (crate `contracts`) into scope.
#![allow(clippy::panic, reason = "test assertions fail by panicking")]

use super::{parsed_contract, validate, validate_output};
use serde_json::Value;
use std::collections::BTreeSet;

/// Pages a walk follows before it fails as unbounded.
pub(crate) const WALK_CAP: usize = 200;

/// `output`, a public response (`{results:[...]}`; clasify `{queries:[...]}`),
/// satisfies `tool`'s output contract.
pub fn assert_output_contract(tool: &str, output: &Value) {
    validate_output(tool, output)
        .unwrap_or_else(|error| panic!("{tool} output contract: {error:?}\n{output}"));
    assert_lead_names_declared(output);
}

/// One `{tool, query}` call an output offers: `channel` is `next` or `hints`
/// (`responsePagination` for the response-paging call), `name` its key.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Lead {
    pub channel: String,
    pub name: String,
    pub tool: String,
    pub query: Value,
}

fn hint_call(text: &str) -> Option<(&str, Value)> {
    let (name, text) = text.split_once(": ")?;
    let (text, low) = text
        .strip_prefix("Approximate: ")
        .map_or((text, false), |text| (text, true));
    let (tool, input) = text.split_once(' ')?;
    if !input.starts_with('{') {
        return None;
    }
    let query: Value = serde_json::from_str(input).expect("complete suggested input");
    validate(tool, query.clone()).expect("suggested call validates unchanged");
    let mut call = serde_json::json!({"tool":tool,"query":query});
    if low {
        call["confidence"] = serde_json::json!("low");
    }
    Some((name, call))
}

/// Decode a named suggestion from the public string list without accepting
/// the internal object format. Every decoded call must validate unchanged.
#[cfg_attr(
    not(unix),
    allow(dead_code, reason = "its in-crate caller is unix-only; integration tests use it on every platform")
)]
pub fn hint(data: &Value, name: &str) -> Value {
    let Some(items) = data.get("hints") else {
        return Value::Null;
    };
    let items = items
        .as_array()
        .expect("public hints must be a string list");
    assert!(items.iter().all(Value::is_string), "{items:?}");
    if name == "text" {
        let text: Vec<_> = items
            .iter()
            .filter(|item| hint_call(item.as_str().expect("string")).is_none())
            .cloned()
            .collect();
        return if text.is_empty() {
            Value::Null
        } else {
            Value::Array(text)
        };
    }
    items
        .iter()
        .filter_map(Value::as_str)
        .filter_map(hint_call)
        .find_map(|(key, call)| (key == name).then_some(call))
        .unwrap_or(Value::Null)
}

/// Every lead of `output`: each `{tool, query}` under a `next` or `hints` key
/// at any depth, never one nested inside another lead's query.
pub fn leads(output: &Value) -> Vec<Lead> {
    fn call(value: &Value) -> Option<(String, Value)> {
        Some((
            value.get("tool")?.as_str()?.to_owned(),
            value.get("query")?.clone(),
        ))
    }
    fn visit(value: &Value, parent: &str, out: &mut Vec<Lead>) {
        match value {
            Value::Array(items) => items.iter().for_each(|item| visit(item, parent, out)),
            Value::Object(map) => {
                for (key, child) in map {
                    if key == "query" {
                        continue;
                    }
                    if key == "next" && parent == "responsePagination" {
                        if let Some((tool, query)) = call(child) {
                            out.push(Lead {
                                channel: "responsePagination".into(),
                                name: "next".into(),
                                tool,
                                query,
                            });
                        }
                    } else if key == "next" || key == "hints" {
                        if key == "hints"
                            && let Some(items) = child.as_array()
                        {
                            for item in items {
                                let text = item.as_str().expect("public hint is a string");
                                if let Some((name, lead)) = hint_call(text) {
                                    let (tool, query) = call(&lead).expect("suggested call");
                                    out.push(Lead {
                                        channel: key.clone(),
                                        name: name.into(),
                                        tool,
                                        query,
                                    });
                                }
                            }
                        }
                        for (name, lead) in child.as_object().into_iter().flatten() {
                            if let Some((tool, query)) = call(lead) {
                                out.push(Lead {
                                    channel: key.clone(),
                                    name: name.clone(),
                                    tool,
                                    query,
                                });
                            }
                        }
                    } else {
                        visit(child, key, out);
                    }
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    visit(output, "", &mut out);
    out
}

/// The names the contract declares for continuations
/// (`continuationChannels.kinds.pages` and `.leads`).
fn declared_lead_kinds() -> BTreeSet<String> {
    let kinds = &parsed_contract().expect("contract")["continuationChannels"]["kinds"];
    ["pages", "leads"]
        .into_iter()
        .flat_map(|group| kinds[group].as_array().expect("declared kinds"))
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

/// Every lead and page of `output` is named for a kind the contract declares,
/// or for the tool it calls (an entry named after its tool resumes that
/// tool's walk). A name is its kind plus an optional repeat number
/// (`readHits2`).
pub fn assert_lead_names_declared(output: &Value) {
    let declared = declared_lead_kinds();
    let undeclared = leads(output)
        .into_iter()
        .filter(|lead| lead.channel != "responsePagination")
        .map(|lead| (lead.name, lead.tool))
        .filter(|(name, tool)| {
            let kind = name.trim_end_matches(|c: char| c.is_ascii_digit());
            !declared.contains(kind) && kind != tool
        })
        .collect::<Vec<_>>();
    assert!(
        undeclared.is_empty(),
        "lead names the contract does not declare (name, tool): {undeclared:?}\n{output}"
    );
}

/// Every lead of `output` is a complete call (`{queries:[row, ...]}`) that the
/// strict input contract of the tool it names accepts unchanged, and is named
/// for a kind the contract declares.
pub fn assert_leads_valid(output: &Value) {
    assert_lead_names_declared(output);
    for lead in leads(output) {
        assert!(
            lead.query["queries"]
                .as_array()
                .is_some_and(|rows| !rows.is_empty()),
            "{}.{} is not a complete call: {lead:?}",
            lead.channel,
            lead.name
        );
        if let Err(error) = validate(&lead.tool, lead.query.clone()) {
            panic!(
                "{}.{} → {} fails its input contract: {error:?}\n{lead:?}",
                lead.channel, lead.name, lead.tool
            );
        }
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
pub(crate) fn paths_in(value: &Value) -> Vec<String> {
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

/// The contract's page channels (`continuationChannels.kinds.pages`).
pub(crate) fn page_channels() -> BTreeSet<String> {
    parsed_contract().expect("contract")["continuationChannels"]["kinds"]["pages"]
        .as_array()
        .expect("page channels")
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

/// One executed call of a walk: its public output and the ids of the rows it
/// delivered.
pub struct Page {
    pub output: Value,
    pub rows: Vec<String>,
}

/// What a walk saw: every row id (each exactly once) and every call it made,
/// in order.
#[derive(Debug, Default)]
pub struct Walk {
    pub rows: Vec<String>,
    pub queries: Vec<Value>,
}

/// A page walk of `tool` from `start`: each call's output satisfies the output
/// contract and offers only contract-valid leads; every page lead (`next.<k>`,
/// `k` a contract page channel, naming `tool`) is followed breadth-first,
/// verbatim. A branch's last page may re-offer the one call that continues
/// past it (it is not rerun). Fails on any other repeated call, a row
/// delivered twice, or more than
/// [`WALK_CAP`] calls. Drive it with [`walk`], or call [`Walker::next_query`]
/// and [`Walker::accept`] from an async test.
pub struct Walker {
    tool: String,
    channels: BTreeSet<String>,
    queue: std::collections::VecDeque<Value>,
    seen_queries: BTreeSet<String>,
    seen_rows: BTreeSet<String>,
    walk: Walk,
}

impl Walker {
    pub fn new(tool: &str, start: Value) -> Self {
        Self {
            tool: tool.to_owned(),
            channels: page_channels(),
            queue: std::collections::VecDeque::from([start]),
            seen_queries: BTreeSet::new(),
            seen_rows: BTreeSet::new(),
            walk: Walk::default(),
        }
    }

    /// The next call to make, or `None` when every page lead has been followed.
    pub fn next_query(&mut self) -> Option<Value> {
        let query = self.queue.pop_front()?;
        assert!(
            self.seen_queries.insert(canonical(&query)),
            "{} walk repeats a call: {query}",
            self.tool
        );
        assert!(
            self.walk.queries.len() < WALK_CAP,
            "{} walk still paging after {WALK_CAP} calls",
            self.tool
        );
        self.walk.queries.push(query.clone());
        Some(query)
    }

    /// Check the page the last [`Walker::next_query`] call produced and queue
    /// its page leads.
    pub fn accept(&mut self, page: Page) {
        assert_output_contract(&self.tool, &page.output);
        assert_leads_valid(&page.output);
        for row in page.rows {
            assert!(
                self.seen_rows.insert(row.clone()),
                "{} walk delivers `{row}` twice (call {})",
                self.tool,
                self.walk.queries.len()
            );
            self.walk.rows.push(row);
        }
        let page_leads = leads(&page.output)
            .into_iter()
            .filter(|lead| {
                lead.channel == "next"
                    && lead.tool == self.tool
                    && self.channels.contains(&lead.name)
            })
            .collect::<Vec<_>>();
        // A branch's last page may re-offer the one call that continues past
        // it (a localSearch match page that ends its file group re-offers
        // `nextPage`), so a linear reader never dead-ends; the walk does not
        // rerun it. Any other repeated call still fails in `next_query`.
        let branch_end = page_leads.len() == 1;
        for lead in page_leads {
            let key = canonical(&lead.query);
            if branch_end
                && (self.seen_queries.contains(&key)
                    || self.queue.iter().any(|queued| canonical(queued) == key))
            {
                continue;
            }
            self.queue.push_back(lead.query);
        }
    }

    pub fn finish(self) -> Walk {
        self.walk
    }
}

/// [`Walker`] driven by a synchronous `exec`.
pub fn walk(tool: &str, start: Value, mut exec: impl FnMut(&Value) -> Page) -> Walk {
    let mut walker = Walker::new(tool, start);
    while let Some(query) = walker.next_query() {
        walker.accept(exec(&query));
    }
    walker.finish()
}
