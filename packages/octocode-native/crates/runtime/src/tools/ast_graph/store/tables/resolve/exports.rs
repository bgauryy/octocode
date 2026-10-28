//! Export resolution through re-exports, and each file's import bindings.
use super::*;

/// A resolved binding retains unknown branches as well as observed targets.
#[derive(Clone)]
pub(crate) struct Resolved {
    pub(crate) targets: Vec<u32>,
    hopped: bool,
    pub(crate) uncertain: bool,
}

pub(crate) struct ExportResolver<'a> {
    pub(crate) built: &'a BuiltGraph,
    pub(crate) indexes: &'a BTreeMap<&'a str, FileIndex<'a>>,
    pub(crate) star_targets: &'a HashMap<&'a str, Vec<&'a str>>,
    /// file → name → resolution; call sites repeat the same imports. Keyed
    /// in two levels so a hit is looked up by borrowed strings.
    pub(crate) memo: std::cell::RefCell<HashMap<String, HashMap<String, Resolved>>>,
}

impl ExportResolver<'_> {
    pub(crate) fn exported(&self, file: &str, name: &str) -> (Vec<u32>, bool) {
        let resolved = self.resolved(file, name);
        (resolved.targets, resolved.hopped)
    }

    pub(crate) fn resolved(&self, file: &str, name: &str) -> Resolved {
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
    /// re-exports. Visiting each (file, name) once bounds cycles without
    /// silently dropping a target behind a long barrel chain.
    fn exported_uncached(&self, file: &str, name: &str) -> Resolved {
        let mut pending = vec![(file.to_owned(), name.to_owned(), 0usize)];
        let mut seen = BTreeSet::new();
        let mut found = Vec::new();
        let mut hopped = false;
        let mut uncertain = false;
        while let Some((file, name, depth)) = pending.pop() {
            if !seen.insert((file.clone(), name.clone())) {
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
                let rust = crate::tools::ast_graph::linking::Linker::of(&file)
                    == crate::tools::ast_graph::linking::Linker::Rust;
                if rust {
                    let local = facts
                        .declarations
                        .iter()
                        .filter(|decl| decl.name == name && decl.parent.is_none())
                        .filter_map(|decl| {
                            self.indexes
                                .get(file.as_str())?
                                .decl_nodes
                                .get(decl.id.as_str())
                                .copied()
                        })
                        .collect::<Vec<_>>();
                    if !local.is_empty() {
                        found.extend(local);
                        hopped |= depth > 0;
                        continue;
                    }
                }
                let mut explicit = false;
                for reexport in facts.reexports.iter().filter(|r| r.local_name == name) {
                    explicit = true;
                    if let Some(target) = &reexport.target {
                        pending.push((target.clone(), reexport.imported_name.clone(), depth + 1));
                    } else {
                        uncertain = true;
                    }
                }
                // An explicit export owns the name, even if its target is
                // outside the scan. Star exports cannot replace it.
                if explicit {
                    continue;
                }
                // A Rust `use` binds its names in the module, so `crate::a::f`
                // reaches what `a`'s `use b::f;` (or `use b::*;`) names; that
                // the path compiles is the proof the binding is visible.
                if rust {
                    let named = facts
                        .imports
                        .iter()
                        .filter(|import| {
                            import.imported_name != "*"
                                && import.local_name.as_deref() == Some(name.as_str())
                        })
                        .collect::<Vec<_>>();
                    if !named.is_empty() {
                        for import in named {
                            if let Some(target) = &import.target {
                                pending.push((
                                    target.clone(),
                                    import.imported_name.clone(),
                                    depth + 1,
                                ));
                            } else {
                                uncertain = true;
                            }
                        }
                        continue;
                    }
                    for import in &facts.imports {
                        if import.imported_name == "*" {
                            if let Some(target) = &import.target {
                                pending.push((target.clone(), name.clone(), depth + 1));
                            } else {
                                uncertain = true;
                            }
                        }
                    }
                }
            } else {
                uncertain = true;
            }
            if name != "default" {
                for target in self.star_targets.get(file.as_str()).into_iter().flatten() {
                    pending.push(((*target).to_owned(), name.clone(), depth + 1));
                }
            }
        }
        found.sort_unstable();
        found.dedup();
        Resolved {
            targets: found,
            hopped,
            uncertain,
        }
    }

    /// What `file` exports under `name` (following re-exports), else any
    /// declaration of that name in it: a module-qualified call reaches both.
    pub(crate) fn exported_or_declared(&self, file: &str, name: &str) -> Vec<u32> {
        let (exported, _) = self.exported(file, name);
        if exported.is_empty() {
            self.declared(file, name)
        } else {
            exported
        }
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

pub(crate) fn confidence_for(targets: &[u32], strong: Confidence) -> Confidence {
    if targets.len() == 1 {
        strong
    } else {
        Confidence::Medium.max(strong)
    }
}

/// Per-file import bindings, built once so each call site is O(1).
pub(crate) struct FileBindings<'a> {
    /// Local name → `(target file, name exported there)` for linked imports.
    pub(crate) named: HashMap<&'a str, Vec<(&'a str, &'a str)>>,
    /// Namespace/module binding (`ns` in `ns.f`, `pkg` in `pkg.F`) → files.
    pub(crate) modules: HashMap<&'a str, Vec<&'a str>>,
    /// Heads bound to third-party or standard packages.
    pub(crate) external: HashSet<String>,
    /// Files this file imports (scope for ambiguous names).
    pub(crate) imported_files: HashSet<&'a str>,
    /// Rust glob imports bind names directly, unlike JS namespace imports.
    pub(crate) wildcards: Vec<&'a str>,
    pub(crate) unknown_wildcard: bool,
}

impl<'a> FileBindings<'a> {
    pub(crate) fn new(facts: &'a FileFacts, language: &str) -> Self {
        let mut bindings = Self {
            named: HashMap::new(),
            modules: HashMap::new(),
            wildcards: Vec::new(),
            unknown_wildcard: false,
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
            if language == "rust" && import.imported_name == "*" {
                if let Some(target) = import.target.as_deref() {
                    bindings.wildcards.push(target);
                } else {
                    bindings.unknown_wildcard = true;
                }
            }
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
