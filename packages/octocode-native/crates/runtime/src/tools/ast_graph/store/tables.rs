//! Projects a linked [`BuiltGraph`] into persisted [`GraphTables`]: file,
//! symbol, and package nodes joined by `contains`, `imports`, and `calls`.
//!
//! Call edges are syntactic candidates. Each one records how it was linked
//! (`local`, `import`, `namespace`, `unique-name`) and a confidence, so a
//! consumer never mistakes a name match for proven symbol identity.
use super::classify::{
    ROLE_BUNDLED, ROLE_DECLARATION, ROLE_ENTRY, ROLE_NOT_AUTHORED, ROLE_TEST, ROLE_UNPARSED,
    file_roles,
};
use super::format::{
    Confidence, DiagRec, EdgeKind, EdgeRec, FLAG_EXPORTED, FLAG_LOCAL_USE, FLAG_TEST, GraphTables,
    NONE, NodeKind, NodeRec,
};
use super::workspace::{Declared, Workspace};
use crate::tools::ast_graph::edges::Load;
use crate::tools::ast_graph::types::{BuiltGraph, Declaration, DiagnosticCode, FileFacts};
use octocode_engine::signatures::TYPE_DECLARATION_KINDS;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

mod assemble;
mod names;
mod naming;
mod projector;
mod stats;
mod test_code;
use names::*;
pub(crate) use naming::canonical_language;
use naming::*;
use stats::*;

/// Detail of a file → file `contains` edge: a Rust `mod` declaration.
pub(super) const MODULE_DETAIL: &str = "module";
#[derive(Default)]
struct Interner {
    ids: HashMap<String, u32>,
    strings: Vec<String>,
}

impl Interner {
    fn id(&mut self, value: &str) -> u32 {
        if let Some(id) = self.ids.get(value) {
            return *id;
        }
        let id = self.strings.len() as u32;
        self.strings.push(value.to_owned());
        self.ids.insert(value.to_owned(), id);
        id
    }
}

pub(crate) struct Projection {
    pub tables: GraphTables,
    pub calls: CallStats,
}

struct FileIndex<'a> {
    /// Declaration id → symbol node.
    decl_nodes: HashMap<&'a str, u32>,
    /// Declaration name → symbol nodes (every declaration in the file).
    by_name: HashMap<&'a str, Vec<u32>>,
    /// Public (importable) name → exported symbol nodes.
    exports: HashMap<&'a str, Vec<u32>>,
}

pub(crate) fn project(built: &BuiltGraph, workspace: &Workspace) -> Projection {
    let mut projector = Projector::new(built, workspace);
    let symbols = projector.symbols();
    let (externals, package_nodes) = projector.externals();
    projector.contains_edges();
    let (unparsed_links, relinked) = projector.unparsed_links();
    projector.import_edges(&externals, &package_nodes);
    let star_targets = star_targets(built);
    let resolver = ExportResolver {
        built,
        indexes: &symbols.indexes,
        star_targets: &star_targets,
        memo: std::cell::RefCell::new(HashMap::new()),
    };
    projector.use_edges(&resolver);
    let calls = projector.call_edges(&resolver, &symbols);
    for (src, dst, line) in unparsed_links {
        projector.push(
            src,
            dst,
            EdgeKind::Imports,
            Confidence::Medium,
            "unparsed-target",
            line,
        );
    }
    projector.finish(&relinked, calls)
}

/// Symbol lookups the edge passes share.
#[derive(Default)]
struct Symbols<'a> {
    indexes: BTreeMap<&'a str, FileIndex<'a>>,
    global_names: HashMap<&'a str, Vec<u32>>,
    /// [`CALL_TARGET`]/[`TYPE_TARGET`] bits by node id (none past the end).
    targets: Vec<u8>,
    /// Symbol node → name of its containing declaration (`Box` for `Box.open`).
    container_names: HashMap<u32, &'a str>,
}

/// A symbol a call can target ([`NON_CALLABLE`] kinds cannot).
const CALL_TARGET: u8 = 1;
/// A symbol a base-type clause can target.
const TYPE_TARGET: u8 = 2;

fn target_bits(kind: &str) -> u8 {
    let call = if NON_CALLABLE.contains(&kind) {
        0
    } else {
        CALL_TARGET
    };
    let ty = if TYPE_DECLARATION_KINDS.contains(&kind) {
        TYPE_TARGET
    } else {
        0
    };
    call | ty
}

/// `(src, dst, line)` links to unparsed files, and the `(file, line)`
/// import sites they relink.
type UnparsedLinks<'a> = (Vec<(u32, u32, u32)>, BTreeSet<(&'a str, u32)>);

/// An unlinked external import, classified against its component manifest.
struct External<'f> {
    file: &'f str,
    name: String,
    declared: &'static str,
    line: u32,
}

/// Projection state: the interned strings, nodes and edges built so far.
struct Projector<'a> {
    built: &'a BuiltGraph,
    workspace: &'a Workspace,
    strings: Interner,
    nodes: Vec<NodeRec>,
    edges: Vec<EdgeRec>,
    file_nodes: BTreeMap<&'a str, u32>,
    roles: HashMap<&'a str, u8>,
    languages: HashMap<&'a str, String>,
    /// Files the scan found but did not parse (over the size bound): they
    /// stay in the graph, so imports of them resolve and `find` sees them.
    unparsed: BTreeSet<&'a str>,
}

/// Unique qualified-name keys for `ordered` declarations of `file`.
fn symbol_keys(
    file: &str,
    ordered: &[&Declaration],
    by_id: &HashMap<&str, &Declaration>,
) -> Vec<String> {
    let qualified = |decl: &Declaration| {
        let mut parts = vec![decl.name.as_str()];
        let mut seen = BTreeSet::from([decl.id.as_str()]);
        let mut parent = decl.parent.as_deref();
        while let Some(id) = parent.filter(|id| seen.insert(id)) {
            let Some(parent_decl) = by_id.get(id) else {
                break;
            };
            parts.push(parent_decl.name.as_str());
            parent = parent_decl.parent.as_deref();
        }
        parts.reverse();
        parts.join(".")
    };
    let names = ordered
        .iter()
        .map(|decl| qualified(decl))
        .collect::<Vec<_>>();
    let mut repeats = HashMap::<&str, u32>::new();
    for name in &names {
        *repeats.entry(name.as_str()).or_default() += 1;
    }
    let mut used = BTreeSet::<String>::new();
    ordered
        .iter()
        .zip(&names)
        .map(|(decl, qualified_name)| {
            let mut key = if repeats[qualified_name.as_str()] > 1 {
                format!("{file}#{qualified_name}@{}", decl.line)
            } else {
                format!("{file}#{qualified_name}")
            };
            let mut n = 1;
            while !used.insert(key.clone()) {
                n += 1;
                key = format!("{file}#{qualified_name}@{}.{n}", decl.line);
            }
            key
        })
        .collect()
}

mod edges;
mod resolve;
use resolve::*;

#[cfg(test)]
mod tests;
