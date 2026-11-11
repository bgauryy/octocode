//! Python module roots, one rule whatever the layout: a file's absolute imports
//! start from the nearest directory above it that is no package, which is where
//! `sys.path[0]` sits for a script and where pytest's `rootdir` import puts a
//! test. A package's own directory is never a root, so `import json` inside a
//! package holding a `json.py` still means the standard library.
use super::*;

/// Package directories above a scan root searched for the root's name.
pub(super) const MAX_PYTHON_PACKAGE_DEPTH: usize = 16;

/// What the scan says of Python modules, for one graph build.
#[derive(Debug, Default)]
pub(super) struct PythonModules {
    /// `.`, `src`, and project roots (pyproject/setup) with their `src`.
    pub(super) roots: Vec<String>,
    /// Scanned directories holding `__init__.py` or `__init__.pyi`.
    packages: HashSet<String>,
    /// A topmost package's name → the directories that hold one by that name.
    package_parents: HashMap<String, Vec<String>>,
    /// A module's own name → its scanned files.
    by_name: HashMap<String, Vec<String>>,
    /// Dotted package name of the scan root (`django` for a scan of
    /// `python/django/`), or its directory name when it holds no `__init__.py`.
    pub(super) root_package: Option<String>,
    /// The scan root holds an `__init__.py`, so its name is a package's.
    root_is_package: bool,
    /// The directories above the scan root, up to the repository boundary.
    outer: Vec<PathBuf>,
    /// Dotted module → whether a file above the scan root spells it.
    outer_hits: std::sync::Mutex<HashMap<String, bool>>,
}

/// The dotted package name of `root` when it is a Python package (holds
/// `__init__.py`): its directory name, prefixed by every enclosing package.
pub(super) fn enclosing_python_package(root: &Path) -> Option<String> {
    let is_package =
        |dir: &Path| dir.join("__init__.py").is_file() || dir.join("__init__.pyi").is_file();
    let mut names = Vec::new();
    let mut dir = root;
    while is_package(dir) && names.len() < MAX_PYTHON_PACKAGE_DEPTH {
        names.push(dir.file_name()?.to_string_lossy().into_owned());
        dir = dir.parent()?;
    }
    names.reverse();
    (!names.is_empty()).then(|| names.join("."))
}

/// The last component of a slash path.
fn file_name(path: &str) -> &str {
    path.rsplit_once('/').map_or(path, |(_, name)| name)
}

impl PythonModules {
    /// The index of the scanned Python files in `known`.
    pub(super) fn of<'k>(files: impl Iterator<Item = &'k String>) -> Self {
        let mut modules = Self::default();
        let mut scanned = Vec::new();
        for file in files {
            let name = file_name(file);
            if matches!(name, "__init__.py" | "__init__.pyi") {
                modules.packages.insert(dirname(file).to_owned());
            }
            scanned.push(file);
        }
        for dir in &modules.packages {
            let parent = dirname(dir);
            if dir != "." && !modules.packages.contains(parent) {
                let parents = modules
                    .package_parents
                    .entry(file_name(dir).to_owned())
                    .or_default();
                parents.push(parent.to_owned());
                parents.sort_unstable();
            }
        }
        for file in scanned {
            let name = file_name(file);
            let module = if matches!(name, "__init__.py" | "__init__.pyi") {
                file_name(dirname(file))
            } else {
                name.rsplit_once('.').map_or(name, |(stem, _)| stem)
            };
            modules
                .by_name
                .entry(module.to_owned())
                .or_default()
                .push(file.clone());
        }
        modules
    }

    /// The scan root's place: its package name and what lies above it.
    pub(super) fn at(mut self, root: &Path) -> Self {
        let enclosing = enclosing_python_package(root);
        self.root_is_package = enclosing.is_some();
        self.root_package = enclosing.or_else(|| {
            root.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        });
        self.outer = dirs_above_within_repository(root);
        self
    }

    /// The nearest directory above `importer` that is no package.
    fn import_root<'p>(&self, importer: &'p str) -> &'p str {
        let mut dir = dirname(importer);
        while dir != "." && self.packages.contains(dir) {
            dir = dirname(dir);
        }
        dir
    }

    /// The one directory holding a topmost package `spec` starts with, when
    /// that package could be what `spec` means: no second one competes, and a
    /// lone standard-library name stays the standard library's.
    fn sole_package_parent(&self, spec: &str) -> Option<&str> {
        let (head, rest) = spec.split_once('.').unwrap_or((spec, ""));
        if rest.is_empty() && is_standard_module(head) {
            return None;
        }
        match self.package_parents.get(head)?.as_slice() {
            [only] => Some(only),
            _ => None,
        }
    }

    /// Whether a scanned file could be the module `spec` with some directory
    /// on `sys.path`: its path ends in the module's, and no package holds that.
    fn scanned_module_matches(&self, spec: &str) -> bool {
        let tail = spec.replace('.', "/");
        let name = file_name(&tail);
        self.by_name.get(name).is_some_and(|files| {
            files.iter().any(|file| {
                [
                    format!("{tail}.py"),
                    format!("{tail}.pyi"),
                    format!("{tail}/__init__.py"),
                    format!("{tail}/__init__.pyi"),
                ]
                .iter()
                .any(|suffix| {
                    file.strip_suffix(suffix.as_str()).is_some_and(|above| {
                        above.is_empty()
                            || above
                                .strip_suffix('/')
                                .is_some_and(|dir| !self.packages.contains(dir))
                    })
                })
            })
        })
    }

    /// Whether a file above the scan root spells the module `spec`.
    fn outer_module_matches(&self, spec: &str) -> bool {
        if self.outer.is_empty() {
            return false;
        }
        let mut hits = self
            .outer_hits
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(known) = hits.get(spec) {
            return *known;
        }
        let tail = spec.replace('.', "/");
        let found = self.outer.iter().any(|dir| {
            let module = dir.join(&tail);
            module.with_added_extension("py").is_file()
                || module.with_added_extension("pyi").is_file()
                || module.join("__init__.py").is_file()
                || module.join("__init__.pyi").is_file()
        });
        hits.insert(spec.to_owned(), found);
        found
    }

    /// Whether the absolute import `spec`, which no root resolved, could still
    /// name code of this repository, so calling it external would be a guess.
    fn may_name_repository_code(&self, spec: &str) -> bool {
        let (head, rest) = spec.split_once('.').unwrap_or((spec, ""));
        let standard = is_standard_module(head);
        if (self.root_is_package || !standard)
            && self
                .root_package
                .as_deref()
                .and_then(|package| package.split('.').next())
                == Some(head)
        {
            return true;
        }
        if rest.is_empty() && standard {
            return false;
        }
        self.scanned_module_matches(spec) || self.outer_module_matches(spec)
    }
}

impl ResolveContext {
    pub(super) fn load_python_roots(
        &mut self,
        root: &Path,
        py_dirs: &BTreeSet<String>,
        py_files: PythonModules,
    ) {
        let mut roots = vec![".".to_owned()];
        let mut seen = HashSet::from([".".to_owned()]);
        if root.join("src").is_dir() {
            roots.push("src".to_owned());
            seen.insert("src".to_owned());
        }
        for dir in py_dirs {
            if dir == "." {
                continue;
            }
            let directory = root.join(dir);
            if ["pyproject.toml", "setup.py", "setup.cfg"]
                .iter()
                .any(|name| directory.join(name).is_file())
            {
                if seen.insert(dir.clone()) {
                    roots.push(dir.clone());
                }
                let src = join(dir, "src");
                if directory.join("src").is_dir() && seen.insert(src.clone()) {
                    roots.push(src);
                }
            }
        }
        self.python = PythonModules {
            roots,
            ..py_files.at(root)
        };
    }

    /// The scan-root-relative module for an absolute import that names the
    /// scan root itself: `django.utils.text` → `utils.text` under a scan of
    /// `django/`, and `""` for the package itself.
    pub(crate) fn python_package_local<'s>(&self, spec: &'s str) -> Option<&'s str> {
        let package = self.python.root_package.as_deref()?;
        if !self.python.root_is_package && is_standard_module(package) {
            return None;
        }
        if spec == package {
            return Some("");
        }
        spec.strip_prefix(package)?.strip_prefix('.')
    }

    /// Python import roots ordered for `importer`: its own import root and the
    /// project roots containing it (deepest first), then the scan root, then
    /// the rest.
    pub(crate) fn python_roots_for<'p>(&'p self, importer: &'p str) -> Vec<&'p str> {
        if self.python.roots.is_empty() {
            return vec!["."];
        }
        let own = self.python.import_root(importer);
        let mut containing = self
            .python
            .roots
            .iter()
            .filter(|root| {
                root.as_str() != "."
                    && importer
                        .strip_prefix(root.as_str())
                        .is_some_and(|rest| rest.starts_with('/'))
            })
            .map(String::as_str)
            .chain((own != ".").then_some(own))
            .collect::<Vec<_>>();
        containing.sort_by_key(|root| std::cmp::Reverse(root.len()));
        containing.dedup();
        let mut ordered = containing;
        let mut included = ordered.iter().copied().collect::<HashSet<_>>();
        ordered.push(".");
        included.insert(".");
        for root in &self.python.roots {
            if included.insert(root.as_str()) {
                ordered.push(root);
            }
        }
        ordered
    }

    /// The directory an absolute import may start from when none of
    /// [`Self::python_roots_for`] holds it: the parent of the one topmost
    /// package it names.
    pub(crate) fn python_package_parent(&self, spec: &str) -> Option<&str> {
        self.python.sole_package_parent(spec)
    }

    /// Whether the absolute import `spec`, which no root resolved, could still
    /// name code of this repository.
    pub(crate) fn python_may_be_internal(&self, spec: &str) -> bool {
        self.python.may_name_repository_code(spec)
    }
}

/// Whether `name` is a top-level module of the Python standard library, past
/// and present: such a name means the library unless the importer's own root
/// holds one. The names are generated, not written here
/// (`skills-dev/octocode-graph-dev/scripts/gen-python-stdlib.mjs`).
fn is_standard_module(name: &str) -> bool {
    static MODULES: std::sync::LazyLock<HashSet<&'static str>> = std::sync::LazyLock::new(|| {
        include_str!("python_stdlib.txt")
            .lines()
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .collect()
    });
    MODULES.contains(name)
}
