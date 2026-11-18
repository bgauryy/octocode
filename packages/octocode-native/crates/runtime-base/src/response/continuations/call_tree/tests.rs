use super::*;
use octocode_test_support::json_fixture;
use serde_json::json;

/// Each call found: its path, and which value of the walked document it is.
type Found = Vec<(Vec<String>, usize)>;

fn found(found: &mut Found) -> impl FnMut(&Value, &[String]) + '_ {
    |call, path| found.push((path.to_vec(), std::ptr::from_ref(call) as usize))
}

/// The walk before compilation, kept as the oracle: it resolves the schemas
/// at every node of the value it walks.
fn reference(schema: &Value, value: &Value) -> Found {
    let mut calls = Vec::new();
    reference_walk(
        value,
        &[schema],
        &schema["$defs"],
        &mut Vec::new(),
        &mut found(&mut calls),
    );
    calls
}

fn reference_walk<'a>(
    value: &Value,
    schemas: &[&'a Value],
    defs: &'a Value,
    path: &mut Vec<String>,
    visit: &mut dyn FnMut(&Value, &[String]),
) {
    if schemas.iter().any(|schema| declares_call(schema, defs))
        && crate::contracts::envelope::is_call(value)
    {
        visit(value, path);
        return;
    }
    match value {
        Value::Array(items) => {
            let declared = union(schemas, |schema| contracts::item_schemas(schema, defs));
            if declared.is_empty() {
                return;
            }
            for (index, item) in items.iter().enumerate() {
                path.push(index.to_string());
                reference_walk(item, &declared, defs, path, visit);
                path.pop();
            }
        }
        Value::Object(object) => {
            for (key, child) in object {
                let declared = union(schemas, |schema| {
                    contracts::child_schemas(schema, defs, key)
                });
                if !declared.is_empty() {
                    path.push(key.clone());
                    reference_walk(child, &declared, defs, path, visit);
                    path.pop();
                }
            }
        }
        _ => {}
    }
}

fn compiled(tree: &mut CallTree, value: &Value) -> Found {
    tree.calls(value)
        .into_iter()
        .map(|(path, call)| (path, std::ptr::from_ref(call) as usize))
        .collect()
}

/// The node a found call sits at, read along its path through `value`.
fn node_at(tree: &CallTree, value: &Value, path: &[String]) -> Option<usize> {
    let (mut id, mut at) = (0, value);
    for key in path {
        let node = &tree.nodes[id];
        (id, at) = match at {
            Value::Array(items) => (
                node.items.get().copied().flatten()?,
                items.get(key.parse::<usize>().ok()?)?,
            ),
            _ => (
                node.properties.get(key).copied().flatten().or(node
                    .additional
                    .get()
                    .copied()
                    .flatten())?,
                at.get(key)?,
            ),
        };
    }
    Some(id)
}

fn call(tool: &str) -> Value {
    json!({"kind":"continue","tool":tool,"input":{"queries":[]}})
}

fn positions(found: &Found) -> Vec<String> {
    let mut paths = found
        .iter()
        .map(|(path, _)| path.join("/"))
        .collect::<Vec<_>>();
    paths.sort();
    paths
}

/// Calls are found only where the schema declares one: through `oneOf`
/// branches, typed `additionalProperties`, array items and recursive
/// definitions. A call-shaped value at an undeclared key is data, a
/// declared position holding something else is searched below, and a call's
/// own input is never searched.
#[test]
fn calls_are_found_only_at_declared_positions() {
    let schema = json_fixture(
        r##"{
            "$defs": {
                "call": {
                    "type": "object",
                    "properties": {"kind": {}, "tool": {}, "input": {}}
                },
                "row": {
                    "oneOf": [
                        {
                            "properties": {
                                "next": {
                                    "type": "object",
                                    "additionalProperties": {"$ref": "#/$defs/call"}
                                }
                            }
                        },
                        {
                            "properties": {
                                "related": {"type": "array", "items": {"$ref": "#/$defs/call"}}
                            }
                        }
                    ]
                },
                "tree": {
                    "properties": {
                        "lead": {"$ref": "#/$defs/call"},
                        "children": {"type": "array", "items": {"$ref": "#/$defs/tree"}}
                    }
                }
            },
            "properties": {
                "results": {
                    "type": "array",
                    "items": {"properties": {"data": {"$ref": "#/$defs/row"}}}
                },
                "files": {
                    "type": "array",
                    "items": {"properties": {"metadata": {"type": "object"}}}
                },
                "tree": {"$ref": "#/$defs/tree"}
            }
        }"##,
    );
    let mut nested = call("lspSearch");
    nested["input"]["next"] = json!({"nextPage": call("lspSearch")});
    let value = json!({
        "results": [{"data": {
            "next": {"nextPage": nested, "a/b": call("localFetch")},
            "related": [call("localFetch"), {"kind":"note"}],
            "metadata": call("localFetch")
        }}],
        "files": [{"metadata": call("localFetch")}],
        "tree": {"lead": {"kind":"note"}, "children": [{"lead": call("astSearch"), "children": []}]},
        "undeclared": call("localFetch")
    });
    let mut tree = CallTree::compile(&schema);
    let found = compiled(&mut tree, &value);
    assert_eq!(
        positions(&found),
        [
            "results/0/data/next/a/b",
            "results/0/data/next/nextPage",
            "results/0/data/related/0",
            "tree/children/0/lead",
        ]
    );
    assert_eq!(found, reference(&schema, &value));
    let root = &tree.nodes[0];
    assert_eq!(root.reaches_call, Some(true));
    let (files, recursive) = (root.properties["files"], root.properties["tree"]);
    assert!(
        !tree.reaches(files.expect("files node")),
        "no call is declared under files"
    );
    assert!(
        tree.reaches(recursive.expect("tree node")),
        "a recursive definition reaches its call"
    );
    let mut lazy = CallTree::new(&schema);
    assert_eq!(
        compiled(&mut lazy, &value),
        found,
        "resolved as the walk asks"
    );
}

/// A schema that declares no call position is never walked.
#[test]
fn a_schema_without_call_positions_finds_nothing() {
    let schema = json_fixture(
        r#"{
            "properties": {"results": {"type": "array", "items": {"type": "object"}}}
        }"#,
    );
    let mut tree = CallTree::compile(&schema);
    assert_eq!(tree.nodes[0].reaches_call, Some(false));
    assert!(compiled(&mut tree, &json!({"results":[call("localFetch")]})).is_empty());
}

/// Values shaped by a schema, mixing every branch the walk takes: declared
/// calls, declared positions holding other values, arrays and objects at a
/// node that admits both, typed and untyped extra keys, and call-shaped data
/// everywhere a schema runs out. Each seed picks the branches at random;
/// each schema set is expanded twice per value, so recursive schemas stay
/// finite.
struct Instances<'a> {
    defs: &'a Value,
    expanded: HashMap<Vec<usize>, usize>,
    state: u64,
}

impl<'a> Instances<'a> {
    fn new(defs: &'a Value, seed: u64) -> Self {
        Self {
            defs,
            expanded: HashMap::new(),
            state: seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1,
        }
    }

    /// One of three branches, from a xorshift sequence.
    fn branch(&mut self) -> u64 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        self.state % 3
    }

    fn value(&mut self, set: Vec<&'a Value>) -> Value {
        let key = set
            .iter()
            .map(|schema| std::ptr::from_ref(*schema) as usize)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let expanded = self.expanded.entry(key).or_default();
        *expanded += 1;
        if set.is_empty() || *expanded > 2 {
            return call("localFetch");
        }
        let branch = self.branch();
        let defs = self.defs;
        let declares = set.iter().any(|schema| declares_call(schema, defs));
        if declares && branch == 0 {
            let mut found = call("localSearch");
            found["input"]["related"] = json!([call("localFetch")]);
            return found;
        }
        let items = union(&set, |schema| contracts::item_schemas(schema, defs));
        if !items.is_empty() && branch == 1 {
            return json!([
                self.value(items.clone()),
                call("localFetch"),
                self.value(items)
            ]);
        }
        let mut object = serde_json::Map::new();
        let names = set
            .iter()
            .flat_map(|schema| contracts::property_names(schema, defs))
            .collect::<BTreeSet<_>>();
        for name in names {
            let child = union(&set, |schema| contracts::child_schemas(schema, defs, name));
            object.insert(name.to_owned(), self.value(child));
        }
        let extra = union(&set, |schema| contracts::additional_schemas(schema, defs));
        object.insert("extra/~key".into(), self.value(extra));
        if declares {
            object.insert("kind".into(), json!("note"));
        }
        Value::Object(object)
    }
}

/// Every tool's compiled walk finds exactly what the per-node schema walk
/// finds, in the same order, on values that take every branch of its
/// output schema. The values are strong enough to catch a wrong walk: one
/// that misses any single declared call position disagrees on them.
fn assert_tools_find_the_calls_the_schema_walk_finds(every: usize) {
    for tool in ToolId::ALL.into_iter().step_by(every) {
        let schema = &contracts::tool_contract(tool).expect("embedded contract")["outputSchema"];
        let mut tree = CallTree::compile(schema);
        let calls = (0..tree.nodes.len())
            .filter(|id| tree.nodes[*id].call)
            .collect::<Vec<_>>();
        assert!(!calls.is_empty(), "{tool} declares no call position");
        // Seeds until every call position holds a call in some value.
        let (mut values, mut expected, mut witness) = (Vec::new(), Vec::new(), HashMap::new());
        for seed in 0.. {
            let unreached = calls
                .iter()
                .filter(|id| !witness.contains_key(&Some(**id)))
                .collect::<Vec<_>>();
            if unreached.is_empty() {
                break;
            }
            assert!(
                seed < 512,
                "{tool}: no value reaches call nodes {unreached:?}"
            );
            let value = Instances::new(&schema["$defs"], seed).value(vec![schema]);
            let found = reference(schema, &value);
            assert_eq!(compiled(&mut tree, &value), found, "{tool}, seed {seed}");
            // The production tree resolves only what this value asks of it.
            assert_eq!(
                compiled(&mut CallTree::new(schema), &value),
                found,
                "{tool}, seed {seed}, resolved as asked"
            );
            for (path, _) in &found {
                witness
                    .entry(node_at(&tree, &value, path))
                    .or_insert(values.len());
            }
            values.push(value);
            expected.push(found);
        }
        for id in calls {
            let index = witness[&Some(id)];
            tree.nodes[id].call = false;
            assert_ne!(
                compiled(&mut tree, &values[index]),
                expected[index],
                "{tool}: a walk that misses node {id} must disagree"
            );
            tree.nodes[id].call = true;
        }
    }
}

/// One tool in three, from the first, in the order of [`ToolId::ALL`]: the
/// output schemas share their call definitions, so a walk that misreads one
/// declared position misreads it in several tools.
#[test]
fn every_tool_finds_the_calls_the_schema_walk_finds() {
    assert_tools_find_the_calls_the_schema_walk_finds(3);
}

#[test]
#[ignore = "every tool: cargo test -- --ignored every_tool"]
fn every_tool_finds_the_calls_the_schema_walk_finds_for_every_tool() {
    assert_tools_find_the_calls_the_schema_walk_finds(1);
}
