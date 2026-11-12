//! Each tool's output-schema call positions, compiled as the walk needs them.
//!
//! Every response is walked against its output schema to find the declared
//! call positions (publication, the lspSearch pin, the linear-walk hoist). At
//! each node the walk asks the same questions: which schemas apply to an
//! array item or to a key, and whether one of them declares a call (`kind`,
//! `tool`, `input`). Every answer is a union over the set of schemas that
//! apply at that node, so the answers depend on that set alone: each distinct
//! set gets one [`Node`], and each answer is worked out the first time a walk
//! asks it, then kept for the life of the process. A process that reads one
//! response pays only for the part of the schema that response enters, not
//! for the whole schema. A node whose subtree holds no call position is
//! marked when a walk first reaches it, and the walk skips it.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

use serde_json::Value;

use crate::contracts;
use crate::id::ToolId;

/// Call `visit` with each call at a declared position of `value` under
/// `tool`'s output schema, and its path, in document order. The schema's
/// answers are kept per process; the walk holds them only while it reads
/// `value`, so `visit` runs unlocked.
pub(super) fn visit(
    tool: ToolId,
    value: &Value,
    visit: &mut dyn FnMut(&Value, &[String]),
) -> Result<(), &'static str> {
    static TREES: [OnceLock<Mutex<CallTree<'static>>>; ToolId::ALL.len()] =
        [const { OnceLock::new() }; ToolId::ALL.len()];
    let contract = contracts::tool_contract(tool)?;
    let found = TREES[tool.index()]
        .get_or_init(|| Mutex::new(CallTree::new(&contract["outputSchema"])))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .calls(value);
    for (path, call) in found {
        visit(call, &path);
    }
    Ok(())
}

/// The schemas that apply at one output position, and what the walk has
/// asked of them so far. `None` is an answer not yet worked out.
#[derive(Default)]
struct Node<'a> {
    /// One of the schemas declares a call here.
    call: bool,
    /// The node of an array's items, when any schema declares them; unset
    /// until asked.
    items: std::cell::OnceCell<Option<usize>>,
    /// The node of any key no schema names: typed `additionalProperties`;
    /// unset until asked.
    additional: std::cell::OnceCell<Option<usize>>,
    /// Every key some schema's `properties` names.
    names: Option<BTreeSet<&'a str>>,
    /// The node of each named key asked so far; `None` when its schemas are
    /// empty, so the key falls to [`Self::additional`].
    properties: HashMap<String, Option<usize>>,
    /// A call position is at or below this node.
    reaches_call: Option<bool>,
}

/// An output schema's nodes; the root is node 0.
pub(crate) struct CallTree<'a> {
    defs: &'a Value,
    sets: Sets<'a>,
    nodes: Vec<Node<'a>>,
}

impl<'a> CallTree<'a> {
    /// The tree of `schema` (an output schema with its own `$defs`), with
    /// only its root resolved.
    pub(super) fn new(schema: &'a Value) -> Self {
        let mut tree = Self {
            defs: &schema["$defs"],
            sets: Sets::default(),
            nodes: Vec::new(),
        };
        tree.intern(vec![schema]);
        tree
    }

    /// The node of `set`, created on first sight. Each set of schemas gets
    /// one node, so recursive definitions terminate.
    fn intern(&mut self, set: Vec<&'a Value>) -> Option<usize> {
        let id = self.sets.intern(set)?;
        if id == self.nodes.len() {
            let defs = self.defs;
            let call = self.sets.found[id]
                .iter()
                .any(|schema| declares_call(schema, defs));
            self.nodes.push(Node {
                call,
                ..Node::default()
            });
        }
        Some(id)
    }

    /// `members` of every schema of node `id`, together, as a node.
    fn member_node(
        &mut self,
        id: usize,
        members: impl Fn(&'a Value) -> Vec<&'a Value>,
    ) -> Option<usize> {
        let set = union(&self.sets.found[id], members);
        self.intern(set)
    }

    fn items(&mut self, id: usize) -> Option<usize> {
        if let Some(items) = self.nodes[id].items.get() {
            return *items;
        }
        let defs = self.defs;
        let items = self.member_node(id, |schema| contracts::item_schemas(schema, defs));
        let _ = self.nodes[id].items.set(items);
        items
    }

    fn additional(&mut self, id: usize) -> Option<usize> {
        if let Some(additional) = self.nodes[id].additional.get() {
            return *additional;
        }
        let defs = self.defs;
        let additional = self.member_node(id, |schema| contracts::additional_schemas(schema, defs));
        let _ = self.nodes[id].additional.set(additional);
        additional
    }

    fn names(&mut self, id: usize) -> &BTreeSet<&'a str> {
        if self.nodes[id].names.is_none() {
            let defs = self.defs;
            let names = self.sets.found[id]
                .iter()
                .flat_map(|schema| contracts::property_names(schema, defs))
                .collect();
            self.nodes[id].names = Some(names);
        }
        self.nodes[id].names.get_or_insert_default()
    }

    /// The node of the key `name` some schema of node `id` names; `None`
    /// when no schema names it or its schemas are empty.
    fn named(&mut self, id: usize, name: &str) -> Option<usize> {
        if let Some(child) = self.nodes[id].properties.get(name) {
            return *child;
        }
        let name = *self.names(id).get(name)?;
        let defs = self.defs;
        let child = self.member_node(id, |schema| contracts::child_schemas(schema, defs, name));
        self.nodes[id].properties.insert(name.to_owned(), child);
        child
    }

    /// The node the walk enters for `key` of an object at node `id`.
    fn child(&mut self, id: usize, key: &str) -> Option<usize> {
        self.named(id, key).or_else(|| self.additional(id))
    }

    /// Every node one step below node `id`, resolving all of them.
    fn children(&mut self, id: usize) -> Vec<usize> {
        let mut children = Vec::new();
        children.extend(self.items(id));
        children.extend(self.additional(id));
        let names = self.names(id).iter().copied().collect::<Vec<_>>();
        for name in names {
            children.extend(self.named(id, name));
        }
        children
    }

    /// Whether a call position is at or below node `id`. The first ask
    /// searches the schemas below it, stopping at the first call it meets; a
    /// search that meets none proves every node it read reaches none.
    fn reaches(&mut self, id: usize) -> bool {
        let mut read = HashSet::new();
        let found = self.search(id, &mut read);
        if !found {
            for id in read {
                self.nodes[id].reaches_call = Some(false);
            }
        }
        found
    }

    /// [`Self::reaches`] below `id`, not reading a node `read` already holds:
    /// one this search met is either still being searched (a cycle back to
    /// it adds nothing) or found no call.
    fn search(&mut self, id: usize, read: &mut HashSet<usize>) -> bool {
        if let Some(known) = self.nodes[id].reaches_call {
            return known;
        }
        if !read.insert(id) {
            return false;
        }
        let found = self.nodes[id].call
            || self
                .children(id)
                .into_iter()
                .any(|child| self.search(child, read));
        if found {
            self.nodes[id].reaches_call = Some(true);
        }
        found
    }

    /// Each call at a declared position of `value`, with its path, in
    /// document order. A call's own contents are not searched.
    fn calls<'v>(&mut self, value: &'v Value) -> Vec<(Vec<String>, &'v Value)> {
        let mut found = Vec::new();
        if self.reaches(0) {
            self.walk(0, value, &mut Vec::new(), &mut found);
        }
        found
    }

    fn walk<'v>(
        &mut self,
        id: usize,
        value: &'v Value,
        path: &mut Vec<String>,
        found: &mut Vec<(Vec<String>, &'v Value)>,
    ) {
        if self.nodes[id].call && crate::contracts::envelope::is_call(value) {
            found.push((path.clone(), value));
            return;
        }
        match value {
            Value::Array(items) => {
                let Some(child) = self.items(id).filter(|child| self.reaches(*child)) else {
                    return;
                };
                for (index, item) in items.iter().enumerate() {
                    path.push(index.to_string());
                    self.walk(child, item, path, found);
                    path.pop();
                }
            }
            Value::Object(object) => {
                for (key, item) in object {
                    if let Some(child) = self.child(id, key).filter(|child| self.reaches(*child)) {
                        path.push(key.clone());
                        self.walk(child, item, path, found);
                        path.pop();
                    }
                }
            }
            _ => {}
        }
    }
}

/// The distinct schema sets met so far, in the order found: a set's index is
/// its node's.
#[derive(Default)]
struct Sets<'a> {
    found: Vec<Vec<&'a Value>>,
    index: HashMap<Vec<usize>, usize>,
}

impl<'a> Sets<'a> {
    /// The index of `set`, as a set: order and repeats do not change what the
    /// walk reads. `None` for an empty set, which the walk does not enter.
    fn intern(&mut self, set: Vec<&'a Value>) -> Option<usize> {
        let key = set
            .iter()
            .map(|schema| std::ptr::from_ref(*schema) as usize)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        if key.is_empty() {
            return None;
        }
        let next = self.found.len();
        let id = *self.index.entry(key).or_insert(next);
        if id == next {
            self.found.push(set);
        }
        Some(id)
    }
}

/// `members` of every schema in `set`, together.
fn union<'a>(set: &[&'a Value], members: impl Fn(&'a Value) -> Vec<&'a Value>) -> Vec<&'a Value> {
    set.iter().flat_map(|schema| members(schema)).collect()
}

/// Whether `schema` declares a call: `kind`, `tool` and `input` together.
fn declares_call(schema: &Value, defs: &Value) -> bool {
    ["kind", "tool", "input"]
        .iter()
        .all(|name| !contracts::property_schemas(schema, defs, name).is_empty())
}

/// Every node resolved up front, as walks over every possible value would
/// leave them: what the tests read the tree's answers from.
#[cfg(test)]
impl<'a> CallTree<'a> {
    fn compile(schema: &'a Value) -> Self {
        let mut tree = Self::new(schema);
        let mut id = 0;
        while id < tree.nodes.len() {
            tree.children(id);
            id += 1;
        }
        for id in 0..tree.nodes.len() {
            tree.reaches(id);
        }
        tree
    }
}

#[cfg(test)]
mod tests;
