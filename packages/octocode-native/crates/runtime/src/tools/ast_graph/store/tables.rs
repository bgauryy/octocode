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

/// Detail of a file → file `contains` edge: a Rust `mod` declaration.
pub(super) const MODULE_DETAIL: &str = "module";
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
        "java" | "scala" => match spec.rsplit_once('.') {
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
/// (oxc reports the extension, tree-sitter a language id): the selector of
/// the file's grammar in the engine registry
/// ([`octocode_engine::signatures::languages::label`]), except that the TSX
/// grammar keeps its name (`tsx`); `raw` for an extension no grammar claims.
pub(crate) fn canonical_language(file: &str, raw: &str) -> String {
    use octocode_engine::signatures::languages;
    let extension = octocode_engine::text::extension_of(file, true, "");
    match languages::find_entry(&extension) {
        Some(entry) if entry.name == "TSX" => "tsx".to_owned(),
        Some(entry) => languages::label(entry),
        None => raw.to_owned(),
    }
}

fn ecosystem(language: &str) -> &'static str {
    match language {
        "javascript" | "typescript" | "tsx" => "npm",
        "csharp" => "nuget",
        "python" => "pypi",
        "rust" => "cargo",
        "go" => "go",
        "java" | "scala" => "maven",
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
                    .filter(|d| d.code == DiagnosticCode::ScanSkip)
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
            .filter(|d| d.code == DiagnosticCode::ScanSkip && !built.nodes.contains_key(&d.file))
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
        let marks = self.built.facts.get(file).map(|facts| facts.marks);
        let mut flags = file_roles(&self.built.root, file, marks);
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
            symbols.targets.resize(id as usize + 1, 0);
            symbols.targets[id as usize] = target_bits(&decl.kind);
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
        // Between two nodes, the earliest evidence comes first: a walk that
        // shows one edge per neighbor cites the file's first reference.
        self.edges
            .sort_by_key(|e| (e.src, e.kind, e.dst, e.line, e.detail, e.confidence));
        let built = self.built;
        let mut diagnostics = built
            .diagnostics
            .iter()
            .filter(|diag| {
                !(diag.code == DiagnosticCode::UnresolvedInternal
                    && diag
                        .line
                        .is_some_and(|line| relinked.contains(&(diag.file.as_str(), line))))
            })
            .map(|diag| DiagRec {
                file: self.strings.id(&diag.file),
                line: diag.line.unwrap_or(NONE),
                code: self.strings.id(diag.code.as_str()),
                message: self.strings.id(&diag.message),
                target: diag
                    .target
                    .as_deref()
                    .and_then(|target| self.file_nodes.get(target).copied())
                    .unwrap_or(NONE),
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

mod edges;
mod resolve;
use resolve::*;

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
