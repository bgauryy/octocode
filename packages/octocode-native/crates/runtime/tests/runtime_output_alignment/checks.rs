//! The shape checks, generic over tools: each reads one response and reports
//! what deviates from the protocol as `check | tool | detail`.

use regex::Regex;
use serde_json::{Map, Value};
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::LazyLock;

/// The closed vocabularies of the RFC's concept registry (core
/// `conceptRegistry.ts`), which the generated contract does not carry yet.
const ENVELOPE_KEYS: [&str; 6] = [
    "results",
    "responsePagination",
    "warnings",
    "oversized",
    "error",
    "errorCode",
];
const ROW_KEYS: [&str; 4] = ["index", "data", "status", "rowPart"];
const ROW_STATUSES: [&str; 3] = ["empty", "error", "outOfRange"];
const PAGINATION_KEYS: [&str; 6] = ["unit", "offset", "length", "total", "countScope", "hasMore"];
const RESPONSE_PAGINATION_KEYS: [&str; 4] = ["hasMore", "next", "expiresAt", "replayed"];
/// Fraction digits a ratio or score may carry.
const FRACTION_DIGITS: usize = 3;
/// Elements a collection needs before a shared value reads as stated once.
const UNIFORM_ROWS: usize = 3;

pub type Findings = BTreeSet<String>;

pub struct Report<'a> {
    pub tool: &'a str,
    pub findings: &'a mut Findings,
}

impl Report<'_> {
    pub fn add(&mut self, check: &str, detail: impl AsRef<str>) {
        self.findings
            .insert(format!("{check} | {} | {}", self.tool, detail.as_ref()));
    }
}

/// A continuation or a lead: `{tool, input}`.
pub fn is_call(value: &Value) -> bool {
    value["tool"].is_string() && value["input"].is_object()
}

/// Where a node reads, array positions as `[]`, a row's `data` as the root.
pub fn place(path: &[String]) -> String {
    let mut text = String::new();
    for segment in path {
        if segment != "[]" && !text.is_empty() {
            text.push('.');
        }
        text.push_str(segment);
    }
    text.strip_prefix("results[].data.")
        .map_or(text.clone(), str::to_owned)
}

/// Every node of `value` with its path; a call is visited, its input is not.
pub fn each_node(value: &Value, path: &mut Vec<String>, visit: &mut dyn FnMut(&[String], &Value)) {
    visit(path, value);
    if is_call(value) {
        return;
    }
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                path.push(key.clone());
                each_node(child, path, visit);
                path.pop();
            }
        }
        Value::Array(items) => {
            path.push("[]".to_owned());
            for item in items {
                each_node(item, path, visit);
            }
            path.pop();
        }
        _ => {}
    }
}

fn rows(output: &Value) -> &[Value] {
    output["results"].as_array().map_or(&[], Vec::as_slice)
}

/// B1: the envelope and row key sets, and the closed row status.
pub fn envelope(report: &mut Report, output: &Value) {
    for key in output.as_object().into_iter().flat_map(Map::keys) {
        if !ENVELOPE_KEYS.contains(&key.as_str()) {
            report.add("B1 envelope", format!("envelope key {key}"));
        }
    }
    for row in rows(output) {
        for key in row.as_object().into_iter().flat_map(Map::keys) {
            if !ROW_KEYS.contains(&key.as_str()) {
                report.add("B1 envelope", format!("row key {key}"));
            }
        }
        if !row["index"].is_u64() || !row["data"].is_object() {
            report.add("B1 envelope", "row without index and data");
        }
        if let Some(status) = row.get("status")
            && !status.as_str().is_some_and(|s| ROW_STATUSES.contains(&s))
        {
            report.add("B1 envelope", format!("row status {status}"));
        }
    }
}

fn is_pagination_name(key: &str) -> bool {
    key == "pagination" || key.ends_with("Pagination")
}

/// The pagination objects under one pagination-like key: the object itself,
/// or each section of a record of them.
fn pagination_objects<'a>(key: &str, value: &'a Value) -> Vec<(String, &'a Map<String, Value>)> {
    let Some(object) = value.as_object() else {
        return Vec::new();
    };
    if object.contains_key("hasMore") {
        return vec![(key.to_owned(), object)];
    }
    object
        .values()
        .filter_map(Value::as_object)
        .filter(|section| section.contains_key("hasMore"))
        .map(|section| (format!("{key}.*"), section))
        .collect()
}

/// One pagination object's keys against `allowed`; `reachable` says whether
/// a continuation stands behind its `hasMore`.
fn page_facts(
    report: &mut Report,
    (owner, at): (&str, &str),
    object: &Map<String, Value>,
    (allowed, reachable): (&[&str], bool),
) {
    for field in object.keys().filter(|key| !allowed.contains(&key.as_str())) {
        report.add("B2 pagination", format!("{owner}.{field}"));
    }
    // A row list states no unit or length (the operation names its rows and
    // the rows listed are the page); a text window states both.
    let unit = object.get("unit").and_then(Value::as_str);
    if unit.is_some_and(|unit| !matches!(unit, "lines" | "bytes" | "chars")) {
        report.add(
            "B2 pagination",
            format!("{owner} unit {unit:?} on a row list"),
        );
    }
    if unit.is_none() && object.contains_key("length") {
        report.add(
            "B2 pagination",
            format!("{owner} length without a text unit"),
        );
    }
    if object.get("hasMore") == Some(&Value::Bool(true)) && !reachable {
        report.add("B2 pagination", format!("hasMore without next: {at}"));
    }
}

/// B2: one pagination vocabulary, and a continuation behind every `hasMore`:
/// the row's own `next`, or the response's one walk handle.
pub fn pagination(report: &mut Report, output: &Value) {
    let walk = output.get("responsePagination").and_then(Value::as_object);
    let walk_handle = walk.is_some_and(|walk| walk.contains_key("next"));
    if let Some(walk) = walk {
        let name = "responsePagination";
        page_facts(
            report,
            (name, name),
            walk,
            (&RESPONSE_PAGINATION_KEYS, walk_handle),
        );
    }
    for row in rows(output) {
        let row_next = row
            .pointer("/data/next")
            .and_then(Value::as_object)
            .is_some_and(|calls| !calls.is_empty());
        let mut path = vec!["results".to_owned(), "[]".to_owned()];
        each_node(row, &mut path, &mut |path, node| {
            let Some(key) = path.last().filter(|key| is_pagination_name(key)) else {
                return;
            };
            for (owner, object) in pagination_objects(key, node) {
                let reachable = walk_handle || row_next;
                page_facts(
                    report,
                    (&owner, &place(path)),
                    object,
                    (&PAGINATION_KEYS, reachable),
                );
            }
        });
    }
}

/// B2: every continuation is `{cursor}` under `next`; every lead is under `related`.
pub fn call_places(report: &mut Report, output: &Value) {
    static NEXT: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(next\.[^.\[]+|responsePagination\.next)$").expect("regex"));
    static RELATED: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^related\[\]$").expect("regex"));
    let mut path = Vec::new();
    each_node(output, &mut path, &mut |path, node| {
        if !is_call(node) {
            return;
        }
        let at = place(path);
        let target = node["tool"].as_str().unwrap_or_default();
        let input = node["input"].as_object();
        if input.is_some_and(|input| input.contains_key("cursor")) {
            if !NEXT.is_match(&at) {
                report.add("B2 continuation", format!("outside next: {at}"));
            }
            if input.is_some_and(|input| input.len() != 1) {
                report.add("B2 continuation", format!("input beyond cursor: {at}"));
            }
        } else if NEXT.is_match(&at) {
            report.add("B2 continuation", format!("not a cursor: {at} -> {target}"));
        } else if !RELATED.is_match(&at) {
            report.add("B3 lead", format!("outside related: {at} -> {target}"));
        }
    });
}

/// B3: every `path` a local tool emits resolves from the workspace root.
pub fn paths(report: &mut Report, output: &Value, root: &Path) {
    for row in rows(output).iter().filter(|row| row["status"] != "error") {
        let mut path = vec!["results".to_owned(), "[]".to_owned()];
        each_node(row, &mut path, &mut |path, node| {
            let (Some("path"), Some(text)) = (path.last().map(String::as_str), node.as_str())
            else {
                return;
            };
            if Path::new(text).is_absolute() {
                report.add("B3 path", format!("absolute: {}", place(path)));
            } else if !root.join(text).exists() {
                report.add("B3 path", format!("does not resolve: {}", place(path)));
            }
        });
    }
}

/// Scalar leaves of `value` with their paths; calls and prose are left out.
fn leaves(value: &Value) -> Vec<(Vec<String>, Value)> {
    let mut found = Vec::new();
    let mut path = Vec::new();
    each_node(value, &mut path, &mut |path, node| {
        if matches!(node, Value::String(_) | Value::Number(_)) {
            found.push((path.to_vec(), node.clone()));
        }
    });
    found
}

/// A request value a response may not restate (Rule E): a string, or a
/// number under the same name. Booleans and one-character strings say too
/// little to be told from a coincidence.
fn restates(request: &(Vec<String>, Value), response: &(Vec<String>, Value)) -> bool {
    match (&request.1, &response.1) {
        (Value::String(sent), Value::String(stated)) => sent == stated && sent.chars().count() > 1,
        (Value::Number(sent), Value::Number(stated)) => {
            sent == stated && request.0.last() == response.0.last()
        }
        _ => false,
    }
}

/// B4, Rule E: no response field restates a request value.
pub fn echoes(report: &mut Report, request: &Value, data: &Value) {
    let Some(request) = request.as_object() else {
        return;
    };
    let sent: Vec<_> = request
        .iter()
        .filter(|(key, _)| !matches!(key.as_str(), "mainGoal" | "reasoning"))
        .flat_map(|(key, value)| {
            leaves(value).into_iter().map(move |(mut path, leaf)| {
                path.insert(0, key.clone());
                (path, leaf)
            })
        })
        .collect();
    let stated: Vec<_> = data
        .as_object()
        .into_iter()
        .flatten()
        // Calls carry inputs by design; prose quotes what it explains.
        .filter(|(key, _)| !matches!(key.as_str(), "next" | "related" | "hints" | "error"))
        .flat_map(|(key, value)| {
            leaves(value).into_iter().map(move |(mut path, leaf)| {
                path.insert(0, key.clone());
                (path, leaf)
            })
        })
        .collect();
    for response in &stated {
        for request in sent.iter().filter(|request| restates(request, response)) {
            report.add(
                "B4 echo",
                format!("{} <- request {}", place(&response.0), place(&request.0)),
            );
        }
    }
}

/// B5, Rule H: uniform published presentation fields are hoisting
/// candidates. Opaque source records are evidence, not shared context.
pub fn uniform(report: &mut Report, data: &Value) {
    let contract = octocode_native::contracts::parsed_contract().expect("embedded contract");
    let schema = &super::tool_facts(report.tool)["outputSchema"];
    uniform_with_schema(report, data, schema, contract);
}

/// Published schemas own named presentation fields; dynamic records and
/// unknown subtrees retain their source-local occurrence and provenance.
struct Ownership<'a> {
    schema: &'a Value,
    contract: &'a Value,
}

impl<'a> Ownership<'a> {
    fn expand(
        &self,
        report: &mut Report,
        roots: Vec<&'a Value>,
        path: &[String],
    ) -> Vec<&'a Value> {
        let mut pending = roots;
        let mut seen = BTreeSet::new();
        let mut schemas = Vec::new();
        while let Some(schema) = pending.pop() {
            if !seen.insert(std::ptr::from_ref(schema))
                || schema == &Value::Bool(false)
                || schema.get("not").is_some_and(|not| {
                    not == &Value::Bool(true) || not.as_object().is_some_and(Map::is_empty)
                })
            {
                continue;
            }
            if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
                let target = reference.strip_prefix('#').and_then(|pointer| {
                    self.schema
                        .pointer(pointer)
                        .or_else(|| self.contract.pointer(pointer))
                });
                match target {
                    Some(target) => pending.push(target),
                    None => report.add(
                        "B5 contract route",
                        format!("unresolved reference {reference} at {}", place(path)),
                    ),
                }
            }
            for key in ["allOf", "anyOf", "oneOf"] {
                pending.extend(schema[key].as_array().into_iter().flatten());
            }
            schemas.push(schema);
        }
        schemas
    }

    fn property(schemas: &[&'a Value], key: &str) -> Vec<&'a Value> {
        schemas
            .iter()
            .filter_map(|schema| schema["properties"].get(key))
            .collect()
    }

    fn item(schemas: &[&'a Value], index: usize) -> Vec<&'a Value> {
        schemas
            .iter()
            .filter_map(|schema| {
                schema["prefixItems"]
                    .get(index)
                    .or_else(|| schema.get("items"))
            })
            .collect()
    }

    fn boundary(schemas: &[&Value]) -> bool {
        let named = |schema: &&Value| {
            schema["properties"]
                .as_object()
                .is_some_and(|fields| !fields.is_empty())
        };
        let has_named = schemas.iter().any(named);
        let has_shape = schemas.iter().any(|schema| {
            [
                "type",
                "properties",
                "items",
                "prefixItems",
                "const",
                "enum",
            ]
            .iter()
            .any(|key| schema.get(*key).is_some())
        });
        !has_shape
            || schemas.iter().any(|schema| {
                let delegates = ["$ref", "allOf", "anyOf", "oneOf"]
                    .iter()
                    .any(|key| schema.get(*key).is_some());
                let fixed = schema.get("const").is_some() || schema.get("enum").is_some();
                let record = !named(schema)
                    && schema
                        .get("additionalProperties")
                        .is_some_and(|value| value != &Value::Bool(false));
                let open_object = !has_named
                    && schema["type"] == "object"
                    && schema.get("additionalProperties") != Some(&Value::Bool(false));
                let unknown = schema == &&Value::Bool(true)
                    || (!delegates
                        && ![
                            "type",
                            "properties",
                            "items",
                            "prefixItems",
                            "const",
                            "enum",
                        ]
                        .iter()
                        .any(|key| schema.get(*key).is_some()));
                !fixed && (record || open_object || unknown)
            })
    }

    fn owned(
        &self,
        report: &mut Report,
        value: &Value,
        roots: Vec<&'a Value>,
        path: &mut Vec<String>,
    ) -> bool {
        let schemas = self.expand(report, roots, path);
        if Self::boundary(&schemas) {
            return false;
        }
        if schemas.iter().any(|schema| {
            schema.get("const") == Some(value)
                || schema["enum"]
                    .as_array()
                    .is_some_and(|values| values.contains(value))
        }) {
            return true;
        }
        match value {
            Value::Object(fields) => fields.iter().all(|(key, value)| {
                path.push(key.clone());
                let owned = self.owned(report, value, Self::property(&schemas, key), path);
                path.pop();
                owned
            }),
            Value::Array(items) => {
                let item = self.expand(report, Self::item(&schemas, 0), path);
                !Self::boundary(&item)
                    && items.iter().enumerate().all(|(index, value)| {
                        path.push("[]".into());
                        let owned = self.owned(report, value, Self::item(&schemas, index), path);
                        path.pop();
                        owned
                    })
            }
            _ => true,
        }
    }

    fn walk(
        &self,
        report: &mut Report,
        value: &Value,
        roots: Vec<&'a Value>,
        path: &mut Vec<String>,
    ) {
        let schemas = self.expand(report, roots, path);
        if Self::boundary(&schemas) || is_call(value) {
            return;
        }
        match value {
            Value::Array(items) => {
                if items.len() >= UNIFORM_ROWS
                    && let Some(first) = items.first().and_then(Value::as_object)
                {
                    for (key, value) in first {
                        if value.is_null() || !items.iter().all(|item| item.get(key) == Some(value))
                        {
                            continue;
                        }
                        let published = items.iter().enumerate().all(|(index, item)| {
                            path.push("[]".into());
                            let item_schemas =
                                self.expand(report, Self::item(&schemas, index), path);
                            path.push(key.clone());
                            let owned = !Self::boundary(&item_schemas)
                                && self.owned(
                                    report,
                                    &item[key],
                                    Self::property(&item_schemas, key),
                                    path,
                                );
                            path.pop();
                            path.pop();
                            owned
                        });
                        if published {
                            report.add("B5 stated once", format!("{}[].{key}", place(path)));
                        }
                    }
                }
                path.push("[]".into());
                for (index, item) in items.iter().enumerate() {
                    self.walk(report, item, Self::item(&schemas, index), path);
                }
                path.pop();
            }
            Value::Object(fields) => {
                for (key, value) in fields {
                    path.push(key.clone());
                    self.walk(report, value, Self::property(&schemas, key), path);
                    path.pop();
                }
            }
            _ => {}
        }
    }
}

fn uniform_with_schema(report: &mut Report, data: &Value, schema: &Value, contract: &Value) {
    let ownership = Ownership { schema, contract };
    let mut roots = ownership.expand(report, vec![schema], &[]);
    roots = Ownership::property(&roots, "results");
    roots = ownership.expand(report, roots, &[]);
    roots = Ownership::item(&roots, 0);
    roots = ownership.expand(report, roots, &[]);
    roots = Ownership::property(&roots, "data");
    if roots.is_empty() {
        report.add("B5 contract route", "missing published result data schema");
        return;
    }
    ownership.walk(report, data, roots, &mut Vec::new());
}

fn timestamp_fault(text: &str) -> Option<&'static str> {
    static STAMP: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^\d{4}-\d{2}-\d{2}([T ]\d{2}:\d{2}.*)?$").expect("regex"));
    static SECONDS_UTC: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$").expect("regex"));
    if !STAMP.is_match(text) || SECONDS_UTC.is_match(text) {
        return None;
    }
    Some(if !text.contains(':') {
        "date only"
    } else if text.contains('.') {
        "fraction of a second"
    } else if !text.ends_with('Z') {
        "offset, not Z"
    } else {
        "not seconds"
    })
}

/// B6: one timestamp format, and at most three fraction digits.
pub fn formats(report: &mut Report, output: &Value) {
    let mut path = Vec::new();
    each_node(output, &mut path, &mut |path, node| match node {
        Value::String(text) => {
            if let Some(fault) = timestamp_fault(text) {
                report.add("B6 format", format!("timestamp {}: {fault}", place(path)));
            }
        }
        Value::Number(number) if number.is_f64() => {
            let text = number.to_string();
            let digits = text.split_once('.').map_or(0, |(_, fraction)| {
                fraction.chars().take_while(char::is_ascii_digit).count()
            });
            if digits > FRACTION_DIGITS {
                report.add(
                    "B6 format",
                    format!(
                        "number {}: more than {FRACTION_DIGITS} fraction digits",
                        place(path)
                    ),
                );
            }
        }
        _ => {}
    });
}

/// B7: an empty row keeps one of its tool's collections, as `[]`, and
/// states no zero counter. `collections` are the tool's row-level lists.
pub fn empty_rows(report: &mut Report, output: &Value, collections: &[String]) {
    for row in rows(output).iter().filter(|row| row["status"] == "empty") {
        let data = &row["data"];
        if !collections.is_empty()
            && !collections
                .iter()
                .any(|key| data[key].as_array().is_some_and(Vec::is_empty))
        {
            report.add("B7 empty row", "no collection key");
        }
        for (key, value) in data.as_object().into_iter().flatten() {
            if value.as_u64() == Some(0) {
                report.add("B7 empty row", format!("zero counter {key}"));
            }
        }
    }
}

/// The fields `tool` publishes for one query, in the order `schema <tool>`
/// and `tools/list` state them.
fn published_fields(tool: &str) -> Vec<String> {
    let Ok(presentation) = octocode_native::contracts::cli_presentation() else {
        return Vec::new();
    };
    let entry = presentation["tools"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|entry| entry["name"] == tool);
    let Some(entry) = entry else {
        return Vec::new();
    };
    let widest = &entry["selection"]["mcp"]["inputSchema"];
    let schema = if widest.is_object() {
        widest
    } else {
        &entry["mcp"]["inputSchema"]
    };
    schema["properties"]["queries"]["items"]["properties"]
        .as_object()
        .into_iter()
        .flat_map(Map::keys)
        .cloned()
        .collect()
}

/// B8: the objects of one list state the keys they share in one order, and
/// a call states each query in the order its target publishes the fields.
pub fn key_order(report: &mut Report, output: &Value) {
    let mut path = Vec::new();
    each_node(output, &mut path, &mut |path, node| {
        if is_call(node) {
            let published = published_fields(node["tool"].as_str().unwrap_or_default());
            let rank = |key: &String| published.iter().position(|field| field == key);
            for query in node["input"]["queries"].as_array().into_iter().flatten() {
                let ranks: Vec<_> = query
                    .as_object()
                    .into_iter()
                    .flat_map(Map::keys)
                    .map(|key| rank(key).unwrap_or(published.len()))
                    .collect();
                if !ranks.is_sorted() {
                    report.add("B8 key order", format!("call input: {}", place(path)));
                }
            }
            return;
        }
        let Some(items) = node.as_array() else {
            return;
        };
        // Each pair of keys in the order an earlier element stated it.
        let mut stated = BTreeSet::new();
        for keys in items.iter().filter_map(Value::as_object).map(Map::keys) {
            let keys: Vec<&String> = keys.collect();
            for (at, earlier) in keys.iter().enumerate() {
                for later in &keys[at + 1..] {
                    if stated.contains(&(*later, *earlier)) {
                        report.add(
                            "B8 key order",
                            format!("{}[]: {earlier} and {later}", place(path)),
                        );
                    }
                    stated.insert((*earlier, *later));
                }
            }
        }
    });
}

#[cfg(test)]
#[path = "checks_tests.rs"]
mod tests;
