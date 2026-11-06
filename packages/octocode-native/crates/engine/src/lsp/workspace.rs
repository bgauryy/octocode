use crate::error::Result;
use crate::text::lexical;
use std::path::{Path, PathBuf};

mod cargo;
mod includes;
mod modules;
pub use cargo::{cargo_manifest, cargo_workspace_root, declares_cargo_workspace};
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
/// otherwise its language's project ([`language_project_root`]), else the
/// nearest ancestor holding a project marker, widened for a
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
        .or_else(|| language_project_root(&file_path, &directory))
        .or_else(|| project_root_of_dir(&directory))
        .unwrap_or_else(|| directory.clone());
    // Widening stays in the repository: a config above it belongs to something else.
    let in_repository = |above: &PathBuf| {
        repository_root(&directory).is_none_or(|repository| above.starts_with(repository))
    };
    let root = typescript_config_above(&file_path, &directory, &root)
        .filter(in_repository)
        .unwrap_or(root);
    let root = multi_module_root_above(&file_path, &root)
        .filter(in_repository)
        .unwrap_or(root);
    Ok(root.to_string_lossy().into_owned())
}

/// The files naming a C-family project's build, nearest being a module of it.
const C_BUILD_FILES: [&str; 4] = ["CMakeLists.txt", "meson.build", "Makefile", "configure.ac"];

/// The files that name a language's project, in tiers: the nearest ancestor
/// holding a file of the first tier that has one is the project. A later tier
/// is weaker evidence (a `requirements.txt` under a `pyproject.toml` lists a
/// subdirectory's dependencies). `*.ext` matches any file with that suffix.
/// Rust is absent: `cargo_package_dir` and the Cargo workspace root it.
fn project_file_tiers(language_id: &str) -> &'static [&'static [&'static str]] {
    match language_id {
        "typescript" | "typescriptreact" | "javascript" | "javascriptreact" => &[&[
            "tsconfig.json",
            "jsconfig.json",
            "package.json",
            "pnpm-workspace.yaml",
            "yarn.lock",
        ]],
        "python" => &[
            &[
                "pyrightconfig.json",
                "pyproject.toml",
                "setup.py",
                "setup.cfg",
                "Pipfile",
            ],
            &["requirements.txt"],
        ],
        "go" => &[&["go.mod"]],
        "java" => &[&[
            "pom.xml",
            "build.gradle",
            "build.gradle.kts",
            "settings.gradle",
            "settings.gradle.kts",
        ]],
        "scala" => &[&["build.sbt", "build.sc", "build.mill"]],
        "csharp" => &[&["*.sln", "*.slnx"], &["*.csproj"]],
        _ => &[],
    }
}

/// Whether `dir` holds `marker`: a file of that name, or for `*.ext` a file
/// with that suffix.
pub(crate) fn dir_holds(dir: &Path, marker: &str) -> bool {
    let Some(suffix) = marker.strip_prefix('*') else {
        return dir.join(marker).is_file();
    };
    std::fs::read_dir(dir).is_ok_and(|entries| {
        entries.flatten().any(|entry| {
            entry.file_name().to_string_lossy().ends_with(suffix)
                && entry.file_type().is_ok_and(|kind| kind.is_file())
        })
    })
}

/// The project root of `file_path` by its own language's project files, else
/// its repository. A marker of another language or a generic build file in a
/// subdirectory (a `Makefile` beside the sources) names no project of this
/// language, and rooting there hides every other file from the server.
/// `None` with neither: the nearest marker of any kind stands.
fn language_project_root(file_path: &str, directory: &Path) -> Option<PathBuf> {
    let language = crate::lsp::config::detect_language_id(file_path)?;
    if crate::lsp::family::is_c_family(Some(&language)) {
        return c_project_root(directory);
    }
    let within = within_repository(directory);
    project_file_tiers(&language)
        .iter()
        .find_map(|tier| {
            within
                .iter()
                .copied()
                .find(|dir| tier.iter().any(|marker| dir_holds(dir, marker)))
        })
        .or_else(|| {
            (language != "rust")
                .then(|| repository_root(directory))
                .flatten()
        })
        .map(Path::to_path_buf)
}

/// `directory` and its ancestors, up to the repository holding it: a project
/// file above the repository belongs to something else.
fn within_repository(directory: &Path) -> Vec<&Path> {
    let mut within = Vec::new();
    for dir in directory.ancestors() {
        within.push(dir);
        if dir.join(".git").exists() {
            break;
        }
    }
    within
}

/// The nearest ancestor of `directory` that is a repository's root.
fn repository_root(directory: &Path) -> Option<&Path> {
    directory.ancestors().find(|dir| dir.join(".git").exists())
}

/// The root of the project a C-family file in `directory` compiles in, found
/// the way clangd finds its command: the nearest ancestor holding a compilation
/// database or `.clangd`; else the repository, where an out-of-tree build
/// directory (`build/debug`) lives and a relative `--compile-commands-dir`
/// resolves; else the top of the unbroken chain of build files above the file
/// (a subdirectory's `Makefile` is one module of it).
fn c_project_root(directory: &Path) -> Option<PathBuf> {
    use crate::lsp::clangd_database::is_project_config_dir;
    let has_build_file = |dir: &Path| C_BUILD_FILES.iter().any(|name| dir.join(name).is_file());
    within_repository(directory)
        .into_iter()
        .find(|dir| is_project_config_dir(dir))
        .or_else(|| repository_root(directory))
        .or_else(|| {
            directory
                .ancestors()
                .skip_while(|dir| !has_build_file(dir))
                .take_while(|dir| has_build_file(dir))
                .last()
        })
        .map(Path::to_path_buf)
}

/// The top of a multi-module project above `root`, for a server that
/// answers cross-module questions only from there: the `go.work` directory
/// above a Go module; the `pyrightconfig.json`, or `pyproject.toml` with a
/// `[tool.pyright]` or `[tool.uv.workspace]` table, above a Python package;
/// the Maven `pom.xml` whose `<modules>` list the module, or the Gradle
/// `settings.gradle(.kts)` that includes it, above a Java module (walked
/// up while each parent lists the one below); a Gradle module with no
/// settings file of its own then roots where Gradle finds its build: the
/// nearest settings file above it, however that file names its projects (a
/// list, a directory scan). `None` keeps the nearest root.
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
            if is_gradle_module(&module)
                && let Some(build) = module
                    .ancestors()
                    .skip(1)
                    .find(|dir| has_gradle_settings(dir))
            {
                module = build.to_path_buf();
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

const GRADLE_SETTINGS: [&str; 2] = ["settings.gradle", "settings.gradle.kts"];

/// Whether `dir` holds a Gradle settings file: the root of a Gradle build.
fn has_gradle_settings(dir: &Path) -> bool {
    GRADLE_SETTINGS.iter().any(|name| dir.join(name).is_file())
}

/// Whether `dir` is a Gradle project that is not a build's root: a build
/// script and no settings file.
fn is_gradle_module(dir: &Path) -> bool {
    ["build.gradle", "build.gradle.kts"]
        .iter()
        .any(|name| dir.join(name).is_file())
        && !has_gradle_settings(dir)
}

/// Whether `parent/settings.gradle` or `settings.gradle.kts` includes the
/// project `name` (`include 'name'`, `include(":name")`, `include("a:name")`).
fn gradle_includes_module(parent: &Path, name: &str) -> bool {
    GRADLE_SETTINGS
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
