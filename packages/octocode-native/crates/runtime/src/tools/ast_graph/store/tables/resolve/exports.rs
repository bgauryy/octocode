//! Export resolution through re-exports, and each file's import bindings.
use super::*;

/// Declarations a name resolves to, and whether a re-export hop was taken.
pub(in super::super) type Resolved = (Vec<u32>, bool);

pub(in super::super) struct ExportResolver<'a> {
    pub(in super::super) built: &'a BuiltGraph,
    pub(in super::super) indexes: &'a BTreeMap<&'a str, FileIndex<'a>>,
    pub(in super::super) star_targets: &'a HashMap<&'a str, Vec<&'a str>>,
    /// file → name → resolution; call sites repeat the same imports. Keyed
    /// in two levels so a hit is looked up by borrowed strings.
    pub(in super::super) memo: std::cell::RefCell<HashMap<String, HashMap<String, Resolved>>>,
}

impl ExportResolver<'_> {
    pub(in super::super) fn exported(&self, file: &str, name: &str) -> (Vec<u32>, bool) {
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
    pub(in super::super) fn exported_uncached(&self, file: &str, name: &str) -> (Vec<u32>, bool) {
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
    pub(in super::super) fn exported_or_declared(&self, file: &str, name: &str) -> Vec<u32> {
        let (exported, _) = self.exported(file, name);
        if exported.is_empty() {
            self.declared(file, name)
        } else {
            exported
        }
    }

    /// Any declaration named `name` in `file` (module-qualified calls such as
    /// Go `pkg.Func` or Rust `module::func` need not be re-exported).
    pub(in super::super) fn declared(&self, file: &str, name: &str) -> Vec<u32> {
        self.indexes
            .get(file)
            .and_then(|index| index.by_name.get(name))
            .cloned()
            .unwrap_or_default()
    }
}

pub(in super::super) fn confidence_for(targets: &[u32], strong: Confidence) -> Confidence {
    if targets.len() == 1 {
        strong
    } else {
        Confidence::Medium.max(strong)
    }
}

/// Per-file import bindings, built once so each call site is O(1).
pub(in super::super) struct FileBindings<'a> {
    /// Local name → `(target file, name exported there)` for linked imports.
    pub(in super::super) named: HashMap<&'a str, Vec<(&'a str, &'a str)>>,
    /// Namespace/module binding (`ns` in `ns.f`, `pkg` in `pkg.F`) → files.
    pub(in super::super) modules: HashMap<&'a str, Vec<&'a str>>,
    /// Heads bound to third-party or standard packages.
    pub(in super::super) external: HashSet<String>,
    /// Files this file imports (scope for ambiguous names).
    pub(in super::super) imported_files: HashSet<&'a str>,
}

impl<'a> FileBindings<'a> {
    pub(in super::super) fn new(facts: &'a FileFacts, language: &str) -> Self {
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
