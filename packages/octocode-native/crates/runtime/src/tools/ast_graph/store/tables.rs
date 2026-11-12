//! Projects a linked [`BuiltGraph`] into persisted [`GraphTables`]: file,
//! symbol, and package nodes joined by `contains`, `imports`, and `calls`.
//!
//! Call edges are syntactic candidates. Each one records how it was linked
//! (`local`, `import`, `namespace`, `unique-name`) and a confidence, so a
//! consumer never mistakes a name match for proven symbol identity.
use super::super::classify::{
    ROLE_CONFIG, ROLE_DECLARATION, ROLE_ENTRY, ROLE_NOT_AUTHORED, ROLE_TEST, ROLE_UNPARSED, Roles,
    file_roles,
};
use super::format::{
    Confidence, DeclarationRec, DiagRec, EdgeKind, EdgeRec, FLAG_EXPORTED, FLAG_LOCAL_USE,
    FLAG_TEST, GraphTables, NONE, NodeKind, NodeRec, ProvenanceRec, UnresolvedRec, WitnessRec,
};
use super::workspace::{Declared, Workspace};
use crate::tools::ast_graph::edges::Load;
use crate::tools::ast_graph::types::{
    BuiltGraph, Declaration, DiagnosticCode, FileFacts, ImportOutcome,
};
use octocode_engine::signatures::TYPE_DECLARATION_KINDS;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

mod argument_bindings;
mod arguments;
mod assemble;
mod names;
mod naming;
mod projector;
mod sources;
mod stats;
mod test_code;
use crate::tools::ast_graph::languages::canonical_language;
use names::*;
use naming::*;
use stats::*;

/// Detail of a file → file `contains` edge: a Rust `mod` declaration.
pub(super) const MODULE_DETAIL: &str = "module";
/// Detail of a `uses` edge from a file to a declaration it re-exports by
/// name (`export { a } from './x'`), or to a whole file when the name
/// resolves to no declaration (`export * as ns from './x'`). It records
/// what the file makes public; it is no use of the declaration.
pub(super) const REEXPORTED_NAME: &str = "reexported-name";
/// Interned strings, each stored once: a lookup walks the ids that share the
/// string's hash, newest first, and compares the stored text.
#[derive(Default)]
struct Interner {
    hasher: std::collections::hash_map::RandomState,
    newest: HashMap<u64, u32>,
    /// By id: the previous id with the same hash, else [`NONE`].
    older: Vec<u32>,
    strings: Vec<String>,
}

impl Interner {
    fn id(&mut self, value: &str) -> u32 {
        use std::hash::BuildHasher;
        self.id_hashed(self.hasher.hash_one(value), value)
    }

    fn id_hashed(&mut self, hash: u64, value: &str) -> u32 {
        let newest = self.newest.get(&hash).copied().unwrap_or(NONE);
        let mut at = newest;
        while at != NONE {
            if self.strings[at as usize] == value {
                return at;
            }
            at = self.older[at as usize];
        }
        // Ids are `u32` columns: a table past its `u32` ids saturates here and the publication preflight
        // (`validate_counts`) refuses it, so no wrapped id is ever published.
        let id = octocode_engine::num::saturating_u32(self.strings.len());
        self.strings.push(value.to_owned());
        self.older.push(newest);
        self.newest.insert(hash, id);
        id
    }
}

pub(crate) struct Projection {
    pub tables: GraphTables,
    pub calls: CallStats,
    /// Import tallies by [`ImportOutcome`] after projection: the build's
    /// tally with every relinked import moved from unresolved to resolved.
    pub imports: [u32; 6],
}

struct FileIndex<'a> {
    /// Declaration id → symbol node.
    decl_nodes: HashMap<&'a str, u32>,
    /// Declaration name → symbol nodes (every declaration in the file).
    by_name: HashMap<&'a str, Vec<u32>>,
    /// Public (importable) name → exported symbol nodes.
    exports: HashMap<&'a str, Vec<u32>>,
}

/// Projects `built` into snapshot tables. Each file's captured source moves
/// into the tables' source table ([`sources::Sources::capture`]), so a build
/// is projected once.
pub(crate) fn project(
    built: &mut BuiltGraph,
    workspace: &Workspace,
    cancel: &dyn crate::execution::cancel::CancellationCheck,
) -> Result<Projection, crate::response::result::ToolError> {
    let sources = sources::Sources::capture(built, cancel)?;
    let built = &*built;
    let mut projector = Projector::new(built, workspace, sources)?;
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
    Ok(projector.finish(&relinked, calls))
}

/// Symbol lookups the edge passes share.
#[derive(Default)]
struct Symbols<'a> {
    indexes: BTreeMap<&'a str, FileIndex<'a>>,
    global_names: HashMap<&'a str, Vec<u32>>,
    /// Call/type eligibility and method classification by node id (none past the end).
    targets: Vec<u8>,
    /// Symbol node → name of its containing declaration (`Box` for `Box.open`).
    container_names: HashMap<u32, &'a str>,
    /// C-family prototype → the one definition it declares (see
    /// `c_definitions`): a call bound to the prototype reaches it.
    definitions: HashMap<u32, u32>,
}

/// A symbol a call can target ([`NON_CALLABLE`] kinds cannot).
const CALL_TARGET: u8 = 1;
/// A symbol a base-type clause can target.
const TYPE_TARGET: u8 = 2;
/// A member declared in a type body, rather than a free or nested function.
const METHOD_TARGET: u8 = 4;
const REFERENCE_TARGET: u8 = 8;
const LEXICAL_SCOPE: u8 = 16;
/// A declaration whose own extent was recovered, or a variable holding a
/// `require`d module: never a call target, but it still occupies its name,
/// so a same-named decoy cannot win a scope choice by its absence.
const RECOVERED_TARGET: u8 = 32;
/// Data a type declares ([`is_member_data`]): reached through a receiver or
/// a type, never by a bare name.
const MEMBER_DATA: u8 = 64;
/// An enum member: constructed (`Attempt::Done(1)`), never called.
const ENUM_MEMBER: u8 = 128;

/// A field, property or enum member, or a constant or variable declared in
/// a type's body: data of that type, not a callable of its scope.
fn is_member_data(decl: &Declaration, by_id: &HashMap<&str, &Declaration>) -> bool {
    match decl.kind.as_str() {
        "property" | "enumMember" => true,
        "constant" | "variable" => decl
            .parent
            .as_deref()
            .and_then(|parent| by_id.get(parent))
            .is_some_and(|parent| {
                TYPE_DECLARATION_KINDS.contains(&parent.kind.as_str()) || parent.kind == "impl"
            }),
        _ => false,
    }
}

fn target_bits(kind: &str, member_data: bool) -> u8 {
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
    let method = if kind == "method" { METHOD_TARGET } else { 0 };
    let lexical = if matches!(kind, "function" | "method" | "module" | "namespace") {
        LEXICAL_SCOPE
    } else {
        0
    };
    let data = if member_data { MEMBER_DATA } else { 0 };
    let variant = if kind == "enumMember" { ENUM_MEMBER } else { 0 };
    call | ty | method | REFERENCE_TARGET | lexical | data | variant
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
    sources: sources::Sources,
    built: &'a BuiltGraph,
    workspace: &'a Workspace,
    strings: Interner,
    nodes: Vec<NodeRec>,
    edges: Vec<EdgeRec>,
    witnesses: Vec<WitnessRec>,
    unresolved: Vec<UnresolvedRec>,
    declarations: Vec<DeclarationRec>,
    file_nodes: BTreeMap<&'a str, u32>,
    roles: HashMap<&'a str, Roles>,
    languages: HashMap<&'a str, String>,
    /// Files the scan found but did not parse (over the size bound): they
    /// stay in the graph, so imports of them resolve and `find` sees them.
    unparsed: BTreeSet<&'a str>,
    /// The shared test-code rule over the scanned root, for file roles.
    test_code: crate::tools::test_code::TestCodeIndex,
}

#[cfg(test)]
thread_local! {
    /// Tests: the suffixes [`symbol_keys`] tried past a taken key.
    pub(super) static KEY_PROBES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Unique qualified-name keys for `ordered` declarations of `file`.
fn symbol_keys(
    file: &str,
    ordered: &[&Declaration],
    by_id: &HashMap<&str, &Declaration>,
) -> Vec<String> {
    let qualified = |decl: &Declaration| {
        let mut parts = vec![decl.name.as_str()];
        // A Go method is `Type.Method`, as gopls names it; a C++ out-of-line
        // definition `void Widget::draw() {}` is `Widget.draw` inside its
        // enclosing namespaces.
        if let Some(owner) = decl.owner.as_deref() {
            parts.extend(owner.rsplit("::").filter(|segment| !segment.is_empty()));
        }
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
    // Repeats of each name, all and other than data members: a field beside
    // its same-named method takes the line suffix, so the method keeps the
    // key it has without the field.
    let mut repeats = HashMap::<&str, (u32, u32)>::new();
    for (decl, name) in ordered.iter().zip(&names) {
        let repeat = repeats.entry(name.as_str()).or_default();
        repeat.0 += 1;
        repeat.1 += u32::from(!is_member_data(decl, by_id));
    }
    let mut used = BTreeSet::<String>::new();
    // The last suffix each repeated name and line took: every suffix below
    // it is taken (keys are never released), so the next repeat resumes
    // there instead of trying `.2` again. The keys are the ones a probe from
    // `.2` would give, in linear work.
    let mut last = HashMap::<(&str, u32), u32>::new();
    ordered
        .iter()
        .zip(&names)
        .map(|(decl, qualified_name)| {
            let (all, others) = repeats[qualified_name.as_str()];
            let repeated = if is_member_data(decl, by_id) {
                all > 1
            } else {
                others > 1
            };
            let mut key = if repeated {
                format!("{file}#{qualified_name}@{}", decl.line)
            } else {
                format!("{file}#{qualified_name}")
            };
            if !used.insert(key.clone()) {
                let n = last
                    .entry((qualified_name.as_str(), decl.line))
                    .or_insert(1);
                loop {
                    #[cfg(test)]
                    KEY_PROBES.with(|probes| probes.set(probes.get() + 1));
                    *n += 1;
                    key = format!("{file}#{qualified_name}@{}.{n}", decl.line);
                    if used.insert(key.clone()) {
                        break;
                    }
                }
            }
            key
        })
        .collect()
}

mod c_definitions;
mod edges;
mod resolve;
use resolve::*;

#[cfg(test)]
mod tests;
