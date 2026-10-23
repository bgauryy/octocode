//! The project model around the parsed files: build manifests (components),
//! their declared dependencies, and inferred entrypoints.
//!
//! Manifests are discovered only on ancestors of scanned files, so excluded
//! trees (`node_modules`, build output) are never walked. Everything here is
//! best effort: an unreadable or malformed manifest contributes nothing and
//! never fails ingest.
use super::classify::SourceMarks;
use octocode_engine::text::JS_TS_EXTENSIONS;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;

mod entries;
mod manifests;
use manifests::*;

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

impl Workspace {
    /// Discovers manifests on the ancestors of `files` (root-relative) and
    /// infers entrypoints.
    /// `mains` are files that declare a `main`/`Main` function or method
    /// and `marks` what each file's scanned source says (both from the
    /// parse, so conventions need no file reads). `read` reads a manifest
    /// through the caller's path and content policy.
    pub(crate) fn discover(
        root: &Path,
        files: &[&str],
        mains: &BTreeSet<&str>,
        marks: &dyn Fn(&str) -> SourceMarks,
        read: &dyn Fn(&Path) -> Option<String>,
    ) -> Self {
        let known = files.iter().copied().collect::<HashSet<_>>();
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
            if let Some(manifest) = read_json(read, &base.join("package.json")) {
                workspace.npm_entries(root, dir, &manifest, &known);
                found.push(npm_component(dir, &manifest));
            }
            if let Some(manifest) = read_toml(read, &base.join("Cargo.toml")) {
                workspace.cargo_entries(dir, &manifest, files, &known);
                found.push(cargo_component(dir, &manifest));
            }
            found.extend(go_component(dir, root, read));
            found.extend(python_component(dir, root, read));
            if !found.is_empty() {
                workspace.components.insert(dir.clone(), found);
            }
        }
        workspace.convention_entries(files, mains, marks);
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
}

#[cfg(test)]
mod tests;
