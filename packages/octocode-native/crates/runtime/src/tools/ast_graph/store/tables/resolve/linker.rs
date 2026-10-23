//! The call linker: how one call site resolves to target symbols, and the
//! directory scope that bounds unique-name links.
use super::*;

/// A linked call: target symbols, how they were found, and the confidence.
pub(in super::super) type Linked = (Vec<u32>, &'static str, Confidence);

/// Graph-wide lookups a call or base-type link needs.
pub(in super::super) struct Linker<'r> {
    pub(in super::super) resolver: &'r ExportResolver<'r>,
    pub(in super::super) nodes: &'r [NodeRec],
    pub(in super::super) global_names: &'r HashMap<&'r str, Vec<u32>>,
    pub(in super::super) members: &'r MemberIndex<'r>,
    pub(in super::super) scope: &'r LinkScope<'r>,
}

/// One call site (or base-type clause) to link.
pub(in super::super) struct Site<'s> {
    pub(in super::super) bindings: &'s FileBindings<'s>,
    pub(in super::super) index: &'s FileIndex<'s>,
    pub(in super::super) caller: u32,
    pub(in super::super) qualifier: &'s str,
    pub(in super::super) name: &'s str,
    pub(in super::super) accept: &'s dyn Fn(u32) -> bool,
    pub(in super::super) receiver_type: Option<&'s str>,
}

impl Site<'_> {
    /// `this`/`self`/`super` style receiver.
    pub(in super::super) fn receiver(&self) -> bool {
        matches!(self.qualifier, "this" | "self" | "Self" | "cls" | "super")
    }
    /// The parser saw the receiver's type (`this.a.m()` may be recorded as a
    /// bare `m`): such a call is a member call, never a local/import match.
    pub(in super::super) fn typed(&self) -> bool {
        self.receiver_type.is_some() && !self.receiver()
    }
    pub(in super::super) fn accepted(&self, ids: impl IntoIterator<Item = u32>) -> Vec<u32> {
        ids.into_iter().filter(|id| (self.accept)(*id)).collect()
    }
}

impl Linker<'_> {
    pub(in super::super) fn link(&self, site: &Site) -> Option<Linked> {
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
    pub(in super::super) fn local(&self, site: &Site) -> Option<Linked> {
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
    pub(in super::super) fn scoped(&self, site: &Site, head: &str) -> Option<Linked> {
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

    pub(in super::super) fn imported(&self, site: &Site) -> Option<Linked> {
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
    pub(in super::super) fn namespace(&self, site: &Site, files: &[&str]) -> Option<Linked> {
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
    pub(in super::super) fn global(&self, site: &Site, head: &str) -> Option<Linked> {
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
    pub(in super::super) fn member(
        &self,
        site: &Site,
        type_head: &str,
        via: &'static str,
    ) -> Option<Linked> {
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
    pub(in super::super) fn pick(&self, site: &Site, pool: &[u32]) -> Option<(u32, &'static str)> {
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
    pub(in super::super) fn near(&self, site: &Site, id: u32) -> bool {
        self.dir(self.file_of(id)) == self.dir(self.file_of(site.caller))
            || self.imported_by(site, id)
    }

    pub(in super::super) fn imported_by(&self, site: &Site, id: u32) -> bool {
        self.scope
            .path(self.file_of(id))
            .is_some_and(|path| site.bindings.imported_files.contains(path))
    }

    pub(in super::super) fn file_of(&self, id: u32) -> u32 {
        self.nodes.get(id as usize).map_or(NONE, |n| n.file)
    }

    pub(in super::super) fn dir(&self, file: u32) -> &str {
        self.scope
            .path(file)
            .map_or("", crate::tools::ast_graph::graph::parent)
    }
}

/// Per-graph lookups [`Linker`] needs for scope-aware choices.
pub(in super::super) struct LinkScope<'a> {
    /// Root-relative path by file node id (`""` for other nodes).
    pub(in super::super) file_paths: &'a [&'a str],
    /// Rust module name → files defining it (`m.rs`, `m/mod.rs`).
    pub(in super::super) modules: HashMap<&'a str, Vec<&'a str>>,
    /// Workspace crate import name (`octocode_engine`) → crate directory.
    pub(in super::super) crate_dirs: HashMap<String, String>,
}

impl LinkScope<'_> {
    /// The root-relative path of file node `id`.
    pub(in super::super) fn path(&self, id: u32) -> Option<&str> {
        self.file_paths
            .get(id as usize)
            .copied()
            .filter(|path| !path.is_empty())
    }

    /// The file a Rust module path (`crate::a::b`, `super::b`,
    /// `other_crate::b`) names, by its last segment, scoped to the caller's
    /// crate (or the named workspace crate) and settled by the longest shared
    /// directory prefix when several files share the module name.
    pub(in super::super) fn rust_module_file(
        &self,
        qualifier: &str,
        caller_file: &str,
    ) -> Option<&str> {
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
