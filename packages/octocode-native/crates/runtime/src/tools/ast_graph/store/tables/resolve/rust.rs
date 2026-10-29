//! Rust paths through inline modules, as rust-analyzer resolves them: a
//! path starts at the module that spells it (`self`, a child module's name),
//! at an ancestor (`super`), or at the crate root (`crate`); each further
//! segment is a child module, and the last names an item of the module the
//! path reaches. A file module's paths that never enter an inline module
//! resolve through [`super::LinkScope::rust_module_file`].
use super::linker::{Linked, Linker, Site};
use super::*;

impl Linker<'_> {
    pub(super) fn is_rust(&self, caller: u32) -> bool {
        let file = self.file_of(caller);
        self.scope.families.get(file as usize).copied() == Some("cargo")
    }

    pub(super) fn rust_inline_path(&self, site: &Site) -> Option<Linked> {
        if site.value_qualifier || site.qualifier.is_empty() {
            return None;
        }
        let segments = site.qualifier.split("::").collect::<Vec<_>>();
        let mut module = self.enclosing_module(site.caller);
        let mut rest = segments.as_slice();
        let mut entered_inline = self.nodes[module as usize].kind == NodeKind::Symbol;
        match segments.first().copied() {
            Some("self") => rest = &rest[1..],
            Some("super") if entered_inline => {
                while let Some(("super", tail)) = rest.split_first().map(|(h, t)| (*h, t)) {
                    // Past the file's top module, the file-level resolver
                    // climbs the module tree.
                    if self.nodes[module as usize].kind != NodeKind::Symbol {
                        return None;
                    }
                    module = self.parent_module(module);
                    rest = tail;
                }
            }
            Some("crate" | "super") => {
                let caller_path = self.scope.path(self.file_of(site.caller))?;
                // The longest prefix naming a module file; the rest must
                // then enter that file's inline modules.
                let (file, used) = (1..segments.len()).rev().find_map(|used| {
                    let prefix = segments[..used].join("::");
                    self.scope
                        .rust_module_file(&prefix, caller_path)
                        .map(|file| (file, used))
                })?;
                module = self.file_node(file)?;
                rest = &segments[used..];
                entered_inline = false;
            }
            Some(_) => {}
            None => return None,
        }
        for segment in rest {
            module = self.child_module(module, segment)?;
            entered_inline = true;
        }
        if !entered_inline {
            return None;
        }
        let file = self.scope.path(self.nodes[module as usize].file)?;
        let parent = if self.nodes[module as usize].kind == NodeKind::Symbol {
            module
        } else {
            NONE
        };
        let targets = site.accepted(
            self.resolver
                .indexes
                .get(file)?
                .by_name
                .get(site.name)?
                .iter()
                .copied()
                .filter(|id| self.nodes[*id as usize].parent == parent),
        );
        let confidence = confidence_for(&targets, Confidence::High);
        (!targets.is_empty()).then_some((targets, "module-path", confidence))
    }

    /// Method resolution tries inherent methods before trait methods: an
    /// inherent `impl T` member shadows the same name in an `impl Trait for
    /// T` (one whose trait resolved to a repo declaration).
    pub(super) fn rust_inherent_first(&self, caller: u32, members: Vec<u32>) -> Vec<u32> {
        if members.len() < 2 || !self.is_rust(caller) {
            return members;
        }
        let trait_impl = |id: &u32| {
            let owner = self.nodes[*id as usize].parent;
            owner != NONE && self.bases.interfaces.contains_key(&owner)
        };
        if members.iter().all(trait_impl) {
            return members;
        }
        members.into_iter().filter(|id| !trait_impl(id)).collect()
    }

    /// `T`'s method `name` that no `impl T` block declares: a provided
    /// (default) method of a trait some `impl Trait for T` names. The type
    /// is matched by name, and several traits may provide the name, so the
    /// candidate stays medium.
    pub(super) fn rust_trait_member(&self, site: &Site, type_head: &str) -> Option<Linked> {
        if !self.is_rust(site.caller) {
            return None;
        }
        let mut targets = BTreeSet::new();
        for implementation in self.named(site.caller, type_head) {
            let traits = self.bases.interfaces.get(&implementation);
            for (provider, _) in traits.into_iter().flatten() {
                let trait_name =
                    self.strings[self.nodes[*provider as usize].name as usize].as_str();
                targets.extend(
                    site.accepted(
                        self.members
                            .get(&(trait_name, site.name))
                            .into_iter()
                            .flatten()
                            .copied()
                            .filter(|id| self.nodes[*id as usize].parent == *provider),
                    ),
                );
            }
        }
        let targets = targets.into_iter().collect::<Vec<_>>();
        (!targets.is_empty()).then_some((targets, "inherited", Confidence::Medium))
    }

    fn is_inline_module(&self, id: u32) -> bool {
        let node = &self.nodes[id as usize];
        node.kind == NodeKind::Symbol && self.strings[node.detail as usize] == "module"
    }

    /// The inline module that holds `id`, else its file node.
    fn enclosing_module(&self, id: u32) -> u32 {
        let mut current = id;
        for _ in 0..self.nodes.len() {
            if current == NONE || self.nodes[current as usize].kind != NodeKind::Symbol {
                break;
            }
            if self.is_inline_module(current) {
                return current;
            }
            current = self.nodes[current as usize].parent;
        }
        self.file_of(id)
    }

    fn parent_module(&self, module: u32) -> u32 {
        match self.nodes[module as usize].parent {
            NONE => self.file_of(module),
            parent => self.enclosing_module(parent),
        }
    }

    /// The inline module `name` declared directly in `module`.
    fn child_module(&self, module: u32, name: &str) -> Option<u32> {
        let node = &self.nodes[module as usize];
        let parent = if node.kind == NodeKind::Symbol {
            module
        } else {
            NONE
        };
        let file = self.scope.path(node.file)?;
        self.resolver
            .indexes
            .get(file)?
            .by_name
            .get(name)?
            .iter()
            .copied()
            .find(|id| self.nodes[*id as usize].parent == parent && self.is_inline_module(*id))
    }

    fn file_node(&self, path: &str) -> Option<u32> {
        let id = self
            .scope
            .file_paths
            .iter()
            .position(|file| *file == path)?;
        u32::try_from(id).ok()
    }
}
