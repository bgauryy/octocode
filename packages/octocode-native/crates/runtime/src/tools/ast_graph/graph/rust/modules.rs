//! Logical module ancestry derived from declarations, independently of paths.
use super::*;
use octocode_engine::graph::{GraphFactImport, GraphFactRustModule};

#[derive(Default)]
struct Module {
    file: String,
    parents: BTreeSet<usize>,
    children: BTreeMap<String, BTreeSet<usize>>,
    root: bool,
}

#[derive(Default)]
pub(in crate::tools::ast_graph::graph) struct RustModules {
    nodes: Vec<Module>,
    files: BTreeMap<String, usize>,
}

pub(in crate::tools::ast_graph::graph) enum ModuleResolution {
    OutsideModuleTree,
    Inside(Option<String>),
}

fn child_file(
    file: &str,
    module: &GraphFactRustModule,
    base: &str,
    known: &BTreeSet<String>,
) -> Option<String> {
    let scoped = module
        .scope
        .iter()
        .fold(base.to_owned(), |dir, name| join(&dir, name));
    if let Some(path) = &module.path {
        let dir = if module.scope.is_empty() {
            dirname(file)
        } else {
            &scoped
        };
        return join_within_root(dir, path).filter(|target| known.contains(target));
    }
    let stem = join(&scoped, &module.name);
    let candidates = [format!("{stem}.rs"), join(&stem, "mod.rs")];
    let mut found = candidates.into_iter().filter(|path| known.contains(path));
    let first = found.next()?;
    found.next().is_none().then_some(first)
}

impl RustModules {
    pub(in crate::tools::ast_graph::graph) fn build<'a>(
        entries: impl Iterator<Item = (&'a str, &'a [GraphFactRustModule])>,
        known: &BTreeSet<String>,
        cargo: &CargoCrates,
    ) -> Self {
        let entries = entries.collect::<Vec<_>>();
        let mut targeted = BTreeSet::new();
        let mut relocated = BTreeSet::new();
        let mut ordinary = BTreeSet::new();
        // Before choosing crate roots, conservatively exclude every file a
        // declaration can own. Cargo metadata supplies explicit target roots.
        for (file, modules) in &entries {
            for module in modules
                .iter()
                .filter(|module| !module.inline && !module.unsupported)
            {
                for base in [rust_module_child_dir(file), dirname(file).into()] {
                    if let Some(child) = child_file(file, module, &base, known) {
                        targeted.insert(child.clone());
                        if module.path.is_some() {
                            relocated.insert(child);
                        } else {
                            ordinary.insert(child);
                        }
                    }
                }
            }
        }
        let mut out = Self::default();
        for &(file, modules) in &entries {
            let name = file.rsplit('/').next().unwrap_or(file);
            let root = cargo.roots.contains(file)
                || (!targeted.contains(file)
                    && (matches!(name, "lib.rs" | "main.rs") || !modules.is_empty()));
            let id = out.nodes.len();
            out.files.insert(file.to_owned(), id);
            out.nodes.push(Module {
                file: file.to_owned(),
                root,
                ..Default::default()
            });
        }
        for (file, modules) in entries {
            let file_id = out.files[file];
            let mut bases = BTreeSet::new();
            if out.nodes[file_id].root || relocated.contains(file) {
                bases.insert(dirname(file).to_owned());
            }
            // A root that is neither a crate root nor a `lib`/`main`/`mod` file may be a
            // module whose parent is outside the scan (`a.rs` declaring `mod b;` in
            // `a/b.rs`): its children can live in its own module directory too.
            let name = file.rsplit('/').next().unwrap_or(file);
            let orphan = out.nodes[file_id].root
                && !cargo.roots.contains(file)
                && !matches!(name, "lib.rs" | "main.rs" | "mod.rs");
            if ordinary.contains(file) || bases.is_empty() || orphan {
                bases.insert(rust_module_child_dir(file));
            }
            for module in modules.iter().filter(|module| !module.unsupported) {
                let mut parent = file_id;
                for name in &module.scope {
                    parent = out.inline(parent, name);
                }
                if module.inline {
                    out.inline(parent, &module.name);
                } else {
                    for base in &bases {
                        if let Some(target) = child_file(file, module, base, known)
                            && let Some(&child) = out.files.get(&target)
                        {
                            out.nodes[parent]
                                .children
                                .entry(module.name.clone())
                                .or_default()
                                .insert(child);
                            out.nodes[child].parents.insert(parent);
                        }
                    }
                }
            }
        }
        out
    }

    fn inline(&mut self, parent: usize, name: &str) -> usize {
        if let Some(ids) = self.nodes[parent].children.get(name)
            && let Some(&id) = ids.first()
        {
            return id;
        }
        let id = self.nodes.len();
        self.nodes.push(Module {
            file: self.nodes[parent].file.clone(),
            parents: BTreeSet::from([parent]),
            ..Default::default()
        });
        self.nodes[parent]
            .children
            .insert(name.into(), BTreeSet::from([id]));
        id
    }

    fn context(&self, file: &str) -> Option<usize> {
        let &id = self.files.get(file)?;
        (self.nodes[id].root || !self.nodes[id].parents.is_empty()).then_some(id)
    }

    /// `super` from a module with no parent in the tree: a file whose parent
    /// module is outside the scan (a crate root cannot write `super`), so the
    /// directory resolver, which knows the snapshot root, takes over.
    fn super_from_parentless(&self, id: usize, first: &str) -> bool {
        first == "super" && self.nodes[id].parents.is_empty()
    }

    fn only(ids: &BTreeSet<usize>) -> Option<usize> {
        if ids.len() == 1 {
            ids.first().copied()
        } else {
            None
        }
    }

    fn scope(&self, mut id: usize, scope: &[String]) -> Option<usize> {
        for name in scope {
            id = Self::only(self.nodes[id].children.get(name)?)?;
        }
        Some(id)
    }

    fn target(&self, mut id: usize, spec: &str) -> Option<String> {
        let mut segments = spec.trim_end_matches(';').split("::").peekable();
        match segments.peek().copied()? {
            "crate" => {
                segments.next();
                let mut seen = BTreeSet::new();
                while !self.nodes[id].root {
                    if !seen.insert(id) {
                        return None;
                    }
                    id = Self::only(&self.nodes[id].parents)?;
                }
            }
            "self" => {
                segments.next();
            }
            "super" => {
                while segments.peek() == Some(&"super") {
                    segments.next();
                    id = Self::only(&self.nodes[id].parents)?;
                }
            }
            first => {
                id = Self::only(self.nodes[id].children.get(first)?)?;
                segments.next();
            }
        }
        for segment in segments {
            match self.nodes[id].children.get(segment) {
                Some(children) => id = Self::only(children)?,
                None => break, // The remaining segments name items within this module.
            }
        }
        Some(self.nodes[id].file.clone())
    }

    pub(in crate::tools::ast_graph::graph) fn import(
        &self,
        file: &str,
        import: &GraphFactImport,
        cargo: &CargoCrates,
    ) -> ModuleResolution {
        let Some(id) = self.context(file) else {
            return ModuleResolution::OutsideModuleTree;
        };
        let first = import.specifier.split("::").next().unwrap_or_default();
        if (first == "crate" && cargo.scan_is_submodule()) || self.super_from_parentless(id, first)
        {
            return ModuleResolution::OutsideModuleTree;
        }
        if import.import_kind != "module"
            && !matches!(first, "crate" | "self" | "super")
            && !self.nodes[id].children.contains_key(first)
        {
            return ModuleResolution::OutsideModuleTree;
        }
        ModuleResolution::Inside(
            self.scope(id, import.module_scope.as_deref().unwrap_or_default())
                .and_then(|id| {
                    if import.import_kind == "module" {
                        let child = Self::only(
                            self.nodes[id]
                                .children
                                .get(import.imported_name.as_deref()?)?,
                        )?;
                        Some(self.nodes[child].file.clone())
                    } else {
                        self.target(id, &import.specifier)
                    }
                }),
        )
    }

    pub(in crate::tools::ast_graph::graph) fn resolve(
        &self,
        spec: &str,
        file: &str,
        known: &BTreeSet<String>,
        cargo: &CargoCrates,
    ) -> Option<String> {
        let first = spec.split("::").next().unwrap_or_default();
        if first == "crate" && cargo.scan_is_submodule() {
            return resolve_rust(spec, file, known, cargo);
        }
        if let Some(source) = cargo.resolve(file, first) {
            if let Some(&id) = self.files.get(source) {
                return self.target(id, &format!("self{}", &spec[first.len()..]));
            }
        } else if let Some(id) = self.context(file)
            && !self.super_from_parentless(id, first)
        {
            return self.target(id, spec);
        }
        resolve_rust(spec, file, known, cargo)
    }
}
