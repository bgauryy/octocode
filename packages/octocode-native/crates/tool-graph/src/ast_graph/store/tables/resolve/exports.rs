//! Export resolution through re-exports, and each file's import bindings.
use super::*;
use octocode_engine::graph::COMMONJS_WHOLE_EXPORT;
use octocode_engine::text::analysis::ObservedSpan;

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
                // A CommonJS export written on one branch holds on some runs.
                uncertain |= self
                    .built
                    .facts
                    .get(&file)
                    .is_some_and(|facts| facts.conditional_exports);
                continue;
            }
            if let Some(facts) = self.built.facts.get(&file) {
                let linker = crate::ast_graph::linking::Linker::of_read(&file, &facts.language);
                let rust = linker == crate::ast_graph::linking::Linker::Rust;
                // A Python module attribute is any top-level definition or a
                // name its imports bind, as a Rust `use` binds one.
                let python = linker == crate::ast_graph::linking::Linker::Python;
                if rust || python {
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
                if rust || python {
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
                        if import.imported_name == "*"
                            && (rust || python_star(import) && !name.starts_with('_'))
                        {
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

/// `from m import *` (an `import m` binds the module, not its names).
fn python_star(import: &crate::ast_graph::types::Import) -> bool {
    import.imported_name == "*" && import.local_name.as_deref() == Some("*")
}

pub(crate) fn confidence_for(targets: &[u32], strong: Confidence) -> Confidence {
    if targets.len() == 1 {
        strong
    } else {
        Confidence::Medium.max(strong)
    }
}

/// An import as written; see [`FileBindings::written_imports`].
#[derive(Clone, Copy)]
pub(crate) struct WrittenImport<'a> {
    pub(crate) specifier: &'a str,
    pub(crate) scope: Option<&'a str>,
    pub(crate) alias: Option<&'a str>,
}

/// Per-file import bindings, built once so each call site is O(1).
pub(crate) struct FileBindings<'a> {
    /// Local name → `(target file, name exported there)` for linked imports.
    pub(crate) named: HashMap<&'a str, Vec<(&'a str, &'a str)>>,
    /// Namespace/module binding (`ns` in `ns.f`, `pkg` in `pkg.F`) → files.
    pub(crate) modules: HashMap<&'a str, Vec<&'a str>>,
    /// Heads bound to third-party or standard packages.
    pub(crate) external: HashSet<String>,
    /// Names bound by an import that did not link and is no proven package:
    /// what they call is unknown, not absent.
    pub(crate) unknown: HashSet<&'a str>,
    /// Files this file imports (scope for ambiguous names).
    pub(crate) imported_files: HashSet<&'a str>,
    /// Rust and Java static glob imports bind names directly, unlike JS
    /// namespace imports.
    pub(crate) wildcards: Vec<&'a str>,
    pub(crate) unknown_wildcard: bool,
    /// Python `from m import *` binds only `m`'s names without a leading
    /// underscore.
    pub(crate) star_public_only: bool,
    /// Names bound by a JS/TS import or a proven `require`: they resolve
    /// through the binding or not at all, never by a graph-wide name match.
    pub(crate) strict_names: HashSet<&'a str>,
    /// `require` bindings: a member call through one reaches only what its
    /// module exports (`file.copy` → `module.exports = { copy }`).
    pub(crate) commonjs_locals: HashSet<&'a str>,
    /// Whole-module `require` bindings → linked files.
    pub(crate) commonjs_modules: HashMap<&'a str, Vec<&'a str>>,
    /// `require` bindings that hold over one scope extent only (a function
    /// or block, or the one call of an inline `require(x).k()`).
    pub(crate) scoped_requires: Vec<ScopedRequire<'a>>,
    /// Scala, Java and C# imports as written (`cats.data.{A, B => C}`,
    /// `static a.b.C.*`, `Acme.Util`), each with the declaration whose body
    /// holds it (`None` at file level) and its alias (C# `using H = …`).
    pub(crate) written_imports: Vec<WrittenImport<'a>>,
}

impl<'a> FileBindings<'a> {
    pub(crate) fn new(facts: &'a FileFacts, language: &str) -> Self {
        let mut bindings = Self {
            named: HashMap::new(),
            modules: HashMap::new(),
            wildcards: Vec::new(),
            unknown_wildcard: false,
            unknown: HashSet::new(),
            star_public_only: language == "python",
            strict_names: HashSet::new(),
            commonjs_locals: HashSet::new(),
            commonjs_modules: HashMap::new(),
            scoped_requires: Vec::new(),
            written_imports: if matches!(language, "scala" | "java" | "csharp") {
                facts
                    .imports
                    .iter()
                    .map(|import| WrittenImport {
                        specifier: import.specifier.as_str(),
                        scope: import.scope_id.as_deref(),
                        alias: import.local_name.as_deref(),
                    })
                    .collect()
            } else {
                Vec::new()
            },
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
            // Rust globs and Java static globs bind the target's names
            // directly, unlike JS namespace imports.
            if (matches!(language, "rust" | "java" | "go") && import.imported_name == "*")
                || (language == "python" && python_star(import))
            {
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
                None if links_imports(language) => {
                    bindings.unknown.extend(module_binding);
                    if let Some(local) = import.local_name.as_deref().filter(|l| !l.is_empty()) {
                        bindings.unknown.insert(local);
                    }
                    if !import.imported_name.is_empty() && import.imported_name != "*" {
                        bindings.unknown.insert(import.imported_name.as_str());
                    }
                }
                None => {}
            }
        }
        if ecosystem(language) == "npm" {
            bindings.strict_names.extend(bindings.named.keys().copied());
        }
        for binding in &facts.commonjs {
            bindings.add_require(binding, language);
        }
        bindings
    }

    /// One proven `require` binding: a scope binding holds over its extent
    /// only; a module binding resolves like an import, across the file.
    fn add_require(
        &mut self,
        binding: &'a crate::ast_graph::types::CommonJsBinding,
        language: &str,
    ) {
        let import = &binding.import;
        let Some(local) = import.local_name.as_deref() else {
            return;
        };
        if let Some(scope) = &binding.scope {
            self.scoped_requires.push(ScopedRequire {
                local,
                scope,
                target: import.target.as_deref(),
                imported: import.imported_name.as_str(),
            });
            return;
        }
        self.commonjs_locals.insert(local);
        self.strict_names.insert(local);
        let whole = import.imported_name == COMMONJS_WHOLE_EXPORT;
        match &import.target {
            Some(target) => {
                self.named
                    .entry(local)
                    .or_default()
                    .push((target.as_str(), import.imported_name.as_str()));
                if whole {
                    self.commonjs_modules
                        .entry(local)
                        .or_default()
                        .push(target.as_str());
                }
            }
            None if import.external => {
                self.external.insert(local.to_owned());
                self.external
                    .extend(package_name(&import.specifier, language));
            }
            None if links_imports(language) => {
                self.unknown.insert(local);
            }
            None => {}
        }
    }

    /// The innermost scope `require` whose extent holds the start of `span`
    /// (a call starts at its callee, which an inline load's extent is) and
    /// binds the call's head: the bare name, the whole qualifier
    /// (`require()`), or its first segment (`m` of `m.a.f()`).
    pub(crate) fn scoped_require(
        &self,
        qualifier: &str,
        name: &str,
        span: &ObservedSpan,
    ) -> Option<&ScopedRequire<'a>> {
        let head = if qualifier.is_empty() {
            name
        } else {
            first_segment(qualifier)
        };
        self.scoped_requires
            .iter()
            .filter(|binding| binding.local == head || binding.local == qualifier)
            .filter(|binding| {
                binding.scope.unit == span.unit
                    && binding.scope.range.start_byte <= span.range.start_byte
                    && span.range.start_byte < binding.scope.range.end_byte
            })
            .min_by_key(|binding| binding.scope.range.end_byte - binding.scope.range.start_byte)
    }
}

/// A `require` binding that holds over one scope extent.
pub(crate) struct ScopedRequire<'a> {
    pub(crate) local: &'a str,
    pub(crate) scope: &'a ObservedSpan,
    /// The file the load reaches; `None` for a package or an unlinked path.
    pub(crate) target: Option<&'a str>,
    /// The property bound, else `module.exports`.
    pub(crate) imported: &'a str,
}

/// Whether the graph links `language`'s imports to files, so an unlinked one is
/// a gap; Scala and C# imports are not linked at all.
fn links_imports(language: &str) -> bool {
    matches!(ecosystem(language), "npm" | "pypi" | "cargo" | "go") || language == "java"
}
