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
    let ext = file
        .rsplit_once('.')
        .map_or("", |(_, ext)| ext)
        .to_ascii_lowercase();
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
    let mut strings = Interner::default();
    let mut nodes = Vec::<NodeRec>::new();
    let empty = FileFacts::default();

    // Files, sorted by path.
    let mut file_nodes = BTreeMap::<&str, u32>::new();
    let mut roles = HashMap::<&str, u8>::new();
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
    let lang = |file: &str| languages.get(file).map_or("", String::as_str);
    // Files the scan found but did not parse (over the size bound) stay in
    // the graph, so imports of them resolve and `find` sees them.
    let unparsed = built
        .diagnostics
        .iter()
        .filter(|d| d.code == "scan-skip" && !built.nodes.contains_key(&d.file))
        .map(|d| d.file.as_str())
        .collect::<BTreeSet<_>>();
    let mut all_files = built.nodes.keys().map(String::as_str).collect::<Vec<_>>();
    all_files.extend(unparsed.iter().copied());
    all_files.sort_unstable();
    all_files.dedup();
    for file in all_files {
        let id = nodes.len() as u32;
        let name = file.rsplit('/').next().unwrap_or(file);
        let mut flags = file_roles(&built.root, file);
        if workspace.entries.contains_key(file) {
            flags |= ROLE_ENTRY;
        }
        if unparsed.contains(file) {
            flags |= ROLE_UNPARSED;
        }
        roles.insert(file, flags);
        nodes.push(NodeRec {
            kind: NodeKind::File,
            flags,
            key: strings.id(file),
            name: strings.id(name),
            detail: strings.id(lang(file)),
            file: id,
            parent: NONE,
            line: NONE,
            end_line: NONE,
        });
        file_nodes.insert(file, id);
    }

    // Symbols, per file in source order; keys are `path#Qualified.name`
    // with an `@line` suffix only when the qualified name repeats.
    let mut indexes = BTreeMap::<&str, FileIndex>::new();
    let mut global_names = HashMap::<&str, Vec<u32>>::new();
    // Symbol node → declaration kind (for callable/type target filters).
    let mut symbol_kinds = HashMap::<u32, &str>::new();
    // Symbol node → name of its containing declaration (`Box` for `Box.open`).
    let mut container_names = HashMap::<u32, &str>::new();
    for (file, facts) in &built.facts {
        let Some(&file_id) = file_nodes.get(file.as_str()) else {
            continue;
        };
        // Minified bundles declare thousands of mangled names: keep the file
        // and its imports, drop the symbols.
        if roles
            .get(file.as_str())
            .is_some_and(|r| r & ROLE_BUNDLED != 0)
        {
            continue;
        }
        let by_id = facts
            .declarations
            .iter()
            .map(|decl| (decl.id.as_str(), decl))
            .collect::<HashMap<_, _>>();
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
        let mut ordered = facts.declarations.iter().collect::<Vec<_>>();
        ordered.sort_by(|a, b| a.line.cmp(&b.line).then_with(|| a.id.cmp(&b.id)));
        let names = ordered
            .iter()
            .map(|decl| qualified(decl))
            .collect::<Vec<_>>();
        let mut repeats = HashMap::<&str, u32>::new();
        for name in &names {
            *repeats.entry(name.as_str()).or_default() += 1;
        }
        let mut used = BTreeSet::<String>::new();
        let mut index = FileIndex {
            decl_nodes: HashMap::new(),
            by_name: HashMap::new(),
            exports: HashMap::new(),
        };
        let file_is_test = roles.get(file.as_str()).is_some_and(|r| r & ROLE_TEST != 0);
        let test_scopes = test_module_ranges(facts, lang(file));
        let is_test_decl = |decl: &Declaration| {
            file_is_test
                || test_scopes
                    .iter()
                    .any(|(start, end)| *start <= decl.line && decl.line <= *end)
                || (lang(file) == "python"
                    && ((decl.kind == "function" && decl.name.starts_with("test_"))
                        || (decl.kind == "class" && decl.name.starts_with("Test"))))
        };
        for (decl, qualified_name) in ordered.iter().zip(&names) {
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
            let id = nodes.len() as u32;
            nodes.push(NodeRec {
                kind: NodeKind::Symbol,
                flags: if decl.exported { FLAG_EXPORTED } else { 0 }
                    | if is_test_decl(decl) { FLAG_TEST } else { 0 }
                    | if facts.reference_counts.get(&decl.id).is_some_and(|n| *n > 0) {
                        FLAG_LOCAL_USE
                    } else {
                        0
                    },
                key: strings.id(&key),
                name: strings.id(&decl.name),
                detail: strings.id(&decl.kind),
                file: file_id,
                parent: NONE,
                line: decl.line,
                end_line: decl.end_line,
            });
            index.decl_nodes.insert(decl.id.as_str(), id);
            symbol_kinds.insert(id, decl.kind.as_str());
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
            // Generated, bundled, vendored and declaration-only code never
            // serves as a unique-name call target.
            if roles
                .get(file.as_str())
                .is_none_or(|r| r & (ROLE_NOT_AUTHORED | ROLE_DECLARATION) == 0)
            {
                global_names.entry(decl.name.as_str()).or_default().push(id);
            }
        }
        for decl in &facts.declarations {
            if let (Some(parent), Some(&child)) =
                (&decl.parent, index.decl_nodes.get(decl.id.as_str()))
                && let Some(&parent_id) = index.decl_nodes.get(parent.as_str())
                && parent_id != child
            {
                nodes[child as usize].parent = parent_id;
                if let Some(parent_decl) = by_id.get(parent.as_str()) {
                    container_names.insert(child, parent_decl.name.as_str());
                }
            }
        }
        indexes.insert(file.as_str(), index);
    }

    // External packages. Each unlinked external import is classified against
    // its component manifest once; a Cargo root no manifest declares is a
    // local module (Cargo would not build it otherwise), so it gets no node.
    struct External<'f> {
        file: &'f str,
        name: String,
        declared: &'static str,
        line: u32,
    }
    let mut externals = Vec::<External>::new();
    let mut packages = BTreeMap::<String, &'static str>::new();
    for (file, facts) in &built.facts {
        let language = lang(file);
        let eco = ecosystem(language);
        let test_scopes = test_module_ranges(facts, language);
        for import in facts
            .imports
            .iter()
            .filter(|i| i.external && i.target.is_none())
        {
            let name = if eco == "go" {
                workspace
                    .go_module(&import.specifier)
                    .map(str::to_owned)
                    .or_else(|| package_name(&import.specifier, language))
            } else {
                package_name(&import.specifier, language)
            };
            let Some(name) = name else { continue };
            let mut declared = workspace.declared(file, &name, eco);
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
    let mut package_nodes = BTreeMap::<&str, u32>::new();
    for (name, eco) in &packages {
        let id = nodes.len() as u32;
        nodes.push(NodeRec {
            kind: NodeKind::Package,
            flags: 0,
            key: strings.id(&format!("pkg:{name}")),
            name: strings.id(name),
            detail: strings.id(eco),
            file: NONE,
            parent: NONE,
            line: NONE,
            end_line: NONE,
        });
        package_nodes.insert(name.as_str(), id);
    }

    let mut edges = Vec::<EdgeRec>::new();
    let push = |edges: &mut Vec<EdgeRec>,
                strings: &mut Interner,
                src: u32,
                dst: u32,
                kind: EdgeKind,
                confidence: Confidence,
                detail: &str,
                line: u32| {
        edges.push(EdgeRec {
            src,
            dst,
            kind,
            confidence,
            detail: strings.id(detail),
            line,
        })
    };

    // contains: file → top-level symbol, symbol → member.
    for (id, node) in nodes.iter().enumerate() {
        if node.kind == NodeKind::Symbol {
            let owner = if node.parent == NONE {
                node.file
            } else {
                node.parent
            };
            push(
                &mut edges,
                &mut strings,
                owner,
                id as u32,
                EdgeKind::Contains,
                Confidence::High,
                "declares",
                node.line,
            );
        }
    }

    let function_ranges = built
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

    // Relative imports that land on an unparsed file: link them instead of
    // reporting a resolution failure.
    let mut unparsed_links = Vec::<(u32, u32, u32)>::new();
    let mut relinked = BTreeSet::<(&str, u32)>::new();
    if !unparsed.is_empty() {
        for (file, facts) in &built.facts {
            let Some(&src) = file_nodes.get(file.as_str()) else {
                continue;
            };
            for import in facts
                .imports
                .iter()
                .filter(|i| i.target.is_none() && !i.external)
            {
                if let Some(target) = unparsed_target(file, &import.specifier, &unparsed)
                    && let Some(&dst) = file_nodes.get(target)
                {
                    unparsed_links.push((src, dst, import.line));
                    relinked.insert((file.as_str(), import.line));
                }
            }
        }
    }
    let mut externals_by_file = HashMap::<&str, Vec<&External>>::new();
    for external in &externals {
        externals_by_file
            .entry(external.file)
            .or_default()
            .push(external);
    }

    // imports: linked file edges (one per relation) and external packages.
    for (file, node) in &built.nodes {
        let Some(&src) = file_nodes.get(file.as_str()) else {
            continue;
        };
        let facts = built.facts.get(file).unwrap_or(&empty);
        // Import lines per linked target, in one pass (a Go import links
        // every file of its package, so per-target rescans are quadratic).
        let mut lines_by_target = HashMap::<&str, Vec<u32>>::new();
        for import in &facts.imports {
            if let Some(target) = &import.target {
                lines_by_target
                    .entry(target.as_str())
                    .or_default()
                    .push(import.line);
            }
        }
        for (target, kinds) in &node.edges {
            let Some(&dst) = file_nodes.get(target.as_str()) else {
                continue;
            };
            let lines = lines_by_target
                .get(target.as_str())
                .map_or(&[][..], Vec::as_slice);
            let line = lines.iter().copied().min().unwrap_or(NONE);
            // Python imports inside function bodies run lazily, on call:
            // they cannot form a module-load cycle.
            let lazy = lang(file) == "python"
                && !lines.is_empty()
                && lines.iter().all(|line| {
                    function_ranges
                        .get(file.as_str())
                        .is_some_and(|ranges| ranges.iter().any(|(s, e)| s < line && line <= e))
                });
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
                push(
                    &mut edges,
                    &mut strings,
                    src,
                    dst,
                    edge_kind,
                    Confidence::High,
                    detail,
                    line,
                );
            }
        }
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
            push(
                &mut edges,
                &mut strings,
                src,
                dst,
                EdgeKind::Imports,
                Confidence::High,
                declared,
                line,
            );
        }
    }

    // calls: syntax call sites linked to declarations.
    let star_targets = built
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
        .collect::<HashMap<_, _>>();
    let resolver = ExportResolver {
        built,
        indexes: &indexes,
        star_targets: &star_targets,
        memo: std::cell::RefCell::new(HashMap::new()),
    };
    // uses: named imports resolved (through re-exports) to the exported
    // declaration; namespace imports mark the whole target file as used.
    for (file, facts) in &built.facts {
        let Some(&src) = file_nodes.get(file.as_str()) else {
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
                if let Some(&dst) = file_nodes.get(target.as_str()) {
                    push(
                        &mut edges,
                        &mut strings,
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
                push(
                    &mut edges,
                    &mut strings,
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

    let file_paths = file_nodes
        .iter()
        .map(|(path, id)| (*id, *path))
        .collect::<HashMap<_, _>>();
    let mut rust_modules = HashMap::<&str, Vec<&str>>::new();
    for path in file_paths.values() {
        let Some(stem) = path.strip_suffix(".rs") else {
            continue;
        };
        let module = match stem.rsplit_once('/') {
            Some((dir, "mod")) => dir.rsplit('/').next().unwrap_or(dir),
            Some((_, name)) => name,
            None => stem,
        };
        rust_modules.entry(module).or_default().push(*path);
    }
    let crate_dirs = workspace
        .components
        .values()
        .flatten()
        .filter(|c| c.ecosystem == "cargo")
        .map(|c| (c.name.replace('-', "_"), c.dir.clone()))
        .collect::<HashMap<_, _>>();
    let link_scope = LinkScope {
        file_paths: &file_paths,
        modules: rust_modules,
        crate_dirs,
    };
    let callable = |id: u32| {
        symbol_kinds
            .get(&id)
            .is_some_and(|kind| !NON_CALLABLE.contains(kind))
    };
    let is_type = |id: u32| {
        symbol_kinds
            .get(&id)
            .is_some_and(|kind| TYPE_KINDS.contains(kind))
    };
    let mut calls = CallStats::default();
    for (file, facts) in &built.facts {
        // Minified bundles re-declare everything under short names; linking
        // their call sites would only add noise.
        if roles
            .get(file.as_str())
            .is_some_and(|r| r & ROLE_BUNDLED != 0)
        {
            continue;
        }
        let (Some(index), Some(&file_id)) =
            (indexes.get(file.as_str()), file_nodes.get(file.as_str()))
        else {
            continue;
        };
        let bindings = FileBindings::new(facts, lang(file));
        for call in &facts.calls {
            calls.sites += 1;
            let src = call
                .caller_id
                .as_deref()
                .and_then(|id| index.decl_nodes.get(id).copied())
                .unwrap_or(file_id);
            let (qualifier, name) = split_callee(&call.callee);
            let linked = link_call(
                &resolver,
                &bindings,
                index,
                &nodes,
                src,
                qualifier,
                name,
                &global_names,
                &container_names,
                &callable,
                &link_scope,
                call.receiver_type.as_deref(),
            );
            match linked {
                Some((targets, resolution, confidence)) => {
                    calls.linked += 1;
                    calls.by_language.entry(lang(file).to_owned()).or_default()[0] += 1;
                    *calls
                        .by_resolution
                        .entry(resolution.to_owned())
                        .or_default() += 1;
                    // JSX renders, bare decorators and `new` keep their kind
                    // visible: `renders:import`, `constructs:local`.
                    let detail = match call.kind.as_str() {
                        "" | "calls" | "call" => resolution.to_owned(),
                        kind => format!("{kind}:{resolution}"),
                    };
                    for dst in targets {
                        push(
                            &mut edges,
                            &mut strings,
                            src,
                            dst,
                            EdgeKind::Calls,
                            confidence,
                            &detail,
                            call.line,
                        );
                    }
                }
                None => {
                    calls.unresolved += 1;
                    let reason =
                        unlinked_reason(&bindings, qualifier, name, &global_names, &callable);
                    *calls.unresolved_by_reason.entry(reason).or_default() += 1;
                    if INTERNAL_REASONS.contains(&reason) {
                        calls.by_language.entry(lang(file).to_owned()).or_default()[1] += 1;
                    }
                }
            }
        }
        // inherits: `extends` / `implements` base types, linked like calls.
        for heritage in &facts.heritage {
            let Some(&src) = index.decl_nodes.get(heritage.decl_id.as_str()) else {
                continue;
            };
            let (qualifier, name) = split_callee(&heritage.target);
            if let Some((targets, resolution, confidence)) = link_call(
                &resolver,
                &bindings,
                index,
                &nodes,
                src,
                qualifier,
                name,
                &global_names,
                &container_names,
                &is_type,
                &link_scope,
                None,
            ) {
                let detail = format!("{}:{resolution}", heritage.relation);
                for dst in targets.into_iter().filter(|dst| *dst != src) {
                    push(
                        &mut edges,
                        &mut strings,
                        src,
                        dst,
                        EdgeKind::Inherits,
                        confidence,
                        &detail,
                        heritage.line,
                    );
                }
            }
        }
    }

    for (src, dst, line) in unparsed_links {
        push(
            &mut edges,
            &mut strings,
            src,
            dst,
            EdgeKind::Imports,
            Confidence::Medium,
            "unparsed-target",
            line,
        );
    }
    // One edge per (src, kind, dst, detail): the earliest line wins, so a
    // callee invoked ten times from one caller is one edge.
    edges.sort_by(|a, b| {
        (a.src, a.kind, a.dst, a.detail, a.line, a.confidence).cmp(&(
            b.src,
            b.kind,
            b.dst,
            b.detail,
            b.line,
            b.confidence,
        ))
    });
    edges.dedup_by(|b, a| {
        a.src == b.src && a.kind == b.kind && a.dst == b.dst && a.detail == b.detail
    });

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
            file: strings.id(&diag.file),
            line: diag.line.unwrap_or(NONE),
            code: strings.id(&diag.code),
            message: strings.id(&diag.message),
        })
        .collect::<Vec<_>>();
    diagnostics.dedup();
    let digests = file_nodes
        .iter()
        .filter_map(|(file, id)| {
            let digest = &built.facts.get(*file)?.digest;
            (!digest.is_empty()).then(|| (*id, strings.id(digest)))
        })
        .collect();

    let components = file_nodes
        .iter()
        .filter_map(|(file, id)| {
            let component = workspace.component_of(file, ecosystem(lang(file)))?;
            let dir = if component.dir.is_empty() {
                "."
            } else {
                component.dir.as_str()
            };
            Some((
                *id,
                strings.id(dir),
                strings.id(&component.name),
                strings.id(&component.meta()),
            ))
        })
        .collect();
    let entries = workspace
        .entries
        .iter()
        .filter_map(|(file, rule)| Some((*file_nodes.get(file.as_str())?, strings.id(rule))))
        .collect();
    let mut tables = GraphTables {
        components,
        entries,
        strings: strings.strings,
        nodes,
        edges,
        diagnostics,
        digests,
        ..Default::default()
    };
    tables.index();
    Projection { tables, calls }
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

#[allow(clippy::too_many_arguments)]
fn link_call(
    resolver: &ExportResolver,
    bindings: &FileBindings,
    index: &FileIndex,
    nodes: &[NodeRec],
    caller: u32,
    qualifier: &str,
    name: &str,
    global_names: &HashMap<&str, Vec<u32>>,
    container_names: &HashMap<u32, &str>,
    accept: &dyn Fn(u32) -> bool,
    scope: &LinkScope,
    receiver_type: Option<&str>,
) -> Option<(Vec<u32>, &'static str, Confidence)> {
    if name.is_empty() {
        return None;
    }
    let receiver = matches!(qualifier, "this" | "self" | "Self" | "cls" | "super");
    let local = index
        .by_name
        .get(name)
        .map(|ids| {
            ids.iter()
                .copied()
                .filter(|id| accept(*id))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    // The parser saw the receiver's type (`this.a.m()` may be recorded as a
    // bare `m`): such a call is a member call, never a local/import match.
    let typed = receiver_type.is_some() && !receiver;
    if !typed && (qualifier.is_empty() || receiver) && !local.is_empty() {
        // Receiver calls prefer members of the caller's own container.
        let container = nodes.get(caller as usize).map_or(NONE, |node| node.parent);
        let siblings = local
            .iter()
            .copied()
            .filter(|id| receiver && container != NONE && nodes[*id as usize].parent == container)
            .collect::<Vec<_>>();
        let targets = if siblings.is_empty() { local } else { siblings };
        let confidence = confidence_for(&targets, Confidence::High);
        return Some((targets, "local", confidence));
    }
    let head = first_segment(qualifier);
    if qualifier.is_empty() && !typed {
        if let Some(sources) = bindings.named.get(name) {
            let mut targets = Vec::new();
            let mut hopped = false;
            for (target, exported) in sources {
                let (found, via) = resolver.exported(target, exported);
                hopped |= via;
                targets.extend(found.into_iter().filter(|id| accept(*id)));
            }
            targets.sort_unstable();
            targets.dedup();
            if !targets.is_empty() {
                let strong = if hopped {
                    Confidence::Medium
                } else {
                    Confidence::High
                };
                let confidence = confidence_for(&targets, strong);
                return Some((targets, "import", confidence));
            }
        }
    } else if !receiver
        && qualifier.contains("::")
        && let Some(module_file) = scope
            .file_paths
            .get(&nodes.get(caller as usize).map_or(NONE, |n| n.file))
            .and_then(|caller_path| scope.rust_module_file(qualifier, caller_path))
    {
        // `crate::a::b::f()`, `super::b::f()`, `other_crate::b::f()`.
        let targets = resolver
            .declared(module_file, name)
            .into_iter()
            .filter(|id| accept(*id))
            .collect::<Vec<_>>();
        if !targets.is_empty() {
            let confidence = confidence_for(&targets, Confidence::High);
            return Some((targets, "module-path", confidence));
        }
    } else if !receiver && let Some(files) = bindings.modules.get(head) {
        // `ns.fn()`, `pkg.Func()`, `module::func()`: the qualifier's first
        // segment names an import binding whose target files declare `name`.
        let mut targets = Vec::new();
        for target in files {
            let (exported, _) = resolver.exported(target, name);
            if exported.is_empty() {
                targets.extend(
                    resolver
                        .declared(target, name)
                        .into_iter()
                        .filter(|id| accept(*id)),
                );
            } else {
                targets.extend(exported.into_iter().filter(|id| accept(*id)));
            }
        }
        targets.sort_unstable();
        targets.dedup();
        if !targets.is_empty() {
            let confidence = confidence_for(&targets, Confidence::Medium);
            return Some((targets, "namespace", confidence));
        }
    }
    // An external binding (`import pad from 'left-pad'`, `use std::fmt`,
    // `serde_json::from_value`) is that package's, never ours.
    let external_binding = qualifier.is_empty() && bindings.external.contains(name);
    if external_binding || (!qualifier.is_empty() && !receiver && bindings.external.contains(head))
    {
        return None;
    }
    // Last resort: exactly one declaration graph-wide carries this name. A
    // type-qualified call (`Type::new`, `Type.of`) must land in that type.
    // `Type::f` / `Type.f`, including the last segment of a Rust path
    // (`crate::store::Store::new`).
    let path_type = qualifier.rsplit("::").next().filter(|last| {
        qualifier.contains("::") && last.starts_with(|c: char| c.is_ascii_uppercase())
    });
    let head = path_type.unwrap_or(head);
    let type_qualified = !receiver && head.starts_with(|c: char| c.is_ascii_uppercase());
    // `expr.name()`: only a method can be the target, and never a name every
    // type implements.
    // `expr.name()` has an unknown receiver type: a graph-wide name match is
    // a coincidence (`.all()`, `.replace()`, `.kind()` on std/foreign types).
    let member_call = typed || (!qualifier.is_empty() && !receiver && !type_qualified);
    // A member call links only through a receiver type the parser saw
    // declared or constructed locally (`let s: Store`, `s = Store::new()`).
    let type_head = if member_call {
        match receiver_type.map(|ty| ty.rsplit([':', '.']).next().unwrap_or(ty)) {
            Some(ty) if !ty.is_empty() => ty,
            _ => return None,
        }
    } else {
        head
    };
    // A bare call that only matches a method elsewhere is a method call whose
    // receiver the extractor dropped; skip names every type implements.
    let bare_method_call =
        |id: u32| nodes[id as usize].parent != NONE && COMMON_METHODS.contains(&name);
    let candidates = global_names
        .get(name)
        .map(|ids| {
            ids.iter()
                .copied()
                .filter(|id| accept(*id) && (member_call || !bare_method_call(*id)))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if name.len() < MIN_UNIQUE_NAME_LEN || candidates.is_empty() {
        return None;
    }
    let caller_file = nodes.get(caller as usize).map_or(NONE, |n| n.file);
    let dir = |file: u32| {
        scope
            .file_paths
            .get(&file)
            .map_or("", |path| path.rsplit_once('/').map_or("", |(dir, _)| dir))
    };
    let caller_dir = dir(caller_file);
    // Scope preference: the caller's package (directory), then files the
    // caller imports; a choice is made only when it is unique.
    let pick = |pool: &[u32]| -> Option<(u32, &'static str)> {
        let same_package = pool
            .iter()
            .copied()
            .filter(|id| dir(nodes[*id as usize].file) == caller_dir)
            .collect::<Vec<_>>();
        if let [only] = same_package.as_slice() {
            return Some((*only, "same-package"));
        }
        let imported = pool
            .iter()
            .copied()
            .filter(|id| {
                scope
                    .file_paths
                    .get(&nodes[*id as usize].file)
                    .is_some_and(|path| bindings.imported_files.contains(path))
            })
            .collect::<Vec<_>>();
        if let [only] = imported.as_slice() {
            return Some((*only, "import-scope"));
        }
        None
    };
    if type_qualified || member_call {
        // `Type::method` / `Type.method`, or `x.method()` with `x: Type`: a
        // member of a container named `Type`.
        let members = candidates
            .iter()
            .copied()
            .filter(|id| container_names.get(id) == Some(&type_head))
            .collect::<Vec<_>>();
        let chosen = match members.as_slice() {
            [] => None,
            [only] => Some(*only),
            many => pick(many).map(|(id, _)| id),
        };
        return chosen.map(|id| {
            let near = dir(nodes[id as usize].file) == caller_dir
                || scope
                    .file_paths
                    .get(&nodes[id as usize].file)
                    .is_some_and(|path| bindings.imported_files.contains(path));
            let confidence = if near {
                Confidence::High
            } else {
                Confidence::Medium
            };
            let via = if member_call {
                "receiver-type"
            } else {
                "type-qualified"
            };
            (vec![id], via, confidence)
        });
    }
    if let Some((id, via)) = pick(&candidates)
        && nodes[id as usize].file != caller_file
    {
        return Some((vec![id], via, Confidence::Medium));
    }
    // Last resort: exactly one declaration graph-wide carries this name.
    if let [only] = candidates.as_slice()
        && nodes[*only as usize].file != caller_file
    {
        return Some((vec![*only], "unique-name", Confidence::Low));
    }
    None
}

/// Per-graph lookups `link_call` needs for scope-aware choices.
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
