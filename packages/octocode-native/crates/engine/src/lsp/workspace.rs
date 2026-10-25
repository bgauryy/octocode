use crate::error::Result;
use crate::text::lexical;
use std::path::{Path, PathBuf};

mod cargo;
mod modules;
pub use cargo::{cargo_workspace_root, declares_cargo_workspace};

const MARKERS: [&str; 12] = [
    "package.json",
    "pnpm-workspace.yaml",
    "yarn.lock",
    "Cargo.toml",
    "go.mod",
    "pyproject.toml",
    "requirements.txt",
    "tsconfig.json",
    ".git",
    "pom.xml",
    "build.gradle",
    "Makefile",
];

/// The language server root of `file_path`: for a Rust file, the root of
/// the Cargo workspace its package belongs to (one rust-analyzer analyzes
/// every member crate, so a member crate never roots a second one);
/// otherwise the nearest ancestor holding a project marker, widened for a
/// TypeScript-family file to the directory of its governing
/// `tsconfig.json`/`jsconfig.json` when that sits above the marker. With no
/// marker anywhere, the file's own directory. Never the process cwd: a
/// server rooted there would analyze an unrelated tree.
pub fn resolve_workspace_root_for_file(file_path: String) -> Result<String> {
    let directory = Path::new(&file_path)
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map_or_else(|| PathBuf::from(&file_path), Path::to_path_buf);
    let cargo_root = (Path::new(&file_path).extension() == Some("rs".as_ref()))
        .then(|| cargo_package_dir(&directory))
        .flatten()
        .map(|package| cargo_workspace_root(&package).unwrap_or(package));
    let root = cargo_root
        .or_else(|| project_root_of_dir(&directory))
        .unwrap_or_else(|| directory.clone());
    let root = typescript_config_above(&file_path, &directory, &root).unwrap_or(root);
    Ok(root.to_string_lossy().into_owned())
}

/// Project configs tsserver looks for, nearest first, from a file's
/// directory upward.
const TYPESCRIPT_PROJECT_CONFIGS: [&str; 2] = ["tsconfig.json", "jsconfig.json"];

/// The directory of the `tsconfig.json`/`jsconfig.json` governing a
/// TypeScript-family file when it lies above `root`. tsserver searches for
/// that config only up to the workspace root it is given
/// (`projectRootPath`): a package marker below the config (a workspace
/// package without its own tsconfig) would leave the file in an inferred
/// project that sees only opened files, so incoming queries (references,
/// implementations) would miss every unopened importer.
fn typescript_config_above(file_path: &str, directory: &Path, root: &Path) -> Option<PathBuf> {
    let language_id = crate::lsp::config::detect_language_id(file_path);
    if !crate::lsp::family::is_typescript_family(language_id.as_deref()) {
        return None;
    }
    let config_dir = directory.ancestors().find(|dir| {
        TYPESCRIPT_PROJECT_CONFIGS
            .iter()
            .any(|name| dir.join(name).is_file())
    })?;
    (!config_dir.starts_with(root)).then(|| config_dir.to_path_buf())
}

/// The nearest ancestor of `directory` (itself included) holding a
/// `Cargo.toml`: the Cargo package (or virtual workspace) a Rust file
/// belongs to.
pub(crate) fn cargo_package_dir(directory: &Path) -> Option<PathBuf> {
    directory
        .ancestors()
        .find(|dir| dir.join(CARGO_MANIFEST).is_file())
        .map(Path::to_path_buf)
}

/// Whether `dir` is the root of a multi-package workspace in any ecosystem:
/// `pnpm-workspace.yaml`, `lerna.json`, `go.work`, a `package.json` with
/// `workspaces`, or a `Cargo.toml` declaring a workspace.
pub fn is_workspace_manifest_root(dir: &Path) -> bool {
    dir.join("pnpm-workspace.yaml").is_file()
        || dir.join("lerna.json").is_file()
        || dir.join("go.work").is_file()
        || std::fs::read_to_string(dir.join("package.json"))
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .is_some_and(|manifest| manifest.get("workspaces").is_some())
        || declares_cargo_workspace(dir)
}

/// Source a Cargo package `include!`s that rust-analyzer may not load.
/// rust-analyzer loads a package's own directory (and, once build scripts
/// ran, their `OUT_DIR`); an `include!` it cannot load leaves every item the
/// included file defines unknown, so uses that reach a symbol through those
/// types (a method called through `Deref` to one, a field of one) do not
/// resolve.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PackageIncludes {
    /// Package files that `include!` build-script output (`env!(…)`, such
    /// as `concat!(env!("OUT_DIR"), …)`): loaded only when the package has a
    /// build script and rust-analyzer runs it.
    pub generated: Vec<PathBuf>,
    /// Literal `include!` targets outside the package directory (lexically
    /// normalized): never loaded, whatever the build context.
    pub outside: Vec<PathBuf>,
    /// Package files that declare the scanned file as a module (`mod x;`,
    /// or `#[path = "…"] mod x;`). Two or more compile it into several
    /// crates (a bench and a test sharing a support file); rust-analyzer
    /// analyzes it as part of one, so uses in the others' copies are not
    /// references of the symbol it answers for.
    pub declared_by: Vec<PathBuf>,
    /// The scan stopped at its file bound: more includes may exist.
    pub has_more: bool,
}

/// [`PackageIncludes`] of the Cargo package `file` belongs to, scanning at
/// most `limit` of its `.rs` files (nested packages and ignored files are
/// not its sources). `generated` is empty when the package has no build
/// script: without one no build-script output exists to load.
pub fn cargo_package_includes(file: &Path, limit: usize) -> Option<PackageIncludes> {
    let package = cargo_package_dir(file.parent()?)?;
    let build_script = cargo_package_build_script(&package);
    let has_build_script = build_script.is_some();
    let targets = cargo::TargetRoots::new(&package, build_script);
    let root = package.clone();
    let walker = ignore::WalkBuilder::new(&package)
        .filter_entry(move |entry| {
            entry.path() == root
                || !entry.file_type().is_some_and(|kind| kind.is_dir())
                || !entry.path().join(CARGO_MANIFEST).is_file()
        })
        .build();
    let anchor = lexical(file);
    let mut includes = PackageIncludes::default();
    let mut modules = modules::ModuleSources::default();
    let mut visited = 0;
    for entry in walker.flatten() {
        let path = entry.path();
        if !entry.file_type().is_some_and(|kind| kind.is_file())
            || path.extension().is_none_or(|ext| ext != "rs")
        {
            continue;
        }
        if visited >= limit {
            includes.has_more = true;
            break;
        }
        visited += 1;
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        for argument in include_arguments(&text) {
            match argument {
                IncludeArgument::Generated => {
                    if has_build_script && !includes.generated.iter().any(|seen| seen == path) {
                        includes.generated.push(path.to_path_buf());
                    }
                }
                IncludeArgument::Literal(target) => {
                    let target = lexical(&path.parent().unwrap_or(path).join(target));
                    if !target.starts_with(&package) && !includes.outside.contains(&target) {
                        includes.outside.push(target);
                    }
                }
            }
        }
        modules.push(path, &text);
    }
    includes.declared_by = modules.declared_by(&anchor, &targets);
    Some(includes)
}

/// What an `include!` reads: a build-script value (`env!`, possibly in a
/// `concat!`) or a literal path relative to the including file.
#[derive(Debug, PartialEq, Eq)]
enum IncludeArgument {
    Generated,
    Literal(String),
}

/// The arguments of the `include!` invocations in `text`, outside line
/// comments. `include_str!`/`include_bytes!` read data, not source.
fn include_arguments(text: &str) -> Vec<IncludeArgument> {
    let mut found = Vec::new();
    for line in text.lines() {
        let code = line.split("//").next().unwrap_or(line);
        for (at, _) in code.match_indices("include!") {
            if code[..at]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphanumeric() || c == '_')
            {
                continue;
            }
            let rest = code[at + "include!".len()..].trim_start();
            let Some(argument) = rest
                .strip_prefix('(')
                .or_else(|| rest.strip_prefix('{'))
                .or_else(|| rest.strip_prefix('['))
                .map(str::trim_start)
            else {
                continue;
            };
            if let Some(literal) = argument.strip_prefix('"') {
                if let Some(end) = literal.find('"') {
                    found.push(IncludeArgument::Literal(literal[..end].to_owned()));
                }
            } else if argument.starts_with("env!") || argument.starts_with("concat!") {
                found.push(IncludeArgument::Generated);
            }
        }
    }
    found
}

/// The build script of the Cargo package at `package` (its manifest
/// directory), when it has one: `[package] build = "path"`, else `build.rs`
/// beside the manifest (Cargo's default), unless `build = false`.
fn cargo_package_build_script(package: &Path) -> Option<PathBuf> {
    let manifest = cargo::manifest(package)?;
    let settings = manifest.get("package")?;
    if settings.get("build").and_then(toml::Value::as_bool) == Some(false) {
        return None;
    }
    let script = package.join(
        settings
            .get("build")
            .and_then(toml::Value::as_str)
            .unwrap_or("build.rs"),
    );
    script.is_file().then_some(script)
}

const CARGO_MANIFEST: &str = "Cargo.toml";

/// The nearest ancestor of `directory` (itself included) holding a project
/// marker, or `None` when no ancestor has one.
pub fn project_root_of_dir(directory: &Path) -> Option<PathBuf> {
    directory
        .ancestors()
        .find(|dir| MARKERS.iter().any(|marker| dir.join(marker).exists()))
        .map(Path::to_path_buf)
}

#[cfg(test)]
mod tests;
