use crate::error::Result;
use crate::text::lexical;
use std::path::{Path, PathBuf};

mod cargo;
mod includes;
mod modules;
pub use cargo::{cargo_workspace_root, declares_cargo_workspace};
#[cfg(test)]
use includes::{PACKAGE_FILES_READ, cargo_package_build_script};
pub use includes::{PackageIncludes, cargo_package_includes};

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
    let root = multi_module_root_above(&file_path, &root).unwrap_or(root);
    Ok(root.to_string_lossy().into_owned())
}

/// The top of a multi-module project above `root`, for a server that
/// answers cross-module questions only from there: the `go.work` directory
/// above a Go module; the `pyrightconfig.json`, or `pyproject.toml` with a
/// `[tool.pyright]` or `[tool.uv.workspace]` table, above a Python package;
/// the Maven `pom.xml` whose `<modules>` list the module, or the Gradle
/// `settings.gradle(.kts)` that includes it, above a Java module (walked
/// up while each parent lists the one below). `None` keeps the nearest
/// root.
fn multi_module_root_above(file_path: &str, root: &Path) -> Option<PathBuf> {
    let extension = Path::new(file_path).extension()?.to_str()?;
    let mut parents = root.parent().into_iter().flat_map(Path::ancestors);
    match extension {
        "go" => parents
            .find(|dir| dir.join("go.work").is_file())
            .map(Path::to_path_buf),
        "py" | "pyi" => parents
            .find(|dir| dir.join("pyrightconfig.json").is_file() || pyproject_declares_root(dir))
            .map(Path::to_path_buf),
        "java" => {
            let mut module = root.to_path_buf();
            while let Some(parent) = module.parent()
                && let Some(name) = module.file_name().and_then(|name| name.to_str())
                && (maven_lists_module(parent, name) || gradle_includes_module(parent, name))
            {
                module = parent.to_path_buf();
            }
            (module != root).then_some(module)
        }
        _ => None,
    }
}

/// Whether `dir/pyproject.toml` configures a whole project for pyright or a
/// uv workspace, which its packages below share.
fn pyproject_declares_root(dir: &Path) -> bool {
    std::fs::read_to_string(dir.join("pyproject.toml")).is_ok_and(|text| {
        text.lines().map(str::trim).any(|line| {
            matches!(
                line,
                "[tool.pyright]" | "[tool.uv.workspace]" | "[tool.basedpyright]"
            )
        })
    })
}

/// Whether `parent/pom.xml` lists `name` in its `<modules>`.
fn maven_lists_module(parent: &Path, name: &str) -> bool {
    std::fs::read_to_string(parent.join("pom.xml")).is_ok_and(|text| {
        text.split("<module>")
            .skip(1)
            .filter_map(|rest| rest.split("</module>").next())
            .any(|listed| {
                let listed = listed.trim().trim_end_matches('/');
                listed == name || listed.rsplit('/').next() == Some(name)
            })
    })
}

/// Whether `parent/settings.gradle` or `settings.gradle.kts` includes the
/// project `name` (`include 'name'`, `include(":name")`, `include("a:name")`).
fn gradle_includes_module(parent: &Path, name: &str) -> bool {
    ["settings.gradle", "settings.gradle.kts"]
        .iter()
        .filter_map(|settings| std::fs::read_to_string(parent.join(settings)).ok())
        .any(|text| {
            text.lines()
                .filter(|line| line.trim_start().starts_with("include"))
                .any(|line| {
                    line.split(['\'', '"'])
                        .skip(1)
                        .step_by(2)
                        .any(|quoted| quoted.rsplit(':').next() == Some(name))
                })
        })
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
    let config_dir = typescript_project_dir(directory)?;
    (!config_dir.starts_with(root)).then(|| config_dir.to_path_buf())
}

/// The nearest ancestor of `directory` (itself included) holding a
/// `tsconfig.json`/`jsconfig.json`: where tsserver finds the project of a
/// file in `directory`.
pub fn typescript_project_dir(directory: &Path) -> Option<&Path> {
    directory.ancestors().find(|dir| {
        TYPESCRIPT_PROJECT_CONFIGS
            .iter()
            .any(|name| dir.join(name).is_file())
    })
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

const CARGO_MANIFEST: &str = "Cargo.toml";

/// Project files rust-analyzer loads a Rust file from: it analyzes no file
/// outside a Cargo package or a `rust-project.json`, whatever other marker
/// (`.git`, `package.json`) sits above it.
const RUST_PROJECT_FILES: [&str; 2] = [CARGO_MANIFEST, "rust-project.json"];

/// Whether `file` lies outside every project its language server needs to
/// analyze it, so the server's empty answer about it is no absence. Only
/// rust-analyzer needs one ([`RUST_PROJECT_FILES`]); the other servers
/// analyze a file without a project file in an inferred or single-file
/// project.
pub fn outside_required_project(file: &Path) -> bool {
    file.extension() == Some("rs".as_ref())
        && file.parent().is_none_or(|directory| {
            !directory.ancestors().any(|dir| {
                RUST_PROJECT_FILES
                    .iter()
                    .any(|name| dir.join(name).is_file())
            })
        })
}

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
