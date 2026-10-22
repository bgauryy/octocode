//! Name resolution for uses, calls and heritage: the export resolver, each
//! file's bindings, and the call linker.
use super::*;

/// Files each file star-re-exports.
pub(super) fn star_targets(built: &BuiltGraph) -> HashMap<&str, Vec<&str>> {
    built
        .nodes
        .iter()
        .map(|(file, node)| {
            let targets = node
                .edges
                .iter()
                .filter(|(_, kinds)| kinds.keys().any(|kind| kind.ends_with("star-reexport")))
                .map(|(target, _)| target.as_str())
                .collect::<Vec<_>>();
            (file.as_str(), targets)
        })
        .collect()
}

/// Unique-name candidates that are members, by `(container name, name)`,
/// in [`Symbols::global_names`] order: a member call looks up its type's
/// members instead of filtering every declaration of the name.
pub(super) fn member_index<'a>(
    global_names: &HashMap<&'a str, Vec<u32>>,
    container_names: &HashMap<u32, &'a str>,
) -> MemberIndex<'a> {
    let mut members = MemberIndex::new();
    for (name, ids) in global_names {
        for id in ids {
            if let Some(container) = container_names.get(id) {
                members.entry((*container, *name)).or_default().push(*id);
            }
        }
    }
    members
}

/// See [`member_index`].
pub(super) type MemberIndex<'a> = HashMap<(&'a str, &'a str), Vec<u32>>;

/// Rust module and workspace-crate lookups for qualified call paths.
pub(super) fn link_scope<'a>(file_paths: &'a [&'a str], workspace: &Workspace) -> LinkScope<'a> {
    let mut modules = HashMap::<&str, Vec<&str>>::new();
    for path in file_paths.iter().filter(|path| !path.is_empty()) {
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
pub(super) type PendingEdge = (u32, u32, EdgeKind, Confidence, String, u32);

/// One file's call-linking context.
pub(super) struct FileLinks<'f> {
    pub(super) bindings: FileBindings<'f>,
    pub(super) index: &'f FileIndex<'f>,
    pub(super) file_id: u32,
    pub(super) language: &'f str,
}

/// calls: each call site linked to its target declarations.
pub(super) fn link_calls(
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
pub(super) fn link_heritage(
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
pub(super) type Resolved = (Vec<u32>, bool);

pub(super) struct ExportResolver<'a> {
    pub(super) built: &'a BuiltGraph,
    pub(super) indexes: &'a BTreeMap<&'a str, FileIndex<'a>>,
    pub(super) star_targets: &'a HashMap<&'a str, Vec<&'a str>>,
    /// file → name → resolution; call sites repeat the same imports. Keyed
    /// in two levels so a hit is looked up by borrowed strings.
    pub(super) memo: std::cell::RefCell<HashMap<String, HashMap<String, Resolved>>>,
}

impl ExportResolver<'_> {
    pub(super) fn exported(&self, file: &str, name: &str) -> (Vec<u32>, bool) {
        if let Some(hit) = self
            .memo
            .borrow()
            .get(file)
            .and_then(|names| names.get(name))
        {
            return hit.clone();
        }
        let result = self.exported_uncached(file, name);
        self.memo
            .borrow_mut()
            .entry(file.to_owned())
            .or_default()
            .insert(name.to_owned(), result.clone());
        result
    }

    /// Declarations `file` exports under `name`, following named and star
    /// re-exports up to [`MAX_REEXPORT_DEPTH`] hops.
    pub(super) fn exported_uncached(&self, file: &str, name: &str) -> (Vec<u32>, bool) {
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
                // A Rust `use` binds its names in the module, so `crate::a::f`
                // reaches what `a`'s `use b::f;` (or `use b::*;`) names; that
                // the path compiles is the proof the binding is visible.
                if crate::tools::ast_graph::linking::Linker::of(&file)
                    == crate::tools::ast_graph::linking::Linker::Rust
                {
                    for import in &facts.imports {
                        let Some(target) = &import.target else {
                            continue;
                        };
                        if import.imported_name == "*" {
                            pending.push((target.clone(), name.clone(), depth + 1));
                        } else if import.local_name.as_deref() == Some(name.as_str()) {
                            pending.push((target.clone(), import.imported_name.clone(), depth + 1));
                        }
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

    /// What `file` exports under `name` (following re-exports), else any
    /// declaration of that name in it: a module-qualified call reaches both.
    pub(super) fn exported_or_declared(&self, file: &str, name: &str) -> Vec<u32> {
        let (exported, _) = self.exported(file, name);
        if exported.is_empty() {
            self.declared(file, name)
        } else {
            exported
        }
    }

    /// Any declaration named `name` in `file` (module-qualified calls such as
    /// Go `pkg.Func` or Rust `module::func` need not be re-exported).
    pub(super) fn declared(&self, file: &str, name: &str) -> Vec<u32> {
        self.indexes
            .get(file)
            .and_then(|index| index.by_name.get(name))
            .cloned()
            .unwrap_or_default()
    }
}

pub(super) fn confidence_for(targets: &[u32], strong: Confidence) -> Confidence {
    if targets.len() == 1 {
        strong
    } else {
        Confidence::Medium.max(strong)
    }
}

/// Per-file import bindings, built once so each call site is O(1).
pub(super) struct FileBindings<'a> {
    /// Local name → `(target file, name exported there)` for linked imports.
    pub(super) named: HashMap<&'a str, Vec<(&'a str, &'a str)>>,
    /// Namespace/module binding (`ns` in `ns.f`, `pkg` in `pkg.F`) → files.
    pub(super) modules: HashMap<&'a str, Vec<&'a str>>,
    /// Heads bound to third-party or standard packages.
    pub(super) external: HashSet<String>,
    /// Files this file imports (scope for ambiguous names).
    pub(super) imported_files: HashSet<&'a str>,
}

impl<'a> FileBindings<'a> {
    pub(super) fn new(facts: &'a FileFacts, language: &str) -> Self {
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
pub(super) type Linked = (Vec<u32>, &'static str, Confidence);

/// Graph-wide lookups a call or base-type link needs.
pub(super) struct Linker<'r> {
    pub(super) resolver: &'r ExportResolver<'r>,
    pub(super) nodes: &'r [NodeRec],
    pub(super) global_names: &'r HashMap<&'r str, Vec<u32>>,
    pub(super) members: &'r MemberIndex<'r>,
    pub(super) scope: &'r LinkScope<'r>,
}

/// One call site (or base-type clause) to link.
pub(super) struct Site<'s> {
    pub(super) bindings: &'s FileBindings<'s>,
    pub(super) index: &'s FileIndex<'s>,
    pub(super) caller: u32,
    pub(super) qualifier: &'s str,
    pub(super) name: &'s str,
    pub(super) accept: &'s dyn Fn(u32) -> bool,
    pub(super) receiver_type: Option<&'s str>,
}

impl Site<'_> {
    /// `this`/`self`/`super` style receiver.
    pub(super) fn receiver(&self) -> bool {
        matches!(self.qualifier, "this" | "self" | "Self" | "cls" | "super")
    }
    /// The parser saw the receiver's type (`this.a.m()` may be recorded as a
    /// bare `m`): such a call is a member call, never a local/import match.
    pub(super) fn typed(&self) -> bool {
        self.receiver_type.is_some() && !self.receiver()
    }
    pub(super) fn accepted(&self, ids: impl IntoIterator<Item = u32>) -> Vec<u32> {
        ids.into_iter().filter(|id| (self.accept)(*id)).collect()
    }
}

impl Linker<'_> {
    pub(super) fn link(&self, site: &Site) -> Option<Linked> {
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
    pub(super) fn local(&self, site: &Site) -> Option<Linked> {
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
    pub(super) fn scoped(&self, site: &Site, head: &str) -> Option<Linked> {
        let receiver = site.receiver();
        if site.qualifier.is_empty() && !site.typed() {
            return self.imported(site);
        }
        if !receiver
            && site.qualifier.contains("::")
            && let Some(module_file) = self
                .scope
                .path(self.file_of(site.caller))
                .and_then(|caller_path| self.scope.rust_module_file(site.qualifier, caller_path))
        {
            // `crate::a::b::f()`, `super::b::f()`, `other_crate::b::f()`:
            // what the module exports (through `pub use`), else declares.
            let targets = site.accepted(self.resolver.exported_or_declared(module_file, site.name));
            let confidence = confidence_for(&targets, Confidence::High);
            return (!targets.is_empty()).then_some((targets, "module-path", confidence));
        }
        if !receiver && let Some(files) = site.bindings.modules.get(head) {
            return self.namespace(site, files);
        }
        None
    }

    pub(super) fn imported(&self, site: &Site) -> Option<Linked> {
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
    pub(super) fn namespace(&self, site: &Site, files: &[&str]) -> Option<Linked> {
        let mut targets = Vec::new();
        for target in files {
            targets.extend(site.accepted(self.resolver.exported_or_declared(target, site.name)));
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
    pub(super) fn global(&self, site: &Site, head: &str) -> Option<Linked> {
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
        if name.len() < MIN_UNIQUE_NAME_LEN {
            return None;
        }
        let common_method = !member_call && COMMON_METHODS.contains(&name);
        if type_qualified || member_call {
            let via = if member_call {
                "receiver-type"
            } else {
                "type-qualified"
            };
            // Every member has a parent, so a common method name in a bare
            // call matches none.
            return (!common_method)
                .then(|| self.member(site, type_head, via))
                .flatten();
        }
        let bare_method_call = |id: u32| common_method && self.nodes[id as usize].parent != NONE;
        let candidates = self
            .global_names
            .get(name)
            .map(|ids| {
                ids.iter()
                    .copied()
                    .filter(|id| (site.accept)(*id) && !bare_method_call(*id))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if candidates.is_empty() {
            return None;
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
    pub(super) fn member(&self, site: &Site, type_head: &str, via: &'static str) -> Option<Linked> {
        let members = site.accepted(self.members.get(&(type_head, site.name))?.iter().copied());
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
    pub(super) fn pick(&self, site: &Site, pool: &[u32]) -> Option<(u32, &'static str)> {
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
    pub(super) fn near(&self, site: &Site, id: u32) -> bool {
        self.dir(self.file_of(id)) == self.dir(self.file_of(site.caller))
            || self.imported_by(site, id)
    }

    pub(super) fn imported_by(&self, site: &Site, id: u32) -> bool {
        self.scope
            .path(self.file_of(id))
            .is_some_and(|path| site.bindings.imported_files.contains(path))
    }

    pub(super) fn file_of(&self, id: u32) -> u32 {
        self.nodes.get(id as usize).map_or(NONE, |n| n.file)
    }

    pub(super) fn dir(&self, file: u32) -> &str {
        self.scope
            .path(file)
            .map_or("", |path| path.rsplit_once('/').map_or("", |(dir, _)| dir))
    }
}

/// Per-graph lookups [`Linker`] needs for scope-aware choices.
pub(super) struct LinkScope<'a> {
    /// Root-relative path by file node id (`""` for other nodes).
    pub(super) file_paths: &'a [&'a str],
    /// Rust module name → files defining it (`m.rs`, `m/mod.rs`).
    pub(super) modules: HashMap<&'a str, Vec<&'a str>>,
    /// Workspace crate import name (`octocode_engine`) → crate directory.
    pub(super) crate_dirs: HashMap<String, String>,
}

impl LinkScope<'_> {
    /// The root-relative path of file node `id`.
    pub(super) fn path(&self, id: u32) -> Option<&str> {
        self.file_paths
            .get(id as usize)
            .copied()
            .filter(|path| !path.is_empty())
    }

    /// The file a Rust module path (`crate::a::b`, `super::b`,
    /// `other_crate::b`) names, by its last segment, scoped to the caller's
    /// crate (or the named workspace crate) and settled by the longest shared
    /// directory prefix when several files share the module name.
    pub(super) fn rust_module_file(&self, qualifier: &str, caller_file: &str) -> Option<&str> {
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
