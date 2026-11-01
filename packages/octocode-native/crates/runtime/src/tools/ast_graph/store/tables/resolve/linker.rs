//! The call linker: how one call site resolves to target symbols.
use super::*;
use octocode_engine::graph::COMMONJS_WHOLE_EXPORT;

/// A linked call: target symbols, how they were found, and the confidence.
pub(super) type Linked = (Vec<u32>, &'static str, Confidence);

/// Graph-wide lookups a call or base-type link needs.
pub(crate) struct Linker<'r> {
    pub(crate) resolver: &'r ExportResolver<'r>,
    pub(crate) nodes: &'r [NodeRec],
    pub(crate) strings: &'r [String],
    pub(crate) global_names: &'r HashMap<&'r str, Vec<u32>>,
    pub(crate) members: &'r MemberIndex<'r>,
    pub(crate) scope: &'r LinkScope<'r>,
    pub(crate) bases: &'r BaseIndex,
    pub(crate) is_type: &'r dyn Fn(u32) -> bool,
    /// Declarations whose own extent was recovered (never targets).
    pub(crate) recovered: &'r dyn Fn(u32) -> bool,
    /// Names of the `macro_rules!` the repo's Rust files declare.
    pub(crate) rust_macros: &'r HashSet<&'r str>,
    /// A Rust function's declared return type and the type `?` yields from
    /// it, by declaration node.
    pub(crate) return_types: &'r HashMap<u32, ReturnTypes<'r>>,
}

/// See [`Linker::return_types`].
pub(crate) type ReturnTypes<'r> = (Option<&'r str>, Option<&'r str>);

/// Where a bare name may bind once its file and imports did not
/// ([`Linker::bare_scope`]).
pub(super) enum BareScope {
    /// Any declaration in the language family, scoped by preference.
    Graph,
    /// The caller's Go package.
    Package,
    /// The caller's module, which `local` and `imported` already searched.
    Module,
}

/// One call site (or base-type clause) to link.
pub(crate) struct Site<'s> {
    pub(crate) bindings: &'s FileBindings<'s>,
    pub(crate) index: &'s FileIndex<'s>,
    pub(crate) caller: u32,
    pub(crate) qualifier: &'s str,
    pub(crate) name: &'s str,
    pub(crate) accept: &'s dyn Fn(u32) -> bool,
    pub(crate) receiver_type: Option<&'s str>,
    /// The callee's head is a term bound in scope (see `Call::term_binding`).
    pub(crate) term_binding: bool,
    /// The receiver is a Scala self alias of this enclosing template.
    pub(crate) self_template: Option<&'s str>,
    /// A Rust or C-family `value.name()` call: its qualifier is a value (a
    /// local, a field, a call result), which never names a module or a C++
    /// namespace, even when an imported module shares its name
    /// (`driver.events.f()` beside `use crate::driver`).
    pub(crate) value_qualifier: bool,
}

impl Site<'_> {
    /// `this`/`self`/`super` style receiver.
    pub(super) fn receiver(&self) -> bool {
        matches!(
            self.qualifier,
            "this" | "self" | "Self" | "cls" | "super" | "super()"
        )
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
    pub(crate) fn link(&self, site: &Site) -> Option<Linked> {
        if site.name.is_empty() {
            return None;
        }
        if self.is_rust(site.caller)
            && let Some(linked) = self.rust_inline_path(site)
        {
            return Some(linked);
        }
        if site.receiver()
            && let Some(owner) = self.receiver_owner(site)
        {
            return self.bound_member(
                site,
                owner,
                matches!(site.qualifier, "super" | "super()"),
                Confidence::High,
                "local",
            );
        }
        if let Some(linked) = self.local(site) {
            return Some(linked);
        }
        if let Some(linked) = self.rust_self_impl(site) {
            return Some(linked);
        }
        let head = first_segment(site.qualifier);
        // An import-bound name resolves through its binding or not at all:
        // a same-named declaration elsewhere is a different identity.
        if site.qualifier.is_empty()
            && !site.typed()
            && site.bindings.strict_names.contains(site.name)
        {
            return self.imported(site);
        }
        if !site.qualifier.is_empty()
            && !site.receiver()
            && site.bindings.commonjs_locals.contains(head)
        {
            let files = site.bindings.commonjs_modules.get(head);
            return files
                .filter(|_| site.qualifier == head)
                .and_then(|files| self.strict_namespace(site, files));
        }
        if !site.qualifier.is_empty()
            && !site.receiver()
            && !site.typed()
            && site.bindings.strict_names.contains(head)
            && let Some(sources) = site.bindings.named.get(head)
        {
            return self.import_member(site, sources);
        }
        // JS/TS: a receiver path the syntax gives no type (`this.a.m()`,
        // `Foo.cache.get()`, `make().run()`) names no declaration.
        if !site.typed()
            && site.qualifier.contains(['.', '(', '['])
            && self.scope.families.get(self.file_of(site.caller) as usize) == Some(&"npm")
        {
            return None;
        }
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
        // Unmatched, `Type::f()` still searches the type's bases below.
        if self.c_family_path(site)
            && let Some(linked) = self.c_family_qualified(site)
        {
            return Some(linked);
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
                .path(self.file_of(site.caller))
                .and_then(|caller_path| self.scope.rust_module_file(site.qualifier, caller_path))
        {
            // `crate::a::b::f()`, `super::b::f()`, `other_crate::b::f()`:
            // what the module exports (through `pub use`), else declares.
            let targets = site.accepted(self.resolver.exported_or_declared(module_file, site.name));
            let confidence = confidence_for(&targets, Confidence::High);
            return (!targets.is_empty()).then_some((targets, "module-path", confidence));
        }
        if !receiver
            && !site.value_qualifier
            && let Some(files) = site.bindings.modules.get(head)
        {
            return self.namespace(site, files);
        }
        None
    }

    fn imported(&self, site: &Site) -> Option<Linked> {
        let mut targets = Vec::new();
        let mut uncertain = false;
        let via = if let Some(sources) = site.bindings.named.get(site.name) {
            for (target, exported) in sources {
                let mut resolved = self.resolver.resolved(target, exported);
                // Node interop: an ESM default import of a CommonJS module
                // is its `module.exports` value.
                if *exported == "default" && resolved.targets.is_empty() {
                    resolved = self.resolver.resolved(target, COMMONJS_WHOLE_EXPORT);
                }
                uncertain |= resolved.uncertain;
                targets.extend(site.accepted(resolved.targets));
            }
            "import"
        } else {
            if site.bindings.external.contains(site.name) {
                return None;
            }
            uncertain |= site.bindings.unknown_wildcard;
            let private = site.bindings.star_public_only && site.name.starts_with('_');
            for target in site.bindings.wildcards.iter().filter(|_| !private) {
                let resolved = self.resolver.resolved(target, site.name);
                uncertain |= resolved.uncertain;
                targets.extend(site.accepted(resolved.targets));
            }
            "import-glob"
        };
        targets.sort_unstable();
        targets.dedup();
        // A re-export preserves identity; competing or unobserved branches
        // weaken the candidate without erasing the targets we did resolve.
        let strong = if uncertain {
            Confidence::Medium
        } else {
            Confidence::High
        };
        let confidence = confidence_for(&targets, strong);
        (!targets.is_empty()).then_some((targets, via, confidence))
    }

    /// `ns.fn()`, `pkg.Func()`, `module::func()`: the qualifier's first
    /// segment names an import binding whose target files declare `name`.
    fn namespace(&self, site: &Site, files: &[&str]) -> Option<Linked> {
        let mut targets = Vec::new();
        for target in files {
            targets.extend(site.accepted(self.resolver.exported_or_declared(target, site.name)));
        }
        targets.sort_unstable();
        targets.dedup();
        let confidence = confidence_for(&targets, Confidence::High);
        (!targets.is_empty()).then_some((targets, "namespace", confidence))
    }

    /// `m.fn()` through a whole-module `require` binding: only what the
    /// module exports, never another declaration of the name in it.
    fn strict_namespace(&self, site: &Site, files: &[&str]) -> Option<Linked> {
        let mut targets = Vec::new();
        let mut uncertain = false;
        for target in files {
            let resolved = self.resolver.resolved(target, site.name);
            uncertain |= resolved.uncertain;
            targets.extend(site.accepted(resolved.targets));
        }
        targets.sort_unstable();
        targets.dedup();
        let strong = if uncertain {
            Confidence::Medium
        } else {
            Confidence::High
        };
        let confidence = confidence_for(&targets, strong);
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
        // A value qualifier (`T::open().m`, `items.m`) is never a type, even
        // when it starts with one.
        let type_qualified = !receiver
            && !site.value_qualifier
            && head.starts_with(|c: char| c.is_ascii_uppercase());
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
        // The written or inferred type scopes a member call, so only a bare
        // call skips names every type implements.
        if type_qualified || member_call {
            let via = if member_call {
                "receiver-type"
            } else {
                "type-qualified"
            };
            return self.member(site, type_head, via);
        }
        // A bare call that only matches a method elsewhere is a method call
        // whose receiver the extractor dropped; skip names every type
        // implements.
        let common_method = COMMON_METHODS.contains(&name);
        let bare_method_call = |id: u32| common_method && self.nodes[id as usize].parent != NONE;
        // A recovered declaration of the name stays in the pool: when scope
        // would choose it, or it makes the name non-unique, nothing links.
        let candidates = self
            .named(site.caller, name)
            .filter(|id| ((site.accept)(*id) || (self.recovered)(*id)) && !bare_method_call(*id))
            .collect::<Vec<_>>();
        if !candidates.iter().any(|id| (site.accept)(*id)) {
            return None;
        }
        match self.bare_scope(site.caller) {
            BareScope::Module => return None,
            BareScope::Package => return self.same_go_package(site, &candidates),
            BareScope::Graph => {}
        }
        let caller_file = self.file_of(site.caller);
        if let Some((id, via)) = self.pick(site, &candidates) {
            return ((site.accept)(id) && self.file_of(id) != caller_file)
                .then(|| (vec![id], via, Confidence::Medium));
        }
        // Exactly one declaration graph-wide carries this name, and the name
        // is distinctive enough for that to mean something.
        if let [only] = candidates.as_slice()
            && name.len() >= MIN_UNIQUE_NAME_LEN
            && (site.accept)(*only)
            && self.file_of(*only) != caller_file
        {
            return Some((vec![*only], "unique-name", Confidence::Low));
        }
        None
    }

    /// The graph-wide declarations named `name` in `caller`'s language
    /// family: without an import, a name never binds across languages.
    /// Every graph-wide pool reads [`Linker::global_names`] through this.
    pub(crate) fn named<'a>(&'a self, caller: u32, name: &str) -> impl Iterator<Item = u32> + 'a {
        let caller_file = self.file_of(caller);
        self.global_names
            .get(name)
            .into_iter()
            .flatten()
            .copied()
            .filter(move |id| self.scope.same_family(caller_file, self.file_of(*id)))
    }

    /// Whether `id` belongs to `caller`'s language family.
    pub(super) fn same_family(&self, caller: u32, id: u32) -> bool {
        self.scope
            .same_family(self.file_of(caller), self.file_of(id))
    }
}
