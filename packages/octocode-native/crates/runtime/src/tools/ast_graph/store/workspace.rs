//! The project model around the parsed files: build manifests (components),
//! their declared dependencies, and inferred entrypoints.
//!
//! Manifests are discovered only on ancestors of scanned files, so excluded
//! trees (`node_modules`, build output) are never walked. Everything here is
//! best effort: an unreadable or malformed manifest contributes nothing and
//! never fails ingest.
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// One build unit: an npm package, Cargo crate, Go module, or Python project.
#[derive(Clone, Debug, Default)]
pub(crate) struct Component {
    /// Root-relative directory (`""` is the scan root).
    pub dir: String,
    pub name: String,
    pub ecosystem: &'static str,
    /// Runtime dependencies (normalized import names).
    pub deps: BTreeSet<String>,
    /// Development-only dependencies.
    pub dev_deps: BTreeSet<String>,
    /// Whether `deps` is authoritative enough to call a miss "undeclared".
    pub complete: bool,
    /// A distributable library whose modules are all importable by
    /// consumers (a Python package with no app entry markers).
    pub library: bool,
    /// Templates (MDX, Vue, Svelte, Astro) may import code the graph cannot
    /// see, so reachability here is weaker evidence.
    pub templates: bool,
}

impl Component {
    /// `ecosystem[;library][;templates]`, persisted per file.
    pub(crate) fn meta(&self) -> String {
        let mut meta = self.ecosystem.to_owned();
        if self.library {
            meta.push_str(";library");
        }
        if self.templates {
            meta.push_str(";templates");
        }
        meta
    }
}

#[derive(Debug, Default)]
pub(crate) struct Workspace {
    /// Components by directory, deepest first when iterated in reverse.
    /// Components by directory; one directory may hold several ecosystems
    /// (a Python package with a `package.json` for its JS assets).
    pub components: BTreeMap<String, Vec<Component>>,
    /// Root-relative files inferred as production entrypoints, with the rule
    /// that produced each one.
    pub entries: BTreeMap<String, &'static str>,
}

/// How a file→package import stands against its component's manifest.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Declared {
    Runtime,
    Dev,
    Builtin,
    /// Declared only by another workspace package (a hoisted phantom).
    Hoisted,
    /// Imported only from an in-file unit-test module.
    TestScoped,
    Undeclared,
    Unknown,
}

impl Declared {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Runtime => "external",
            Self::Dev => "external-dev",
            Self::Builtin => "external-builtin",
            Self::Hoisted => "external-hoisted",
            Self::TestScoped => "external-test",
            Self::Undeclared => "external-undeclared",
            Self::Unknown => "external-unknown",
        }
    }
}

const NODE_BUILTINS: &[&str] = &[
    "assert",
    "async_hooks",
    "buffer",
    "child_process",
    "cluster",
    "console",
    "constants",
    "crypto",
    "dgram",
    "diagnostics_channel",
    "dns",
    "domain",
    "events",
    "fs",
    "http",
    "http2",
    "https",
    "inspector",
    "module",
    "net",
    "os",
    "path",
    "perf_hooks",
    "process",
    "punycode",
    "querystring",
    "readline",
    "repl",
    "stream",
    "string_decoder",
    "sys",
    "test",
    "timers",
    "tls",
    "trace_events",
    "tty",
    "url",
    "util",
    "v8",
    "vm",
    "wasi",
    "worker_threads",
    "zlib",
];
const RUST_BUILTINS: &[&str] = &["std", "core", "alloc", "proc_macro", "test"];
const JS_EXTENSIONS: &[&str] = &["ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs"];

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

fn read_toml(path: &Path) -> Option<toml::Table> {
    std::fs::read_to_string(path)
        .ok()?
        .parse::<toml::Table>()
        .ok()
}

fn join(dir: &str, file: &str) -> String {
    let file = file.trim_start_matches("./");
    if dir.is_empty() {
        file.to_owned()
    } else {
        format!("{dir}/{file}")
    }
}

fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(dir, _)| dir)
}

/// PEP 508 requirement → import-ish name (`Django>=4` → `django`).
fn python_requirement(spec: &str) -> Option<String> {
    let name = spec
        .trim()
        .split(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')))
        .next()?
        .to_ascii_lowercase()
        .replace('-', "_");
    (!name.is_empty() && !name.starts_with('#')).then_some(name)
}

fn npm_component(dir: &str, manifest: &Value) -> Component {
    let mut component = Component {
        dir: dir.to_owned(),
        name: manifest["name"].as_str().unwrap_or(dir).to_owned(),
        ecosystem: "npm",
        complete: true,
        ..Default::default()
    };
    const TEMPLATE_DEPS: &[&str] = &[
        "@docusaurus/core",
        "@mdx-js/react",
        "@mdx-js/loader",
        "@next/mdx",
        "vue",
        "svelte",
        "astro",
        "vitepress",
        "nuxt",
        "@nuxt/content",
        "@storybook/react",
    ];
    component.templates = ["dependencies", "devDependencies", "peerDependencies"]
        .iter()
        .any(|field| {
            manifest[*field]
                .as_object()
                .is_some_and(|deps| TEMPLATE_DEPS.iter().any(|d| deps.contains_key(*d)))
        });
    for (field, dev) in [
        ("dependencies", false),
        ("peerDependencies", false),
        ("optionalDependencies", false),
        ("devDependencies", true),
    ] {
        for name in manifest[field]
            .as_object()
            .into_iter()
            .flatten()
            .map(|(k, _)| k)
        {
            let set = if dev {
                &mut component.dev_deps
            } else {
                &mut component.deps
            };
            set.insert(name.clone());
            // `@types/x` provides types for `x`.
            if let Some(typed) = name.strip_prefix("@types/") {
                let target = match typed.split_once("__") {
                    Some((scope, pkg)) => format!("@{scope}/{pkg}"),
                    None => typed.to_owned(),
                };
                component.dev_deps.insert(target);
            }
        }
    }
    component
}

fn cargo_component(dir: &str, manifest: &toml::Table) -> Component {
    let name = manifest
        .get("package")
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or(dir)
        .replace('-', "_");
    let mut component = Component {
        dir: dir.to_owned(),
        name,
        ecosystem: "cargo",
        complete: manifest.contains_key("package"),
        ..Default::default()
    };
    let mut tables = vec![(manifest.clone(), false)];
    if let Some(targets) = manifest.get("target").and_then(|t| t.as_table()) {
        for target in targets.values().filter_map(|t| t.as_table()) {
            tables.push((target.clone(), false));
        }
    }
    for (table, _) in tables {
        for (field, dev) in [
            ("dependencies", false),
            ("build-dependencies", false),
            ("dev-dependencies", true),
        ] {
            // `foo = { package = "real-name" }` imports as `foo`: keys suffice.
            for key in table
                .get(field)
                .and_then(|d| d.as_table())
                .into_iter()
                .flat_map(|t| t.keys())
            {
                let set = if dev {
                    &mut component.dev_deps
                } else {
                    &mut component.deps
                };
                set.insert(key.replace('-', "_"));
            }
        }
    }
    component
}

fn python_component(dir: &str, root: &Path) -> Option<Component> {
    let base = root.join(dir);
    let mut component = Component {
        dir: dir.to_owned(),
        name: dir.rsplit('/').next().unwrap_or(dir).to_owned(),
        ecosystem: "pypi",
        ..Default::default()
    };
    let mut found = false;
    if let Some(pyproject) = read_toml(&base.join("pyproject.toml")) {
        found = true;
        let project = pyproject.get("project");
        if let Some(name) = project.and_then(|p| p.get("name")).and_then(|n| n.as_str()) {
            component.name = name.to_owned();
        }
        for spec in project
            .and_then(|p| p.get("dependencies"))
            .and_then(|d| d.as_array())
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str())
        {
            component.deps.extend(python_requirement(spec));
        }
        for group in project
            .and_then(|p| p.get("optional-dependencies"))
            .and_then(|d| d.as_table())
            .into_iter()
            .flat_map(|t| t.values())
            .filter_map(|v| v.as_array())
        {
            for spec in group.iter().filter_map(|v| v.as_str()) {
                component.dev_deps.extend(python_requirement(spec));
            }
        }
        let poetry = pyproject.get("tool").and_then(|t| t.get("poetry"));
        for key in poetry
            .and_then(|p| p.get("dependencies"))
            .and_then(|d| d.as_table())
            .into_iter()
            .flat_map(|t| t.keys())
        {
            component.deps.extend(python_requirement(key));
        }
    }
    for requirements in [
        "requirements.txt",
        "requirements-dev.txt",
        "dev-requirements.txt",
    ] {
        if let Ok(text) = std::fs::read_to_string(base.join(requirements)) {
            found = true;
            let dev = requirements.contains("dev");
            for line in text
                .lines()
                .filter(|l| !l.trim_start().starts_with(['#', '-']))
            {
                let set = if dev {
                    &mut component.dev_deps
                } else {
                    &mut component.deps
                };
                set.extend(python_requirement(line));
            }
        }
    }
    let distributable = base.join("setup.py").is_file()
        || base.join("setup.cfg").is_file()
        || base.join("pyproject.toml").is_file();
    found |= distributable;
    let app = [
        "manage.py",
        "__main__.py",
        "main.py",
        "app.py",
        "wsgi.py",
        "asgi.py",
    ]
    .iter()
    .any(|marker| base.join(marker).is_file())
        || read_toml(&base.join("pyproject.toml")).is_some_and(|p| {
            p.get("project").and_then(|x| x.get("scripts")).is_some()
                || p.get("tool")
                    .and_then(|t| t.get("poetry"))
                    .and_then(|x| x.get("scripts"))
                    .is_some()
        });
    component.library = distributable && !app;
    // Python distribution names rarely equal import names (`PyYAML` →
    // `yaml`), so a miss is never claimed as undeclared.
    component.complete = false;
    found.then_some(component)
}

fn go_component(dir: &str, root: &Path) -> Option<Component> {
    let text = std::fs::read_to_string(root.join(dir).join("go.mod")).ok()?;
    let mut component = Component {
        dir: dir.to_owned(),
        ecosystem: "go",
        complete: true,
        ..Default::default()
    };
    let mut in_require = false;
    for line in text.lines().map(str::trim) {
        if let Some(module) = line.strip_prefix("module ") {
            component.name = module.trim().to_owned();
        } else if line.starts_with("require (") {
            in_require = true;
        } else if in_require && line == ")" {
            in_require = false;
        } else if let Some(spec) = line.strip_prefix("require ").or(in_require.then_some(line))
            && let Some(path) = spec.split_whitespace().next().filter(|p| !p.is_empty())
        {
            component.deps.insert(path.to_owned());
        }
    }
    Some(component)
}

impl Workspace {
    /// Discovers manifests on the ancestors of `files` (root-relative) and
    /// infers entrypoints.
    /// `mains` are files that declare a `main`/`Main` function or method
    /// (from parsed facts, so JVM/.NET entries need no file reads).
    pub(crate) fn discover(root: &Path, files: &[&str], mains: &BTreeSet<&str>) -> Self {
        let mut workspace = Self::default();
        let mut dirs = BTreeSet::new();
        for file in files {
            let mut dir = parent(file);
            loop {
                if !dirs.insert(dir.to_owned()) {
                    break;
                }
                if dir.is_empty() {
                    break;
                }
                dir = parent(dir);
            }
        }
        for dir in &dirs {
            let base = root.join(dir);
            let mut found = Vec::new();
            if let Some(manifest) = read_json(&base.join("package.json")) {
                workspace.npm_entries(root, dir, &manifest, files);
                found.push(npm_component(dir, &manifest));
            }
            if let Some(manifest) = read_toml(&base.join("Cargo.toml")) {
                workspace.cargo_entries(dir, &manifest, files);
                found.push(cargo_component(dir, &manifest));
            }
            found.extend(go_component(dir, root));
            found.extend(python_component(dir, root));
            if !found.is_empty() {
                workspace.components.insert(dir.clone(), found);
            }
        }
        workspace.convention_entries(root, files, mains);
        workspace
    }

    /// The declared Go module (longest `require` path) that provides the
    /// import path `spec`.
    pub(crate) fn go_module(&self, spec: &str) -> Option<&str> {
        self.all()
            .filter(|c| c.ecosystem == "go")
            .flat_map(|c| c.deps.iter())
            .filter(|module| spec == module.as_str() || spec.starts_with(&format!("{module}/")))
            .max_by_key(|module| module.len())
            .map(String::as_str)
    }

    fn all(&self) -> impl Iterator<Item = &Component> {
        self.components.values().flatten()
    }

    /// The deepest component containing `file`, preferring one of the
    /// file's own ecosystem when a directory holds several.
    pub(crate) fn component_of(&self, file: &str, ecosystem: &str) -> Option<&Component> {
        let mut dir = parent(file);
        loop {
            if let Some(list) = self.components.get(dir) {
                return list
                    .iter()
                    .find(|c| c.ecosystem == ecosystem)
                    .or_else(|| list.first());
            }
            if dir.is_empty() {
                return None;
            }
            dir = parent(dir);
        }
    }

    /// Classifies an external package imported from `file`.
    pub(crate) fn declared(&self, file: &str, package: &str, ecosystem: &str) -> Declared {
        if ecosystem == "go" && self.go_module(package).is_some() {
            return Declared::Runtime;
        }
        match ecosystem {
            "npm" if package.starts_with("node:") || NODE_BUILTINS.contains(&package) => {
                return Declared::Builtin;
            }
            "npm" if package.starts_with("bun:") || package.starts_with("deno:") => {
                return Declared::Builtin;
            }
            "cargo" if RUST_BUILTINS.contains(&package) => return Declared::Builtin,
            "go" if !package
                .split('/')
                .next()
                .is_some_and(|host| host.contains('.')) =>
            {
                return Declared::Builtin;
            }
            "system" => return Declared::Builtin,
            _ => {}
        }
        // Walk outward: a workspace member may rely on the root's deps.
        let mut dir = parent(file);
        let mut complete = false;
        loop {
            if let Some(component) = self
                .components
                .get(dir)
                .and_then(|list| list.iter().find(|c| c.ecosystem == ecosystem))
            {
                if component.name == package {
                    return Declared::Runtime;
                }
                if component.deps.contains(package) {
                    return Declared::Runtime;
                }
                if component.dev_deps.contains(package) {
                    return Declared::Dev;
                }
                complete |= component.complete;
            }
            if dir.is_empty() {
                break;
            }
            dir = parent(dir);
        }
        // Sibling workspace packages are internal, not undeclared.
        if self.all().any(|c| c.name == package) {
            return Declared::Runtime;
        }
        if ecosystem == "go"
            && self.all().any(|c| {
                c.ecosystem == "go"
                    && (package == c.name || package.starts_with(&format!("{}/", c.name)))
            })
        {
            return Declared::Runtime;
        }
        // Cargo and Go refuse to build an undeclared dependency, so an
        // unknown root there is a local module or a parse artifact.
        if matches!(ecosystem, "cargo" | "go") {
            return Declared::Unknown;
        }
        if ecosystem == "npm" {
            // Framework/bundler aliases, not packages.
            if [
                "@site/",
                "@theme/",
                "@theme-original/",
                "@generated/",
                "$lib",
                "$app/",
                "virtual:",
                "astro:",
                "~/",
                "@/",
            ]
            .iter()
            .any(|prefix| package.starts_with(prefix) || package == prefix.trim_end_matches('/'))
            {
                return Declared::Unknown;
            }
            if self.all().any(|c| {
                c.ecosystem == "npm" && (c.deps.contains(package) || c.dev_deps.contains(package))
            }) {
                return Declared::Hoisted;
            }
        }
        if complete {
            Declared::Undeclared
        } else {
            Declared::Unknown
        }
    }

    fn add_entry(&mut self, file: &str, rule: &'static str, files: &[&str]) {
        let file = file.trim_start_matches("./").to_owned();
        if files.contains(&file.as_str()) {
            self.entries.entry(file).or_insert(rule);
            return;
        }
        // Build output → source (`dist/index.js` → `src/index.ts`).
        let (stem, _) = file.rsplit_once('.').unwrap_or((&file, ""));
        let mut stems = vec![stem.to_owned()];
        for out in ["dist/", "build/", "out/", "lib/", "esm/", "cjs/"] {
            if let Some(pos) = stem.find(out)
                && (pos == 0 || stem.as_bytes()[pos - 1] == b'/')
            {
                stems.push(format!("{}src/{}", &stem[..pos], &stem[pos + out.len()..]));
                stems.push(format!("{}{}", &stem[..pos], &stem[pos + out.len()..]));
            }
        }
        for candidate in &stems {
            for ext in JS_EXTENSIONS {
                let path = format!("{candidate}.{ext}");
                if files.contains(&path.as_str()) {
                    self.entries.entry(path).or_insert(rule);
                    return;
                }
            }
        }
    }

    fn npm_entries(&mut self, root: &Path, dir: &str, manifest: &Value, files: &[&str]) {
        fn leaves(value: &Value, out: &mut Vec<String>) {
            match value {
                Value::String(s) => out.push(s.clone()),
                Value::Object(map) => map.values().for_each(|v| leaves(v, out)),
                Value::Array(items) => items.iter().for_each(|v| leaves(v, out)),
                _ => {}
            }
        }
        let mut targets = Vec::new();
        for field in ["main", "module", "browser", "bin", "exports", "source"] {
            leaves(&manifest[field], &mut targets);
        }
        for target in targets
            .iter()
            .filter(|t| !t.contains('*') && !t.ends_with(".d.ts"))
        {
            self.add_entry(&join(dir, target), "package.json", files);
        }
        for script in manifest["scripts"]
            .as_object()
            .into_iter()
            .flatten()
            .filter_map(|(_, v)| v.as_str())
        {
            for token in script.split_whitespace() {
                let token = token.trim_matches(['"', '\'']);
                if token
                    .rsplit_once('.')
                    .is_some_and(|(_, ext)| JS_EXTENSIONS.contains(&ext))
                    && root.join(dir).join(token).is_file()
                {
                    self.add_entry(&join(dir, token), "package.json scripts", files);
                }
            }
        }
        for name in ["index", "main", "cli", "server", "app"] {
            for base in ["", "src/"] {
                for ext in JS_EXTENSIONS {
                    let candidate = join(dir, &format!("{base}{name}.{ext}"));
                    if files.contains(&candidate.as_str()) {
                        self.entries
                            .entry(candidate)
                            .or_insert("default entry name");
                    }
                }
            }
        }
    }

    fn cargo_entries(&mut self, dir: &str, manifest: &toml::Table, files: &[&str]) {
        for file in ["src/main.rs", "src/lib.rs", "build.rs"] {
            self.add_entry(&join(dir, file), "cargo target", files);
        }
        for section in ["bin", "example", "test", "bench"] {
            for target in manifest
                .get(section)
                .and_then(|t| t.as_array())
                .into_iter()
                .flatten()
            {
                if let Some(path) = target.get("path").and_then(|p| p.as_str()) {
                    self.add_entry(&join(dir, path), "cargo target", files);
                }
            }
        }
        if let Some(path) = manifest
            .get("lib")
            .and_then(|l| l.get("path"))
            .and_then(|p| p.as_str())
        {
            self.add_entry(&join(dir, path), "cargo target", files);
        }
        let prefix = if dir.is_empty() {
            String::new()
        } else {
            format!("{dir}/")
        };
        for file in files {
            let Some(rest) = file.strip_prefix(&prefix) else {
                continue;
            };
            let auto = ["src/bin/", "examples/", "tests/", "benches/"]
                .iter()
                .any(|d| {
                    rest.strip_prefix(d)
                        .is_some_and(|tail| !tail.contains('/') || tail.ends_with("/main.rs"))
                });
            if auto && rest.ends_with(".rs") {
                self.entries
                    .entry((*file).to_owned())
                    .or_insert("cargo auto-target");
            }
        }
    }

    /// Framework routing conventions and language-level `main`s.
    fn convention_entries(&mut self, root: &Path, files: &[&str], mains: &BTreeSet<&str>) {
        const ROUTE_FILES: &[&str] = &[
            "page",
            "layout",
            "route",
            "loading",
            "error",
            "not-found",
            "template",
            "default",
            "global-error",
            "head",
            "opengraph-image",
            "sitemap",
            "robots",
        ];
        for file in files {
            let segments = file.split('/').collect::<Vec<_>>();
            let name = segments.last().copied().unwrap_or_default();
            let (stem, ext) = name.rsplit_once('.').unwrap_or((name, ""));
            let js = JS_EXTENSIONS.contains(&ext);
            let under = |dir: &str| segments.iter().rev().skip(1).any(|s| *s == dir);
            let rule = if js && under("pages") {
                Some("framework route (pages/)")
            } else if js && under("app") && ROUTE_FILES.contains(&stem) {
                Some("framework route (app/)")
            } else if js
                && (under("routes")
                    || stem == "+page"
                    || stem == "+server"
                    || stem.starts_with("+layout"))
            {
                Some("framework route (routes/)")
            } else if js
                && matches!(
                    stem,
                    "middleware"
                        | "instrumentation"
                        | "entry.client"
                        | "entry.server"
                        | "root"
                        | "_app"
                        | "_document"
                )
            {
                Some("framework convention")
            } else if js && (under("api") && under("server")) {
                Some("framework route (server/api)")
            } else {
                None
            };
            if let Some(rule) = rule {
                self.entries.entry((*file).to_owned()).or_insert(rule);
                continue;
            }
            // Executables run as processes (`node x.mjs`, `./tool.py`), not
            // imported: a shebang or a bin/scripts directory makes a root.
            let in_script_dir = segments
                .iter()
                .rev()
                .skip(1)
                .any(|s| matches!(*s, "bin" | "scripts" | "script"));
            if in_script_dir
                && matches!(
                    ext,
                    "js" | "mjs" | "cjs" | "ts" | "mts" | "py" | "rb" | "sh"
                )
            {
                self.entries
                    .entry((*file).to_owned())
                    .or_insert("script directory");
                continue;
            }
            if matches!(ext, "js" | "mjs" | "cjs" | "ts" | "mts" | "py")
                && let Ok(mut handle) = std::fs::File::open(root.join(file))
            {
                let mut head = [0u8; 2];
                if std::io::Read::read_exact(&mut handle, &mut head).is_ok() && &head == b"#!" {
                    self.entries
                        .entry((*file).to_owned())
                        .or_insert("executable script");
                    continue;
                }
            }
            // C-family and assembly sources are linker inputs, never
            // `#include`d: each translation unit is a root, so only headers
            // can be unreachable.
            if matches!(
                ext,
                "c" | "cc" | "cpp" | "cxx" | "cu" | "m" | "mm" | "s" | "asm" | "S"
            ) {
                self.entries
                    .entry((*file).to_owned())
                    .or_insert("compilation unit");
                continue;
            }
            let needle: Option<(&str, &'static str)> = match ext {
                "go" => Some(("package main", "go package main")),
                "py" => {
                    if matches!(name, "__main__.py" | "manage.py" | "wsgi.py" | "asgi.py") {
                        self.entries
                            .entry((*file).to_owned())
                            .or_insert("python entry name");
                        continue;
                    }
                    Some(("__name__ == \"__main__\"", "python __main__ guard"))
                }
                "java" | "kt" | "scala" | "cs" => {
                    if mains.contains(file) {
                        let rule = if ext == "cs" {
                            "dotnet Main"
                        } else {
                            "jvm main"
                        };
                        self.entries.entry((*file).to_owned()).or_insert(rule);
                    }
                    continue;
                }
                _ => None,
            };
            let Some((needle, rule)) = needle else {
                continue;
            };
            // `package main` is the first clause of a Go file; Python's
            // `__main__` guard sits anywhere, usually at the end.
            let limit = if ext == "go" { 4096 } else { 1 << 20 };
            let Ok(handle) = std::fs::File::open(root.join(file)) else {
                continue;
            };
            let mut bytes = Vec::new();
            if std::io::Read::read_to_end(&mut std::io::Read::take(handle, limit), &mut bytes)
                .is_err()
            {
                continue;
            }
            let text = String::from_utf8_lossy(&bytes);
            let hit = match ext {
                "py" => text.contains(needle) || text.contains("__name__ == '__main__'"),
                _ => text.contains(needle),
            };
            if hit {
                self.entries.entry((*file).to_owned()).or_insert(rule);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifests_yield_components_dependencies_and_entries() {
        let dir = tempfile::tempdir().expect("dir");
        let root = dir.path();
        std::fs::create_dir_all(root.join("packages/app/src/pages")).expect("dirs");
        std::fs::create_dir_all(root.join("crates/core/src/bin")).expect("dirs");
        std::fs::write(
            root.join("packages/app/package.json"),
            r#"{"name":"app","main":"dist/index.js","dependencies":{"react":"1"},"devDependencies":{"vitest":"1","@types/node":"1"}}"#,
        )
        .expect("pkg");
        std::fs::write(
            root.join("crates/core/Cargo.toml"),
            "[package]\nname = \"core-lib\"\n[dependencies]\nserde-json = \"1\"\n[dev-dependencies]\ntempfile = \"3\"\n",
        )
        .expect("cargo");
        let files = [
            "packages/app/src/index.ts",
            "packages/app/src/pages/home.tsx",
            "packages/app/src/util.ts",
            "crates/core/src/lib.rs",
            "crates/core/src/bin/tool.rs",
            "crates/core/src/x.rs",
        ];
        let ws = Workspace::discover(root, &files, &BTreeSet::new());
        assert_eq!(
            ws.entries.get("packages/app/src/index.ts"),
            Some(&"package.json")
        );
        assert!(ws.entries.contains_key("packages/app/src/pages/home.tsx"));
        assert!(ws.entries.contains_key("crates/core/src/lib.rs"));
        assert!(ws.entries.contains_key("crates/core/src/bin/tool.rs"));
        assert!(!ws.entries.contains_key("packages/app/src/util.ts"));

        let file = "packages/app/src/util.ts";
        assert_eq!(ws.declared(file, "react", "npm"), Declared::Runtime);
        assert_eq!(ws.declared(file, "vitest", "npm"), Declared::Dev);
        assert_eq!(ws.declared(file, "node:fs", "npm"), Declared::Builtin);
        assert_eq!(ws.declared(file, "lodash", "npm"), Declared::Undeclared);
        assert_eq!(
            ws.declared("crates/core/src/x.rs", "serde_json", "cargo"),
            Declared::Runtime
        );
        assert_eq!(
            ws.declared("crates/core/src/x.rs", "tempfile", "cargo"),
            Declared::Dev
        );
        assert_eq!(
            ws.declared("crates/core/src/x.rs", "std", "cargo"),
            Declared::Builtin
        );
        assert_eq!(
            ws.component_of(file, "npm").map(|c| c.name.as_str()),
            Some("app")
        );
    }
}
