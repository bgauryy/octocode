//! One key order per list. Every element of a list a row states, and every
//! object inside one that the output schema gives named properties, states
//! its keys in one order: the order the list's first elements built them in,
//! so two rows of one list never differ by how they were built. The embedded
//! output schema lists properties by name, not by declaration, so it says
//! which objects have a shape and the rows say in what order. A record (a
//! map the schema types by value alone, whose order may rank its entries) is
//! never touched, nor is an object the schema does not describe.
//!
//! A call a row emits (`next.*`, `related[]`, a row's own lead) states each
//! query of its input in the order the target tool publishes its fields; a
//! field it does not publish keeps its place after them.
//!
//! The pass moves keys and nothing else: no key or value is added, dropped
//! or changed.
use crate::contracts::output_fields::executable_call;
use crate::contracts::{child_schemas, item_schemas, property_order};
use crate::id::ToolId;
use serde_json::{Map, Value};
use std::collections::HashMap;

type Schema = &'static Value;

/// The schemas that describe one place of a value, with what they say of
/// its keys and of the places below it, read once per pass.
struct Place {
    schemas: Vec<Schema>,
    /// The keys the schemas declare, in their published order.
    declared: Option<Vec<&'static str>>,
    /// The keys the objects read here stated, first seen first.
    stated: Vec<String>,
    members: HashMap<String, usize>,
    items: Option<usize>,
}

/// The places of one root schema (a tool's output rows, or its queries),
/// found as a value walks them.
struct Places {
    defs: Schema,
    places: Vec<Place>,
}

/// The place every value starts at.
const ROOT: usize = 0;

impl Places {
    fn new(defs: Schema, root: Vec<Schema>) -> Self {
        let mut places = Self {
            defs,
            places: Vec::new(),
        };
        places.add(root);
        places
    }

    fn add(&mut self, schemas: Vec<Schema>) -> usize {
        self.places.push(Place {
            schemas,
            declared: None,
            stated: Vec::new(),
            members: HashMap::new(),
            items: None,
        });
        self.places.len() - 1
    }

    /// The place of the member `key` of an object at `at`.
    fn member(&mut self, at: usize, key: &str) -> usize {
        if let Some(found) = self.places[at].members.get(key) {
            return *found;
        }
        let schemas = self.places[at]
            .schemas
            .iter()
            .flat_map(|schema| child_schemas(schema, self.defs, key))
            .collect();
        let found = self.add(schemas);
        self.places[at].members.insert(key.to_owned(), found);
        found
    }

    /// The place of an element of a list at `at`.
    fn item(&mut self, at: usize) -> usize {
        if let Some(found) = self.places[at].items {
            return found;
        }
        let schemas = self.places[at]
            .schemas
            .iter()
            .flat_map(|schema| item_schemas(schema, self.defs))
            .collect();
        let found = self.add(schemas);
        self.places[at].items = Some(found);
        found
    }

    /// The keys the schemas at `at` declare, in the order they publish them.
    fn declared(&mut self, at: usize) -> &[&'static str] {
        let defs = self.defs;
        let place = &mut self.places[at];
        place.declared.get_or_insert_with(|| {
            let mut declared = Vec::new();
            for schema in &place.schemas {
                for name in property_order(schema, defs) {
                    if !declared.contains(&name) {
                        declared.push(name);
                    }
                }
            }
            declared
        })
    }

    /// The key order at `at` once `map` is read there: the order earlier
    /// objects stated their keys in, then the keys `map` adds. Empty where
    /// the schemas name no property: a record, or a value they do not reach.
    fn stated(&mut self, at: usize, map: &Map<String, Value>) -> &[String] {
        if self.declared(at).is_empty() {
            return &[];
        }
        let stated = &mut self.places[at].stated;
        for key in map.keys() {
            if !stated.contains(key) {
                stated.push(key.clone());
            }
        }
        stated
    }
}

/// The schemas of one result row's `data` in `tool`'s output contract.
fn output_places(tool: ToolId) -> Option<Places> {
    let schema = &crate::contracts::tool_contract(tool).ok()?["outputSchema"];
    let defs = &schema["$defs"];
    let data = child_schemas(schema, defs, "results")
        .into_iter()
        .flat_map(|results| item_schemas(results, defs))
        .flat_map(|row| child_schemas(row, defs, "data"))
        .collect();
    Some(Places::new(defs, data))
}

/// The schemas of one query of `tool`, as its published input lists them.
fn query_places(tool: ToolId) -> Option<Places> {
    let schema = crate::contracts::published_input_schema(tool)?;
    let defs = &schema["$defs"];
    let query = child_schemas(schema, defs, "queries")
        .into_iter()
        .flat_map(|queries| item_schemas(queries, defs))
        .collect();
    Some(Places::new(defs, query))
}

struct Pass {
    output: Places,
    /// The query places of each tool a call of this response names.
    queries: HashMap<ToolId, Option<Places>>,
}

impl Pass {
    /// Orders what `value` (at `at` of the output schema) holds. `listed`:
    /// the value is an element of a list or sits inside one, so its own keys
    /// take the list's order as well.
    fn order(&mut self, value: &mut Value, at: usize, listed: bool) {
        match value {
            Value::Array(items) => {
                let at = self.output.item(at);
                for item in items {
                    self.order(item, at, true);
                }
            }
            Value::Object(map) => {
                if listed {
                    reorder(map, self.output.stated(at, map));
                }
                if executable_call(map) {
                    self.order_call(map);
                    return;
                }
                for (key, member) in map.iter_mut() {
                    if member.is_array() || member.is_object() {
                        let at = self.output.member(at, key);
                        self.order(member, at, listed);
                    }
                }
            }
            _ => {}
        }
    }

    /// Orders each query of a call's input as its target tool declares it.
    fn order_call(&mut self, call: &mut Map<String, Value>) {
        let Some(target) = call
            .get("tool")
            .and_then(Value::as_str)
            .and_then(ToolId::from_name)
        else {
            return;
        };
        let Some(places) = self
            .queries
            .entry(target)
            .or_insert_with(|| query_places(target))
        else {
            return;
        };
        let queries = call
            .get_mut("input")
            .and_then(|input| input.get_mut("queries"))
            .and_then(Value::as_array_mut);
        for query in queries.into_iter().flatten() {
            order_declared(query, places, ROOT);
        }
    }
}

/// Orders `value` and everything below it as `places` publishes from `at`.
fn order_declared(value: &mut Value, places: &mut Places, at: usize) {
    match value {
        Value::Array(items) => {
            let at = places.item(at);
            for item in items {
                order_declared(item, places, at);
            }
        }
        Value::Object(map) => {
            reorder(map, places.declared(at));
            for (key, member) in map.iter_mut() {
                if member.is_array() || member.is_object() {
                    let at = places.member(at, key);
                    order_declared(member, places, at);
                }
            }
        }
        _ => {}
    }
}

/// Moves the keys `order` names to the front of `map`, in that order; the
/// rest keep their own order after them.
fn reorder(map: &mut Map<String, Value>, order: &[impl AsRef<str>]) {
    let rank = |key: &str| {
        order
            .iter()
            .position(|name| name.as_ref() == key)
            .unwrap_or(order.len())
    };
    let mut last = 0;
    let ordered = map.keys().all(|key| {
        let rank = rank(key);
        let holds = rank >= last;
        last = rank;
        holds
    });
    if ordered {
        return;
    }
    let mut entries: Vec<(String, Value)> = std::mem::take(map).into_iter().collect();
    entries.sort_by_key(|(key, _)| rank(key));
    map.extend(entries);
}

/// Gives every list of every result row of `tool` one key order, and every
/// emitted call's queries their target's published one (see the module text).
pub(crate) fn order_rows(tool: ToolId, structured: &mut Value) {
    let Some(rows) = structured.get_mut("results").and_then(Value::as_array_mut) else {
        return;
    };
    let Some(output) = output_places(tool) else {
        return;
    };
    let mut pass = Pass {
        output,
        queries: HashMap::new(),
    };
    for row in rows {
        if let Some(data) = row.get_mut("data") {
            pass.order(data, ROOT, false);
        }
    }
}

#[cfg(test)]
mod tests;
