//! Loading the context: tsconfig paths, workspace manifests, Python roots
//! and C/C++ include directories.
use super::*;

/// `-I` / `-iquote` directories named by compile_commands.json, rebased to
/// the scan root; directories outside the root are dropped.
pub(super) fn compile_command_include_dirs(
    root: &Path,
    file: &Path,
    paths: &PathPolicy,
) -> Vec<String> {
    let Ok(validated) = paths.validate_read(file) else {
        return Vec::new();
    };
    let Ok(bytes) =
        crate::tools::source::read_bounded(&validated.canonical, MAX_COMPILE_COMMANDS_BYTES)
    else {
        return Vec::new();
    };
    let Ok(Value::Array(commands)) = serde_json::from_slice::<Value>(&bytes) else {
        return Vec::new();
    };
    let mut dirs = Vec::new();
    let mut seen = HashSet::new();
    for command in commands {
        let directory = command
            .get("directory")
            .and_then(Value::as_str)
            .map_or_else(|| root.to_path_buf(), PathBuf::from);
        let arguments: Vec<String> = match command.get("arguments") {
            Some(Value::Array(items)) => items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect(),
            _ => command
                .get("command")
                .and_then(Value::as_str)
                .map(|line| line.split_whitespace().map(str::to_owned).collect())
                .unwrap_or_default(),
        };
        let mut iter = arguments.iter();
        while let Some(argument) = iter.next() {
            let value = if argument == "-I" || argument == "-iquote" {
                iter.next().cloned()
            } else if let Some(rest) = argument.strip_prefix("-I") {
                Some(rest.to_owned())
            } else {
                argument.strip_prefix("-iquote").map(str::to_owned)
            };
            let Some(value) = value.filter(|value| !value.is_empty()) else {
                continue;
            };
            let value = value.trim_matches('"');
            let Some(relative) = relative_to_root(root, &lexical(&directory.join(value))) else {
                continue;
            };
            if seen.insert(relative.clone()) {
                dirs.push(relative);
            }
        }
    }
    dirs
}

/// Package directories above a scan root searched for the root's name.
pub(super) const MAX_PYTHON_PACKAGE_DEPTH: usize = 16;

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

impl ResolveContext {
    pub(crate) fn load(
        root: &Path,
        known: &BTreeSet<String>,
        paths: &PathPolicy,
        security: &ContentSecurity,
    ) -> Self {
        let mut context = Self {
            root_names: root
                .components()
                .filter_map(|component| match component {
                    Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
                    _ => None,
                })
                .collect(),
            ..Self::default()
        };
        let mut js_dirs = BTreeSet::from([".".to_owned()]);
        let mut py_dirs = BTreeSet::new();
        let mut all_dirs = BTreeSet::from([".".to_owned()]);
        let mut has_c = false;
        for file in known {
            let linker = super::super::linking::Linker::of(file);
            let is_js = linker == super::super::linking::Linker::JsTs;
            let is_py = linker == super::super::linking::Linker::Python;
            has_c |= linker == super::super::linking::Linker::CFamily;
            let mut dir = dirname(file);
            loop {
                if !all_dirs.insert(dir.to_owned()) && !is_js && !is_py {
                    break;
                }
                if is_js {
                    js_dirs.insert(dir.to_owned());
                }
                if is_py {
                    py_dirs.insert(dir.to_owned());
                }
                if dir == "." {
                    break;
                }
                dir = dirname(dir);
            }
        }
        let mut reader = ConfigReader {
            root,
            paths,
            security,
            cache: HashMap::new(),
        };
        context.load_ts_configs(&mut reader, &js_dirs);
        context.load_manifests(&reader, &all_dirs);
        context.load_outer_manifest(&reader);
        if !py_dirs.is_empty() {
            context.load_python_roots(root, &py_dirs);
        }
        if has_c {
            context.load_include_dirs(root, paths);
            for file in known {
                let ext = file.rsplit_once('.').map_or("", |x| x.1);
                if matches!(
                    ext,
                    "h" | "hh" | "hpp" | "hxx" | "cuh" | "inc" | "c" | "cc" | "cpp" | "cxx"
                ) {
                    let name = file.rsplit('/').next().unwrap_or(file);
                    context
                        .headers_by_name
                        .entry(name.to_owned())
                        .or_default()
                        .push(file.clone());
                }
            }
        }
        context
    }

    pub(super) fn load_ts_configs(
        &mut self,
        reader: &mut ConfigReader<'_>,
        js_dirs: &BTreeSet<String>,
    ) {
        for dir in js_dirs {
            let directory = reader.root.join(dir);
            if let Some(config) = config_file_in(&directory) {
                self.ts_configs
                    .insert(dir.clone(), Arc::new(reader.effective(&config)));
            }
        }
        if self.ts_configs.contains_key(".") {
            return;
        }
        // A scan of a package subtree (`packages/app/src`) still honors the
        // package's own tsconfig above the root; stop at the project boundary.
        for directory in reader
            .root
            .ancestors()
            .skip(1)
            .take(MAX_OUTER_CONFIG_LEVELS)
        {
            if let Some(config) = config_file_in(directory) {
                self.outer_ts_config = Some(Arc::new(reader.effective(&config)));
                return;
            }
            if directory.join("package.json").is_file()
                || octocode_engine::search::repository::is_root(directory)
            {
                return;
            }
        }
    }

    pub(super) fn load_manifests(&mut self, reader: &ConfigReader<'_>, dirs: &BTreeSet<String>) {
        for dir in dirs {
            let Some(value) = reader.read_json(&reader.root.join(dir).join("package.json")) else {
                continue;
            };
            let manifest = Arc::new(Manifest::parse(dir, &value));
            if let Some(name) = value.get("name").and_then(Value::as_str) {
                self.packages.insert(name.to_owned(), Arc::clone(&manifest));
            }
            self.manifests.insert(dir.clone(), manifest);
        }
    }

    /// A scan of a package subtree (`packages/app/src`) still honors the
    /// package's own `package.json` above the root (`#` subpath imports,
    /// self-referencing exports), mirroring the outer tsconfig lookup: the
    /// first manifest within the bounded ancestor walk governs the scan root,
    /// its targets rebased onto the root; the walk stops at a repository
    /// root ([`octocode_engine::search::repository`]).
    pub(super) fn load_outer_manifest(&mut self, reader: &ConfigReader<'_>) {
        if self.manifests.contains_key(".") {
            return;
        }
        for directory in reader
            .root
            .ancestors()
            .skip(1)
            .take(MAX_OUTER_CONFIG_LEVELS)
        {
            let file = directory.join("package.json");
            if file.is_file() {
                let (Some(value), Ok(offset)) =
                    (reader.read_json(&file), reader.root.strip_prefix(directory))
                else {
                    return;
                };
                let offset = octocode_engine::text::slash_path(offset);
                let manifest = Arc::new(Manifest::parse(".", &value).rebased(&offset));
                if let Some(name) = value.get("name").and_then(Value::as_str) {
                    self.packages
                        .entry(name.to_owned())
                        .or_insert_with(|| Arc::clone(&manifest));
                }
                self.manifests.insert(".".to_owned(), manifest);
                return;
            }
            if octocode_engine::search::repository::is_root(directory) {
                return;
            }
        }
    }

    pub(super) fn load_python_roots(&mut self, root: &Path, py_dirs: &BTreeSet<String>) {
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
        self.python_roots = roots;
        self.python_package = enclosing_python_package(root);
    }

    pub(super) fn load_include_dirs(&mut self, root: &Path, paths: &PathPolicy) {
        self.include_dirs = [".", "include", "src"]
            .into_iter()
            .filter(|dir| *dir == "." || root.join(dir).is_dir())
            .map(str::to_owned)
            .collect();
        let mut system = Vec::new();
        let mut system_seen = HashSet::new();
        for candidate in ["compile_commands.json", "build/compile_commands.json"] {
            let file = root.join(candidate);
            if !file.is_file() {
                continue;
            }
            for dir in compile_command_include_dirs(root, &file, paths) {
                if system_seen.insert(dir.clone()) {
                    system.push(dir);
                }
            }
            break;
        }
        if root.join("include").is_dir() && system_seen.insert("include".to_owned()) {
            system.push("include".to_owned());
        }
        let mut included = self.include_dirs.iter().cloned().collect::<HashSet<_>>();
        for dir in &system {
            if included.insert(dir.clone()) {
                self.include_dirs.push(dir.clone());
            }
        }
        self.system_include_dirs = system;
    }
}
