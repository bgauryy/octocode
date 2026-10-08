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
use crate::tools::ast_graph::types::{BuiltGraph, Declaration, FileFacts};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

/// How many re-export hops a call target may traverse.
const MAX_REEXPORT_DEPTH: usize = 8;
/// Shorter names are too generic for a graph-wide unique-name match.
const MIN_UNIQUE_NAME_LEN: usize = 3;
/// Declarations a call can never target (`format!` is not `mod format`,
/// `Err(..)` is not `type Err`).
const NON_CALLABLE: &[&str] = &[
    "module",
    "namespace",
    "type",
    "interface",
    "trait",
    "impl",
    "enum",
    "label",
];
/// Declarations an `extends`/`implements` clause can name.
const TYPE_KINDS: &[&str] = &["class", "interface", "struct", "trait", "type", "enum"];
/// Method names nearly every type defines (trait/protocol conventions): a
/// graph-wide unique match on `expr.name()` would be a coincidence.
const COMMON_METHODS: &[&str] = &[
    "clone",
    "iter",
    "iter_mut",
    "into_iter",
    "next",
    "fmt",
    "eq",
    "ne",
    "hash",
    "cmp",
    "partial_cmp",
    "default",
    "from",
    "into",
    "try_from",
    "try_into",
    "to_string",
    "to_owned",
    "as_ref",
    "as_mut",
    "deref",
    "drop",
    "len",
    "is_empty",
    "get",
    "get_mut",
    "insert",
    "remove",
    "push",
    "pop",
    "map",
    "and_then",
    "unwrap",
    "expect",
    "contains",
    "extend",
    "join",
    "split",
    "trim",
    "parse",
    "write",
    "read",
    "flush",
    "close",
    "send",
    "recv",
    "lock",
    "run",
    "call",
    "apply",
    "bind",
    "toString",
    "valueOf",
    "then",
    "catch",
    "finally",
    "forEach",
    "filter",
    "reduce",
    "find",
    "some",
    "every",
    "keys",
    "values",
    "entries",
    "has",
    "set",
    "add",
    "delete",
    "clear",
    "update",
    "render",
    "emit",
    "on",
    "off",
    "init",
    "start",
    "stop",
    "reset",
    "build",
    "create",
    "new",
    "open",
    "equals",
    "hashCode",
    "compareTo",
    "size",
    "__init__",
    "__str__",
    "__repr__",
    "__eq__",
    "__hash__",
    "__call__",
    "__enter__",
    "__exit__",
];

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

/// Call-linking tallies surfaced in the manifest.
#[derive(Clone, Debug, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CallStats {
    pub sites: u64,
    pub linked: u64,
    pub unresolved: u64,
    pub by_resolution: BTreeMap<String, u64>,
    /// Why call sites stayed unlinked; `internalRecall` counts only sites
    /// that could plausibly target code in this repository.
    pub unresolved_by_reason: BTreeMap<&'static str, u64>,
    /// Per language: `[linked, unlinked sites naming an in-repo declaration]`.
    #[serde(skip)]
    pub by_language: BTreeMap<String, [u64; 2]>,
}

/// Unlinked reasons that still name a declaration in this repository.
const INTERNAL_REASONS: &[&str] = &["ambiguous", "member-unknown-receiver-in-repo", "other"];

impl CallStats {
    /// Internal recall per language (see [`CallStats::internal_recall`]).
    pub(crate) fn recall_by_language(&self) -> BTreeMap<String, f64> {
        self.by_language
            .iter()
            .map(|(language, [linked, missed])| {
                let total = linked + missed;
                let recall = if total == 0 {
                    1.0
                } else {
                    *linked as f64 / total as f64
                };
                (language.clone(), (recall * 1000.0).round() / 1000.0)
            })
            .collect()
    }

    /// Linked / (linked + unlinked sites that name an in-repo declaration).
    pub(crate) fn internal_recall(&self) -> f64 {
        let internal = self.linked
            + INTERNAL_REASONS
                .iter()
                .map(|reason| self.unresolved_by_reason.get(reason).copied().unwrap_or(0))
                .sum::<u64>();
        if internal == 0 {
            1.0
        } else {
            self.linked as f64 / internal as f64
        }
    }
}

/// Classifies an unlinked call site for `CallStats::unresolved_by_reason`.
fn unlinked_reason(
    bindings: &FileBindings,
    qualifier: &str,
    name: &str,
    global_names: &HashMap<&str, Vec<u32>>,
    callable: &dyn Fn(u32) -> bool,
) -> &'static str {
    let receiver = matches!(qualifier, "this" | "self" | "Self" | "cls" | "super");
    let head = first_segment(qualifier);
    let in_repo = global_names
        .get(name)
        .map_or(0, |ids| ids.iter().filter(|id| callable(**id)).count());
    if (qualifier.is_empty() && bindings.external.contains(name))
        || (!qualifier.is_empty() && !receiver && bindings.external.contains(head))
    {
        "external-package"
    } else if in_repo == 0 {
        // std/builtins/macros/methods of foreign types: nothing to link to.
        "not-declared-in-repo"
    } else if !qualifier.is_empty()
        && !receiver
        && head.starts_with(|c: char| c.is_ascii_uppercase())
    {
        "type-qualified-unmatched"
    } else if !qualifier.is_empty() && !receiver {
        "member-unknown-receiver-in-repo"
    } else if in_repo > 1 {
        "ambiguous"
    } else {
        "other"
    }
}

pub(crate) struct Projection {
    pub tables: GraphTables,
    pub calls: CallStats,
}

/// Package identity for an external import specifier, by ecosystem rules.
fn package_name(specifier: &str, language: &str) -> Option<String> {
    let spec = specifier.trim();
    if spec.is_empty() || spec.starts_with('.') || spec.starts_with('/') {
        return None;
    }
    let name = match language {
        "rust" => spec.trim_start_matches("::").split("::").next()?.to_owned(),
        "python" => spec.split('.').next()?.to_owned(),
        "go" => {
            let parts = spec.split('/').collect::<Vec<_>>();
            if parts[0].contains('.') && parts.len() >= 3 {
                parts[..3].join("/")
            } else {
                spec.to_owned()
            }
        }
        "java" | "kotlin" | "scala" => match spec.rsplit_once('.') {
            Some((package, _)) => package.to_owned(),
            None => spec.to_owned(),
        },
        "c" | "cpp" | "cuda" => spec.trim_matches(['<', '>', '"']).to_owned(),
        _ => {
            if let Some(rest) = spec.strip_prefix('@') {
                let mut parts = rest.splitn(3, '/');
                match (parts.next(), parts.next()) {
                    (Some(scope), Some(name)) => format!("@{scope}/{name}"),
                    _ => spec.to_owned(),
                }
            } else {
                spec.split('/').next()?.to_owned()
            }
        }
    };
    (!name.is_empty()).then_some(name)
}

/// The unparsed file a relative specifier from `importer` names, probing
/// the usual source extensions and `index` files.
fn unparsed_target<'a>(
    importer: &str,
    spec: &str,
    unparsed: &BTreeSet<&'a str>,
) -> Option<&'a str> {
    if !spec.starts_with('.') {
        return None;
    }
    let mut parts = importer.split('/').collect::<Vec<_>>();
    parts.pop();
    for segment in spec.split('/') {
        match segment {
            "." | "" => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    let base = parts.join("/");
    const SUFFIXES: &[&str] = &[
        "",
        ".js",
        ".mjs",
        ".cjs",
        ".ts",
        ".tsx",
        ".jsx",
        "/index.js",
        "/index.ts",
    ];
    SUFFIXES.iter().find_map(|suffix| {
        let candidate = format!("{base}{suffix}");
        unparsed.get(candidate.as_str()).copied()
    })
}

/// One label per language regardless of which parser produced the facts
/// (oxc reports the extension, tree-sitter a language id).
pub(crate) fn canonical_language(file: &str, raw: &str) -> String {
    let ext = octocode_engine::text::extension_of(file, true, "");
    let known = match ext.as_str() {
        "ts" | "mts" | "cts" => "typescript",
        "tsx" => "tsx",
        "js" | "mjs" | "cjs" | "jsx" => "javascript",
        "py" | "pyi" => "python",
        "go" => "go",
        "rs" => "rust",
        "java" => "java",
        "kt" | "kts" => "kotlin",
        "c" | "h" => "c",
        "cpp" | "hpp" | "cc" | "cxx" | "hh" | "hxx" => "cpp",
        "cu" | "cuh" => "cuda",
        "scala" | "sc" | "sbt" => "scala",
        "cs" => "csharp",
        "asm" | "s" | "assembly" => "asm",
        _ => return raw.to_owned(),
    };
    known.to_owned()
}

fn ecosystem(language: &str) -> &'static str {
    match language {
        "javascript" | "typescript" | "tsx" | "jsx" => "npm",
        "csharp" => "nuget",
        "python" => "pypi",
        "rust" => "cargo",
        "go" => "go",
        "java" | "kotlin" | "scala" => "maven",
        "c" | "cpp" | "cuda" => "system",
        _ => "other",
    }
}

/// Line ranges of Rust test modules (`mod tests`, `mod proptests`, …) (unit tests living
/// inside production files; their imports are test-scoped).
fn test_module_ranges(facts: &FileFacts, language: &str) -> Vec<(u32, u32)> {
    if language != "rust" {
        return Vec::new();
    }
    facts
        .declarations
        .iter()
        .filter(|d| d.kind == "module" && d.name.contains("test"))
        .map(|d| (d.line, d.end_line))
        .collect()
}

/// `(qualifier, name)` of a callee: `this.run` → `("this", "run")`,
/// `Self::new` → `("Self", "new")`, `run` → `("", "run")`.
fn split_callee(callee: &str) -> (&str, &str) {
    match callee.rfind(['.', ':']) {
        Some(at) => {
            let name = &callee[at + 1..];
            let qualifier = callee[..at].trim_end_matches([':', '.']);
            (qualifier, name)
        }
        None => ("", callee),
    }
}

fn first_segment(qualifier: &str) -> &str {
    qualifier
        .split(['.', ':'])
        .find(|segment| !segment.is_empty())
        .unwrap_or(qualifier)
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
    /// Symbol node → declaration kind (for callable/type target filters).
    symbol_kinds: HashMap<u32, &'a str>,
    /// Symbol node → name of its containing declaration (`Box` for `Box.open`).
    container_names: HashMap<u32, &'a str>,
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

impl<'a> Projector<'a> {
    /// File nodes, sorted by path.
    fn new(built: &'a BuiltGraph, workspace: &'a Workspace) -> Self {
        let languages = built
            .nodes
            .keys()
            .map(String::as_str)
            .chain(
                built
                    .diagnostics
                    .iter()
                    .filter(|d| d.code == "scan-skip")
                    .map(|d| d.file.as_str()),
            )
            .map(|file| {
                let raw = built.facts.get(file).map_or("", |f| f.language.as_str());
                (file, canonical_language(file, raw))
            })
            .collect::<HashMap<_, _>>();
        let unparsed = built
            .diagnostics
            .iter()
            .filter(|d| d.code == "scan-skip" && !built.nodes.contains_key(&d.file))
            .map(|d| d.file.as_str())
            .collect::<BTreeSet<_>>();
        let mut projector = Self {
            built,
            workspace,
            strings: Interner::default(),
            nodes: Vec::new(),
            edges: Vec::new(),
            file_nodes: BTreeMap::new(),
            roles: HashMap::new(),
            languages,
            unparsed,
        };
        let mut all_files = built.nodes.keys().map(String::as_str).collect::<Vec<_>>();
        all_files.extend(projector.unparsed.iter().copied());
        all_files.sort_unstable();
        all_files.dedup();
        for file in all_files {
            projector.push_file(file);
        }
        projector
    }

    fn push_file(&mut self, file: &'a str) {
        let id = self.nodes.len() as u32;
        let name = file.rsplit('/').next().unwrap_or(file);
        let mut flags = file_roles(&self.built.root, file);
        if self.workspace.entries.contains_key(file) {
            flags |= ROLE_ENTRY;
        }
        if self.unparsed.contains(file) {
            flags |= ROLE_UNPARSED;
        }
        self.roles.insert(file, flags);
        let language = self.lang(file).to_owned();
        self.nodes.push(NodeRec {
            kind: NodeKind::File,
            flags,
            key: self.strings.id(file),
            name: self.strings.id(name),
            detail: self.strings.id(&language),
            file: id,
            parent: NONE,
            line: NONE,
            end_line: NONE,
        });
        self.file_nodes.insert(file, id);
    }

    fn lang(&self, file: &str) -> &str {
        self.languages.get(file).map_or("", String::as_str)
    }

    fn has_role(&self, file: &str, mask: u8) -> bool {
        self.roles.get(file).is_some_and(|r| r & mask != 0)
    }

    fn push(
        &mut self,
        src: u32,
        dst: u32,
        kind: EdgeKind,
        confidence: Confidence,
        detail: &str,
        line: u32,
    ) {
        let detail = self.strings.id(detail);
        self.edges.push(EdgeRec {
            src,
            dst,
            kind,
            confidence,
            detail,
            line,
        });
    }

    /// Symbols, per file in source order; keys are `path#Qualified.name`
    /// with an `@line` suffix only when the qualified name repeats.
    fn symbols(&mut self) -> Symbols<'a> {
        let mut symbols = Symbols::default();
        let built = self.built;
        for (file, facts) in &built.facts {
            let Some(&file_id) = self.file_nodes.get(file.as_str()) else {
                continue;
            };
            // Minified bundles declare thousands of mangled names: keep the
            // file and its imports, drop the symbols.
            if self.has_role(file, ROLE_BUNDLED) {
                continue;
            }
            let index = self.file_symbols(file, facts, file_id, &mut symbols);
            symbols.indexes.insert(file.as_str(), index);
        }
        symbols
    }

    fn file_symbols(
        &mut self,
        file: &'a str,
        facts: &'a FileFacts,
        file_id: u32,
        symbols: &mut Symbols<'a>,
    ) -> FileIndex<'a> {
        let by_id = facts
            .declarations
            .iter()
            .map(|decl| (decl.id.as_str(), decl))
            .collect::<HashMap<_, _>>();
        let mut ordered = facts.declarations.iter().collect::<Vec<_>>();
        ordered.sort_by(|a, b| a.line.cmp(&b.line).then_with(|| a.id.cmp(&b.id)));
        let keys = symbol_keys(file, &ordered, &by_id);
        let file_is_test = self.has_role(file, ROLE_TEST);
        let test_scopes = test_module_ranges(facts, self.lang(file));
        let python = self.lang(file) == "python";
        let is_test_decl = |decl: &Declaration| {
            file_is_test
                || test_scopes
                    .iter()
                    .any(|(start, end)| *start <= decl.line && decl.line <= *end)
                || (python
                    && ((matches!(decl.kind.as_str(), "function" | "method")
                        && decl.name.starts_with("test_"))
                        || (decl.kind == "class" && decl.name.starts_with("Test"))))
        };
        // Generated, bundled, vendored and declaration-only code never
        // serves as a unique-name call target.
        let unique_target = !self.has_role(file, ROLE_NOT_AUTHORED | ROLE_DECLARATION);
        let mut index = FileIndex {
            decl_nodes: HashMap::new(),
            by_name: HashMap::new(),
            exports: HashMap::new(),
        };
        for (decl, key) in ordered.iter().zip(&keys) {
            let id = self.nodes.len() as u32;
            let flags = if decl.exported { FLAG_EXPORTED } else { 0 }
                | if is_test_decl(decl) { FLAG_TEST } else { 0 }
                | if facts.reference_counts.get(&decl.id).is_some_and(|n| *n > 0) {
                    FLAG_LOCAL_USE
                } else {
                    0
                };
            self.nodes.push(NodeRec {
                kind: NodeKind::Symbol,
                flags,
                key: self.strings.id(key),
                name: self.strings.id(&decl.name),
                detail: self.strings.id(&decl.kind),
                file: file_id,
                parent: NONE,
                line: decl.line,
                end_line: decl.end_line,
            });
            index.decl_nodes.insert(decl.id.as_str(), id);
            symbols.symbol_kinds.insert(id, decl.kind.as_str());
            index
                .by_name
                .entry(decl.name.as_str())
                .or_default()
                .push(id);
            if decl.exported {
                for public in decl.public_names() {
                    index.exports.entry(public.as_str()).or_default().push(id);
                }
            }
            if unique_target {
                symbols
                    .global_names
                    .entry(decl.name.as_str())
                    .or_default()
                    .push(id);
            }
        }
        for decl in &facts.declarations {
            if let (Some(parent), Some(&child)) =
                (&decl.parent, index.decl_nodes.get(decl.id.as_str()))
                && let Some(&parent_id) = index.decl_nodes.get(parent.as_str())
                && parent_id != child
            {
                self.nodes[child as usize].parent = parent_id;
                if let Some(parent_decl) = by_id.get(parent.as_str()) {
                    symbols
                        .container_names
                        .insert(child, parent_decl.name.as_str());
                }
            }
        }
        index
    }

    /// External packages. Each unlinked external import is classified
    /// against its component manifest once; a Cargo root no manifest
    /// declares is a local module (Cargo would not build it otherwise), so
    /// it gets no node.
    fn externals(&mut self) -> (Vec<External<'a>>, BTreeMap<String, u32>) {
        let mut externals = Vec::<External>::new();
        let mut packages = BTreeMap::<String, &'static str>::new();
        let built = self.built;
        for (file, facts) in &built.facts {
            let language = self.lang(file);
            let eco = ecosystem(language);
            let test_scopes = test_module_ranges(facts, language);
            for import in facts
                .imports
                .iter()
                .filter(|i| i.external && i.target.is_none())
            {
                let name = if eco == "go" {
                    self.workspace
                        .go_module(&import.specifier)
                        .map(str::to_owned)
                        .or_else(|| package_name(&import.specifier, language))
                } else {
                    package_name(&import.specifier, language)
                };
                let Some(name) = name else { continue };
                let mut declared = self.workspace.declared(file, &name, eco);
                if eco == "cargo" && declared == Declared::Unknown {
                    continue;
                }
                if declared != Declared::Builtin
                    && test_scopes
                        .iter()
                        .any(|(start, end)| (*start..=*end).contains(&import.line))
                {
                    declared = Declared::TestScoped;
                }
                packages.entry(name.clone()).or_insert(eco);
                externals.push(External {
                    file: file.as_str(),
                    name,
                    declared: declared.as_str(),
                    line: import.line,
                });
            }
        }
        let mut package_nodes = BTreeMap::<String, u32>::new();
        for (name, eco) in packages {
            let id = self.nodes.len() as u32;
            self.nodes.push(NodeRec {
                kind: NodeKind::Package,
                flags: 0,
                key: self.strings.id(&format!("pkg:{name}")),
                name: self.strings.id(&name),
                detail: self.strings.id(eco),
                file: NONE,
                parent: NONE,
                line: NONE,
                end_line: NONE,
            });
            package_nodes.insert(name, id);
        }
        (externals, package_nodes)
    }

    /// contains: file → top-level symbol, symbol → member.
    fn contains_edges(&mut self) {
        let owned = self
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| node.kind == NodeKind::Symbol)
            .map(|(id, node)| {
                let owner = if node.parent == NONE {
                    node.file
                } else {
                    node.parent
                };
                (owner, id as u32, node.line)
            })
            .collect::<Vec<_>>();
        for (owner, id, line) in owned {
            self.push(
                owner,
                id,
                EdgeKind::Contains,
                Confidence::High,
                "declares",
                line,
            );
        }
    }

    /// Relative imports that land on an unparsed file: linked instead of
    /// reported as a resolution failure.
    fn unparsed_links(&self) -> UnparsedLinks<'a> {
        let mut links = Vec::new();
        let mut relinked = BTreeSet::new();
        if self.unparsed.is_empty() {
            return (links, relinked);
        }
        for (file, facts) in &self.built.facts {
            let Some(&src) = self.file_nodes.get(file.as_str()) else {
                continue;
            };
            for import in facts
                .imports
                .iter()
                .filter(|i| i.target.is_none() && !i.external)
            {
                if let Some(target) = unparsed_target(file, &import.specifier, &self.unparsed)
                    && let Some(&dst) = self.file_nodes.get(target)
                {
                    links.push((src, dst, import.line));
                    relinked.insert((file.as_str(), import.line));
                }
            }
        }
        (links, relinked)
    }

    /// imports: linked file edges (one per relation) and external packages.
    fn import_edges(&mut self, externals: &[External], package_nodes: &BTreeMap<String, u32>) {
        let function_ranges = self
            .built
            .facts
            .iter()
            .map(|(file, facts)| {
                let ranges = facts
                    .declarations
                    .iter()
                    .filter(|d| matches!(d.kind.as_str(), "function" | "method"))
                    .map(|d| (d.line, d.end_line))
                    .collect::<Vec<_>>();
                (file.as_str(), ranges)
            })
            .collect::<HashMap<_, _>>();
        let mut externals_by_file = HashMap::<&str, Vec<&External>>::new();
        for external in externals {
            externals_by_file
                .entry(external.file)
                .or_default()
                .push(external);
        }
        let built = self.built;
        for (file, node) in &built.nodes {
            let Some(&src) = self.file_nodes.get(file.as_str()) else {
                continue;
            };
            let ranges = function_ranges
                .get(file.as_str())
                .map_or(&[][..], Vec::as_slice);
            self.file_import_edges(file, node, src, ranges);
            let mut seen = BTreeMap::<u32, (u32, &'static str)>::new();
            for external in externals_by_file.get(file.as_str()).into_iter().flatten() {
                if let Some(&dst) = package_nodes.get(external.name.as_str()) {
                    let entry = seen
                        .entry(dst)
                        .or_insert((external.line, external.declared));
                    entry.0 = entry.0.min(external.line);
                    // A non-test import outranks a test-scoped one.
                    if entry.1 == "external-test" {
                        entry.1 = external.declared;
                    }
                }
            }
            for (dst, (line, declared)) in seen {
                self.push(
                    src,
                    dst,
                    EdgeKind::Imports,
                    Confidence::High,
                    declared,
                    line,
                );
            }
        }
    }

    fn file_import_edges(
        &mut self,
        file: &str,
        node: &crate::tools::ast_graph::types::Node,
        src: u32,
        function_ranges: &[(u32, u32)],
    ) {
        let built = self.built;
        let facts = built.facts.get(file);
        // Import lines per linked target, in one pass (a Go import links
        // every file of its package, so per-target rescans are quadratic).
        let mut lines_by_target = HashMap::<&str, Vec<u32>>::new();
        for import in facts.into_iter().flat_map(|facts| &facts.imports) {
            if let Some(target) = &import.target {
                lines_by_target
                    .entry(target.as_str())
                    .or_default()
                    .push(import.line);
            }
        }
        let python = self.lang(file) == "python";
        for (target, kinds) in &node.edges {
            let Some(&dst) = self.file_nodes.get(target.as_str()) else {
                continue;
            };
            let lines = lines_by_target
                .get(target.as_str())
                .map_or(&[][..], Vec::as_slice);
            let line = lines.iter().copied().min().unwrap_or(NONE);
            // Python imports inside function bodies run lazily, on call:
            // they cannot form a module-load cycle.
            let lazy = python
                && !lines.is_empty()
                && lines
                    .iter()
                    .all(|line| function_ranges.iter().any(|(s, e)| s < line && line <= e));
            for kind in kinds {
                // `mod child;` declares a submodule: containment, not a
                // dependency (it would fold every module tree into a cycle).
                let (edge_kind, detail) = if kind == "rust-module" {
                    (EdgeKind::Contains, "module")
                } else if lazy {
                    (EdgeKind::Imports, "lazy-import")
                } else {
                    (EdgeKind::Imports, kind.as_str())
                };
                self.push(src, dst, edge_kind, Confidence::High, detail, line);
            }
        }
    }

    /// uses: named imports resolved (through re-exports) to the exported
    /// declaration; namespace imports mark the whole target file as used.
    fn use_edges(&mut self, resolver: &ExportResolver) {
        let built = self.built;
        for (file, facts) in &built.facts {
            let Some(&src) = self.file_nodes.get(file.as_str()) else {
                continue;
            };
            for import in &facts.imports {
                let Some(target) = &import.target else {
                    continue;
                };
                let name = import.imported_name.as_str();
                if name.is_empty() {
                    continue;
                }
                if name == "*" {
                    if let Some(&dst) = self.file_nodes.get(target.as_str()) {
                        self.push(
                            src,
                            dst,
                            EdgeKind::Uses,
                            Confidence::High,
                            "namespace",
                            import.line,
                        );
                    }
                    continue;
                }
                let (found, hopped) = resolver.exported(target, name);
                let confidence = if hopped {
                    Confidence::Medium
                } else {
                    Confidence::High
                };
                for dst in found {
                    self.push(
                        src,
                        dst,
                        EdgeKind::Uses,
                        confidence,
                        if hopped { "reexport" } else { "import" },
                        import.line,
                    );
                }
            }
        }
    }

    /// calls and inherits: syntax call sites and base-type clauses linked to
    /// declarations.
    fn call_edges(&mut self, resolver: &ExportResolver, symbols: &Symbols) -> CallStats {
        let file_paths = self
            .file_nodes
            .iter()
            .map(|(path, id)| (*id, *path))
            .collect::<HashMap<_, _>>();
        let scope = link_scope(&file_paths, self.workspace);
        let linker = Linker {
            resolver,
            nodes: &self.nodes,
            global_names: &symbols.global_names,
            container_names: &symbols.container_names,
            scope: &scope,
        };
        let callable = |id: u32| {
            symbols
                .symbol_kinds
                .get(&id)
                .is_some_and(|kind| !NON_CALLABLE.contains(kind))
        };
        let is_type = |id: u32| {
            symbols
                .symbol_kinds
                .get(&id)
                .is_some_and(|kind| TYPE_KINDS.contains(kind))
        };
        let mut calls = CallStats::default();
        let mut linked = Vec::<PendingEdge>::new();
        for (file, facts) in &self.built.facts {
            // Minified bundles re-declare everything under short names;
            // linking their call sites would only add noise.
            if self.has_role(file, ROLE_BUNDLED) {
                continue;
            }
            let (Some(index), Some(&file_id)) = (
                symbols.indexes.get(file.as_str()),
                self.file_nodes.get(file.as_str()),
            ) else {
                continue;
            };
            let language = self.lang(file);
            let unit = FileLinks {
                bindings: FileBindings::new(facts, language),
                index,
                file_id,
                language,
            };
            link_calls(&linker, &unit, facts, &callable, &mut calls, &mut linked);
            link_heritage(&linker, &unit, facts, &is_type, &mut linked);
        }
        for (src, dst, kind, confidence, detail, line) in linked {
            self.push(src, dst, kind, confidence, &detail, line);
        }
        calls
    }

    /// One edge per (src, kind, dst, detail): the earliest line wins, so a
    /// callee invoked ten times from one caller is one edge.
    fn finish(mut self, relinked: &BTreeSet<(&str, u32)>, calls: CallStats) -> Projection {
        self.edges.sort_by(|a, b| {
            (a.src, a.kind, a.dst, a.detail, a.line, a.confidence).cmp(&(
                b.src,
                b.kind,
                b.dst,
                b.detail,
                b.line,
                b.confidence,
            ))
        });
        self.edges.dedup_by(|b, a| {
            a.src == b.src && a.kind == b.kind && a.dst == b.dst && a.detail == b.detail
        });
        let built = self.built;
        let mut diagnostics = built
            .diagnostics
            .iter()
            .filter(|diag| {
                !(diag.code == "unresolved-internal"
                    && diag
                        .line
                        .is_some_and(|line| relinked.contains(&(diag.file.as_str(), line))))
            })
            .map(|diag| DiagRec {
                file: self.strings.id(&diag.file),
                line: diag.line.unwrap_or(NONE),
                code: self.strings.id(&diag.code),
                message: self.strings.id(&diag.message),
            })
            .collect::<Vec<_>>();
        diagnostics.dedup();
        let digests = self
            .file_nodes
            .iter()
            .filter_map(|(file, id)| {
                let digest = &built.facts.get(*file)?.digest;
                (!digest.is_empty()).then(|| (*id, self.strings.id(digest)))
            })
            .collect();
        let stamps = self
            .file_nodes
            .iter()
            .filter_map(|(file, id)| {
                let facts = built.facts.get(*file)?;
                let stamp = facts.stamp.filter(|_| !facts.digest.is_empty())?;
                Some((*id, stamp))
            })
            .collect();
        let components = self
            .file_nodes
            .iter()
            .filter_map(|(file, id)| {
                let language = self.languages.get(file).map_or("", String::as_str);
                let component = self.workspace.component_of(file, ecosystem(language))?;
                let dir = if component.dir.is_empty() {
                    "."
                } else {
                    component.dir.as_str()
                };
                Some((
                    *id,
                    self.strings.id(dir),
                    self.strings.id(&component.name),
                    self.strings.id(&component.meta()),
                ))
            })
            .collect();
        let entries = self
            .workspace
            .entries
            .iter()
            .filter_map(|(file, rule)| {
                Some((*self.file_nodes.get(file.as_str())?, self.strings.id(rule)))
            })
            .collect();
        let mut tables = GraphTables {
            components,
            entries,
            strings: self.strings.strings,
            nodes: self.nodes,
            edges: self.edges,
            diagnostics,
            digests,
            stamps,
            ..Default::default()
        };
        tables.index();
        Projection { tables, calls }
    }
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

/// Files each file star-re-exports.
fn star_targets(built: &BuiltGraph) -> HashMap<&str, Vec<&str>> {
    built
        .nodes
        .iter()
        .map(|(file, node)| {
            let targets = node
                .edges
                .iter()
                .filter(|(_, kinds)| kinds.iter().any(|kind| kind.ends_with("star-reexport")))
                .map(|(target, _)| target.as_str())
                .collect::<Vec<_>>();
            (file.as_str(), targets)
        })
        .collect()
}

/// Rust module and workspace-crate lookups for qualified call paths.
fn link_scope<'a>(file_paths: &'a HashMap<u32, &'a str>, workspace: &Workspace) -> LinkScope<'a> {
    let mut modules = HashMap::<&str, Vec<&str>>::new();
    for path in file_paths.values() {
        let Some(stem) = path.strip_suffix(".rs") else {
            continue;
        };
        let module = match stem.rsplit_once('/') {
            Some((dir, "mod")) => dir.rsplit('/').next().unwrap_or(dir),
            Some((_, name)) => name,
            None => stem,
        };
        modules.entry(module).or_default().push(*path);
    }
    let crate_dirs = workspace
        .components
        .values()
        .flatten()
        .filter(|c| c.ecosystem == "cargo")
        .map(|c| (c.name.replace('-', "_"), c.dir.clone()))
        .collect::<HashMap<_, _>>();
    LinkScope {
        file_paths,
        modules,
        crate_dirs,
    }
}

/// An edge found while the node list is borrowed, pushed afterwards.
type PendingEdge = (u32, u32, EdgeKind, Confidence, String, u32);

/// One file's call-linking context.
struct FileLinks<'f> {
    bindings: FileBindings<'f>,
    index: &'f FileIndex<'f>,
    file_id: u32,
    language: &'f str,
}

/// calls: each call site linked to its target declarations.
fn link_calls(
    linker: &Linker,
    unit: &FileLinks,
    facts: &FileFacts,
    callable: &dyn Fn(u32) -> bool,
    calls: &mut CallStats,
    out: &mut Vec<PendingEdge>,
) {
    for call in &facts.calls {
        calls.sites += 1;
        let caller = call
            .caller_id
            .as_deref()
            .and_then(|id| unit.index.decl_nodes.get(id).copied())
            .unwrap_or(unit.file_id);
        let (qualifier, name) = split_callee(&call.callee);
        let site = Site {
            bindings: &unit.bindings,
            index: unit.index,
            caller,
            qualifier,
            name,
            accept: callable,
            receiver_type: call.receiver_type.as_deref(),
        };
        let Some((targets, resolution, confidence)) = linker.link(&site) else {
            calls.unresolved += 1;
            let reason = unlinked_reason(
                &unit.bindings,
                qualifier,
                name,
                linker.global_names,
                callable,
            );
            *calls.unresolved_by_reason.entry(reason).or_default() += 1;
            if INTERNAL_REASONS.contains(&reason) {
                calls
                    .by_language
                    .entry(unit.language.to_owned())
                    .or_default()[1] += 1;
            }
            continue;
        };
        calls.linked += 1;
        calls
            .by_language
            .entry(unit.language.to_owned())
            .or_default()[0] += 1;
        *calls
            .by_resolution
            .entry(resolution.to_owned())
            .or_default() += 1;
        // JSX renders, bare decorators and `new` keep their kind visible:
        // `renders:import`, `constructs:local`.
        let detail = match call.kind.as_str() {
            "" | "calls" | "call" => resolution.to_owned(),
            kind => format!("{kind}:{resolution}"),
        };
        for dst in targets {
            out.push((
                caller,
                dst,
                EdgeKind::Calls,
                confidence,
                detail.clone(),
                call.line,
            ));
        }
    }
}

/// inherits: `extends` / `implements` base types, linked like calls.
fn link_heritage(
    linker: &Linker,
    unit: &FileLinks,
    facts: &FileFacts,
    is_type: &dyn Fn(u32) -> bool,
    out: &mut Vec<PendingEdge>,
) {
    for heritage in &facts.heritage {
        let Some(&src) = unit.index.decl_nodes.get(heritage.decl_id.as_str()) else {
            continue;
        };
        let (qualifier, name) = split_callee(&heritage.target);
        let site = Site {
            bindings: &unit.bindings,
            index: unit.index,
            caller: src,
            qualifier,
            name,
            accept: is_type,
            receiver_type: None,
        };
        let Some((targets, resolution, confidence)) = linker.link(&site) else {
            continue;
        };
        let detail = format!("{}:{resolution}", heritage.relation);
        for dst in targets.into_iter().filter(|dst| *dst != src) {
            out.push((
                src,
                dst,
                EdgeKind::Inherits,
                confidence,
                detail.clone(),
                heritage.line,
            ));
        }
    }
}

/// Declarations a name resolves to, and whether a re-export hop was taken.
type Resolved = (Vec<u32>, bool);

struct ExportResolver<'a> {
    built: &'a BuiltGraph,
    indexes: &'a BTreeMap<&'a str, FileIndex<'a>>,
    star_targets: &'a HashMap<&'a str, Vec<&'a str>>,
    /// `(file, name)` → resolution; call sites repeat the same imports.
    memo: std::cell::RefCell<HashMap<(String, String), Resolved>>,
}

impl ExportResolver<'_> {
    fn exported(&self, file: &str, name: &str) -> (Vec<u32>, bool) {
        let key = (file.to_owned(), name.to_owned());
        if let Some(hit) = self.memo.borrow().get(&key) {
            return hit.clone();
        }
        let result = self.exported_uncached(file, name);
        self.memo.borrow_mut().insert(key, result.clone());
        result
    }

    /// Declarations `file` exports under `name`, following named and star
    /// re-exports up to [`MAX_REEXPORT_DEPTH`] hops.
    fn exported_uncached(&self, file: &str, name: &str) -> (Vec<u32>, bool) {
        let mut pending = vec![(file.to_owned(), name.to_owned(), 0usize)];
        let mut seen = BTreeSet::new();
        let mut found = Vec::new();
        let mut hopped = false;
        while let Some((file, name, depth)) = pending.pop() {
            if depth > MAX_REEXPORT_DEPTH || !seen.insert((file.clone(), name.clone())) {
                continue;
            }
            if let Some(ids) = self
                .indexes
                .get(file.as_str())
                .and_then(|index| index.exports.get(name.as_str()))
            {
                found.extend(ids);
                hopped |= depth > 0;
                continue;
            }
            if let Some(facts) = self.built.facts.get(&file) {
                for reexport in facts.reexports.iter().filter(|r| r.local_name == name) {
                    if let Some(target) = &reexport.target {
                        pending.push((target.clone(), reexport.imported_name.clone(), depth + 1));
                    }
                }
            }
            if name != "default" {
                for target in self.star_targets.get(file.as_str()).into_iter().flatten() {
                    pending.push(((*target).to_owned(), name.clone(), depth + 1));
                }
            }
        }
        found.sort_unstable();
        found.dedup();
        (found, hopped)
    }

    /// Any declaration named `name` in `file` (module-qualified calls such as
    /// Go `pkg.Func` or Rust `module::func` need not be re-exported).
    fn declared(&self, file: &str, name: &str) -> Vec<u32> {
        self.indexes
            .get(file)
            .and_then(|index| index.by_name.get(name))
            .cloned()
            .unwrap_or_default()
    }
}

fn confidence_for(targets: &[u32], strong: Confidence) -> Confidence {
    if targets.len() == 1 {
        strong
    } else {
        Confidence::Medium.max(strong)
    }
}

/// Per-file import bindings, built once so each call site is O(1).
struct FileBindings<'a> {
    /// Local name → `(target file, name exported there)` for linked imports.
    named: HashMap<&'a str, Vec<(&'a str, &'a str)>>,
    /// Namespace/module binding (`ns` in `ns.f`, `pkg` in `pkg.F`) → files.
    modules: HashMap<&'a str, Vec<&'a str>>,
    /// Heads bound to third-party or standard packages.
    external: HashSet<String>,
    /// Files this file imports (scope for ambiguous names).
    imported_files: HashSet<&'a str>,
}

impl<'a> FileBindings<'a> {
    fn new(facts: &'a FileFacts, language: &str) -> Self {
        let mut bindings = Self {
            named: HashMap::new(),
            modules: HashMap::new(),
            external: ["std", "core", "alloc"]
                .iter()
                .map(|s| (*s).to_owned())
                .collect(),
            imported_files: facts
                .imports
                .iter()
                .filter_map(|import| import.target.as_deref())
                .collect(),
        };
        for import in &facts.imports {
            let module_binding = import
                .local_name
                .as_deref()
                .filter(|local| !local.is_empty())
                .or_else(|| {
                    import
                        .specifier
                        .rsplit(['/', '.', ':'])
                        .find(|s| !s.is_empty())
                });
            match &import.target {
                Some(target) => {
                    let local = import
                        .local_name
                        .as_deref()
                        .unwrap_or(&import.imported_name);
                    let exported = if import.imported_name.is_empty() {
                        local
                    } else {
                        import.imported_name.as_str()
                    };
                    if !local.is_empty() {
                        bindings
                            .named
                            .entry(local)
                            .or_default()
                            .push((target.as_str(), exported));
                    }
                    if let Some(binding) = module_binding {
                        bindings
                            .modules
                            .entry(binding)
                            .or_default()
                            .push(target.as_str());
                    }
                }
                None if import.external => {
                    if let Some(binding) = module_binding {
                        bindings.external.insert(binding.to_owned());
                    }
                    if let Some(local) = import.local_name.as_deref().filter(|l| !l.is_empty()) {
                        bindings.external.insert(local.to_owned());
                    }
                    if !import.imported_name.is_empty() && import.imported_name != "*" {
                        bindings.external.insert(import.imported_name.clone());
                    }
                    bindings
                        .external
                        .extend(package_name(&import.specifier, language));
                }
                None => {}
            }
        }
        bindings
    }
}

/// A linked call: target symbols, how they were found, and the confidence.
type Linked = (Vec<u32>, &'static str, Confidence);

/// Graph-wide lookups a call or base-type link needs.
struct Linker<'r> {
    resolver: &'r ExportResolver<'r>,
    nodes: &'r [NodeRec],
    global_names: &'r HashMap<&'r str, Vec<u32>>,
    container_names: &'r HashMap<u32, &'r str>,
    scope: &'r LinkScope<'r>,
}

/// One call site (or base-type clause) to link.
struct Site<'s> {
    bindings: &'s FileBindings<'s>,
    index: &'s FileIndex<'s>,
    caller: u32,
    qualifier: &'s str,
    name: &'s str,
    accept: &'s dyn Fn(u32) -> bool,
    receiver_type: Option<&'s str>,
}

impl Site<'_> {
    /// `this`/`self`/`super` style receiver.
    fn receiver(&self) -> bool {
        matches!(self.qualifier, "this" | "self" | "Self" | "cls" | "super")
    }
    /// The parser saw the receiver's type (`this.a.m()` may be recorded as a
    /// bare `m`): such a call is a member call, never a local/import match.
    fn typed(&self) -> bool {
        self.receiver_type.is_some() && !self.receiver()
    }
    fn accepted(&self, ids: impl IntoIterator<Item = u32>) -> Vec<u32> {
        ids.into_iter().filter(|id| (self.accept)(*id)).collect()
    }
}

impl Linker<'_> {
    fn link(&self, site: &Site) -> Option<Linked> {
        if site.name.is_empty() {
            return None;
        }
        if let Some(linked) = self.local(site) {
            return Some(linked);
        }
        let head = first_segment(site.qualifier);
        if let Some(linked) = self.scoped(site, head) {
            return Some(linked);
        }
        // An external binding (`import pad from 'left-pad'`, `use std::fmt`,
        // `serde_json::from_value`) is that package's, never ours.
        let external = &site.bindings.external;
        if (site.qualifier.is_empty() && external.contains(site.name))
            || (!site.qualifier.is_empty() && !site.receiver() && external.contains(head))
        {
            return None;
        }
        self.global(site, head)
    }

    /// A declaration of the caller's own file; receiver calls prefer members
    /// of the caller's own container.
    fn local(&self, site: &Site) -> Option<Linked> {
        let receiver = site.receiver();
        if site.typed() || !(site.qualifier.is_empty() || receiver) {
            return None;
        }
        let local = site.accepted(site.index.by_name.get(site.name)?.iter().copied());
        if local.is_empty() {
            return None;
        }
        let container = self
            .nodes
            .get(site.caller as usize)
            .map_or(NONE, |node| node.parent);
        let siblings = local
            .iter()
            .copied()
            .filter(|id| {
                receiver && container != NONE && self.nodes[*id as usize].parent == container
            })
            .collect::<Vec<_>>();
        let targets = if siblings.is_empty() { local } else { siblings };
        let confidence = confidence_for(&targets, Confidence::High);
        Some((targets, "local", confidence))
    }

    /// A name the file imports, a Rust module path, or an import binding's
    /// namespace.
    fn scoped(&self, site: &Site, head: &str) -> Option<Linked> {
        let receiver = site.receiver();
        if site.qualifier.is_empty() && !site.typed() {
            return self.imported(site);
        }
        if !receiver
            && site.qualifier.contains("::")
            && let Some(module_file) = self
                .scope
                .file_paths
                .get(&self.file_of(site.caller))
                .and_then(|caller_path| self.scope.rust_module_file(site.qualifier, caller_path))
        {
            // `crate::a::b::f()`, `super::b::f()`, `other_crate::b::f()`.
            let targets = site.accepted(self.resolver.declared(module_file, site.name));
            let confidence = confidence_for(&targets, Confidence::High);
            return (!targets.is_empty()).then_some((targets, "module-path", confidence));
        }
        if !receiver && let Some(files) = site.bindings.modules.get(head) {
            return self.namespace(site, files);
        }
        None
    }

    fn imported(&self, site: &Site) -> Option<Linked> {
        let sources = site.bindings.named.get(site.name)?;
        let mut targets = Vec::new();
        let mut hopped = false;
        for (target, exported) in sources {
            let (found, via) = self.resolver.exported(target, exported);
            hopped |= via;
            targets.extend(site.accepted(found));
        }
        targets.sort_unstable();
        targets.dedup();
        let strong = if hopped {
            Confidence::Medium
        } else {
            Confidence::High
        };
        let confidence = confidence_for(&targets, strong);
        (!targets.is_empty()).then_some((targets, "import", confidence))
    }

    /// `ns.fn()`, `pkg.Func()`, `module::func()`: the qualifier's first
    /// segment names an import binding whose target files declare `name`.
    fn namespace(&self, site: &Site, files: &[&str]) -> Option<Linked> {
        let mut targets = Vec::new();
        for target in files {
            let (exported, _) = self.resolver.exported(target, site.name);
            if exported.is_empty() {
                targets.extend(site.accepted(self.resolver.declared(target, site.name)));
            } else {
                targets.extend(site.accepted(exported));
            }
        }
        targets.sort_unstable();
        targets.dedup();
        let confidence = confidence_for(&targets, Confidence::Medium);
        (!targets.is_empty()).then_some((targets, "namespace", confidence))
    }

    /// Last resort: a graph-wide name match, scoped by package and imports.
    /// A type-qualified call (`Type::new`, `Type.of`, including the last
    /// segment of a Rust path such as `crate::store::Store::new`) must land
    /// in that type.
    fn global(&self, site: &Site, head: &str) -> Option<Linked> {
        let (qualifier, name) = (site.qualifier, site.name);
        let receiver = site.receiver();
        let typed = site.typed();
        let path_type = qualifier.rsplit("::").next().filter(|last| {
            qualifier.contains("::") && last.starts_with(|c: char| c.is_ascii_uppercase())
        });
        let head = path_type.unwrap_or(head);
        let type_qualified = !receiver && head.starts_with(|c: char| c.is_ascii_uppercase());
        // `expr.name()` has an unknown receiver type: a graph-wide name match
        // is a coincidence (`.all()`, `.replace()`, `.kind()` on std/foreign
        // types), so a member call links only through a receiver type the
        // parser saw declared or constructed locally (`let s: Store`,
        // `s = Store::new()`).
        let member_call = typed || (!qualifier.is_empty() && !receiver && !type_qualified);
        let type_head = if member_call {
            match site
                .receiver_type
                .map(|ty| ty.rsplit([':', '.']).next().unwrap_or(ty))
            {
                Some(ty) if !ty.is_empty() => ty,
                _ => return None,
            }
        } else {
            head
        };
        // A bare call that only matches a method elsewhere is a method call
        // whose receiver the extractor dropped; skip names every type
        // implements.
        let bare_method_call =
            |id: u32| self.nodes[id as usize].parent != NONE && COMMON_METHODS.contains(&name);
        let candidates = self
            .global_names
            .get(name)
            .map(|ids| {
                ids.iter()
                    .copied()
                    .filter(|id| (site.accept)(*id) && (member_call || !bare_method_call(*id)))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if name.len() < MIN_UNIQUE_NAME_LEN || candidates.is_empty() {
            return None;
        }
        if type_qualified || member_call {
            let via = if member_call {
                "receiver-type"
            } else {
                "type-qualified"
            };
            return self.member(site, &candidates, type_head, via);
        }
        let caller_file = self.file_of(site.caller);
        if let Some((id, via)) = self.pick(site, &candidates)
            && self.file_of(id) != caller_file
        {
            return Some((vec![id], via, Confidence::Medium));
        }
        // Exactly one declaration graph-wide carries this name.
        if let [only] = candidates.as_slice()
            && self.file_of(*only) != caller_file
        {
            return Some((vec![*only], "unique-name", Confidence::Low));
        }
        None
    }

    /// `Type::method` / `Type.method`, or `x.method()` with `x: Type`: a
    /// member of a container named `Type`.
    fn member(
        &self,
        site: &Site,
        candidates: &[u32],
        type_head: &str,
        via: &'static str,
    ) -> Option<Linked> {
        let members = candidates
            .iter()
            .copied()
            .filter(|id| self.container_names.get(id) == Some(&type_head))
            .collect::<Vec<_>>();
        let id = match members.as_slice() {
            [] => return None,
            [only] => *only,
            many => self.pick(site, many)?.0,
        };
        let confidence = if self.near(site, id) {
            Confidence::High
        } else {
            Confidence::Medium
        };
        Some((vec![id], via, confidence))
    }

    /// Scope preference: the caller's package (directory), then files the
    /// caller imports; a choice is made only when it is unique.
    fn pick(&self, site: &Site, pool: &[u32]) -> Option<(u32, &'static str)> {
        let caller_dir = self.dir(self.file_of(site.caller));
        let same_package = pool
            .iter()
            .copied()
            .filter(|id| self.dir(self.file_of(*id)) == caller_dir)
            .collect::<Vec<_>>();
        if let [only] = same_package.as_slice() {
            return Some((*only, "same-package"));
        }
        let imported = pool
            .iter()
            .copied()
            .filter(|id| self.imported_by(site, *id))
            .collect::<Vec<_>>();
        if let [only] = imported.as_slice() {
            return Some((*only, "import-scope"));
        }
        None
    }

    /// The target sits in the caller's package or in a file it imports.
    fn near(&self, site: &Site, id: u32) -> bool {
        self.dir(self.file_of(id)) == self.dir(self.file_of(site.caller))
            || self.imported_by(site, id)
    }

    fn imported_by(&self, site: &Site, id: u32) -> bool {
        self.scope
            .file_paths
            .get(&self.file_of(id))
            .is_some_and(|path| site.bindings.imported_files.contains(path))
    }

    fn file_of(&self, id: u32) -> u32 {
        self.nodes.get(id as usize).map_or(NONE, |n| n.file)
    }

    fn dir(&self, file: u32) -> &str {
        self.scope
            .file_paths
            .get(&file)
            .map_or("", |path| path.rsplit_once('/').map_or("", |(dir, _)| dir))
    }
}

/// Per-graph lookups [`Linker`] needs for scope-aware choices.
struct LinkScope<'a> {
    /// File node → root-relative path.
    file_paths: &'a HashMap<u32, &'a str>,
    /// Rust module name → files defining it (`m.rs`, `m/mod.rs`).
    modules: HashMap<&'a str, Vec<&'a str>>,
    /// Workspace crate import name (`octocode_engine`) → crate directory.
    crate_dirs: HashMap<String, String>,
}

impl LinkScope<'_> {
    /// The file a Rust module path (`crate::a::b`, `super::b`,
    /// `other_crate::b`) names, by its last segment, scoped to the caller's
    /// crate (or the named workspace crate) and settled by the longest shared
    /// directory prefix when several files share the module name.
    fn rust_module_file(&self, qualifier: &str, caller_file: &str) -> Option<&str> {
        let segments = qualifier
            .split("::")
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();
        let (first, module) = (*segments.first()?, *segments.last()?);
        if segments.len() < 2 || module.starts_with(|c: char| c.is_ascii_uppercase()) {
            return None;
        }
        let scope_dir = match first {
            "crate" | "self" | "super" => None,
            name => Some(self.crate_dirs.get(name)?.as_str()),
        };
        let shared = |path: &str| {
            path.split('/')
                .zip(caller_file.split('/'))
                .take_while(|(a, b)| a == b)
                .count()
        };
        let candidates = self
            .modules
            .get(module)?
            .iter()
            .copied()
            .filter(|path| {
                scope_dir.is_none_or(|dir| dir.is_empty() || path.starts_with(&format!("{dir}/")))
            })
            .collect::<Vec<_>>();
        let best = candidates.iter().map(|p| shared(p)).max()?;
        let mut top = candidates.iter().filter(|p| shared(p) == best);
        let first_match = *top.next()?;
        top.next().is_none().then_some(first_match)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_names_follow_ecosystem_rules() {
        assert_eq!(
            package_name("@scope/pkg/sub", "typescript").as_deref(),
            Some("@scope/pkg")
        );
        assert_eq!(
            package_name("react-dom/client", "javascript").as_deref(),
            Some("react-dom")
        );
        assert_eq!(
            package_name("serde::Deserialize", "rust").as_deref(),
            Some("serde")
        );
        assert_eq!(package_name("os.path", "python").as_deref(), Some("os"));
        assert_eq!(
            package_name("github.com/a/b/c", "go").as_deref(),
            Some("github.com/a/b")
        );
        assert_eq!(package_name("fmt", "go").as_deref(), Some("fmt"));
        assert_eq!(
            package_name("java.util.List", "java").as_deref(),
            Some("java.util")
        );
        assert_eq!(package_name("./local", "typescript"), None);
    }

    #[test]
    fn callee_split_handles_member_and_path_calls() {
        assert_eq!(split_callee("run"), ("", "run"));
        assert_eq!(split_callee("this.run"), ("this", "run"));
        assert_eq!(split_callee("Self::new"), ("Self", "new"));
        assert_eq!(split_callee("a.b.c"), ("a.b", "c"));
        assert_eq!(first_segment("utils::inner"), "utils");
    }
}
