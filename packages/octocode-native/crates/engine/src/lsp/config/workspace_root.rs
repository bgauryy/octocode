use crate::text::{JS_EXTENSIONS, extension_of};
use serde_json::Value;
use std::path::Path;

/// Each language a directory can route by: its representative-source
/// extension (as [`WORKSPACE_ROOT_MARKERS`] names it) and the file
/// extensions of its sources, in server-selection order.
pub(super) static SOURCE_FAMILIES: std::sync::LazyLock<Vec<(&str, Vec<&str>)>> =
    std::sync::LazyLock::new(|| {
        use super::catalog::{ROUTED_EXTENSIONS, spec_for_extension};
        [".ts", ".rs", ".go", ".py", ".cpp", ".java", ".scala", ".cs"]
            .into_iter()
            .filter_map(|representative| {
                let server = spec_for_extension(representative)?;
                let extensions = ROUTED_EXTENSIONS
                    .iter()
                    .filter(|extension| {
                        spec_for_extension(extension)
                            .is_some_and(|spec| spec.command == server.command)
                    })
                    .filter_map(|extension| extension.strip_prefix('.'))
                    .collect();
                Some((representative, extensions))
            })
            .collect()
    });

/// Directories a representative or language walk does not enter.
/// Deliberately not the runtime prune policy (`policy::prune`): these walks
/// only pick a first-party file to choose and root a language server and
/// hide nothing from results. `vendor/` is skipped because a vendored
/// dependency tree is the wrong file to root a server on, while search and
/// syntax walks keep `vendor/` because it is routinely real, reviewable
/// source.
pub(super) const SKIPPED_DIRS: &[&str] =
    &["node_modules", "target", "dist", "build", "out", "vendor"];

/// Project markers checked (in priority order) to pick a language server for a
/// workspace root that has no file of its own. `tsconfig.json` is the strongest
/// TypeScript signal; a bare `package.json` ranks last because npm wrappers
/// commonly sit beside Rust/Go/Python projects.
const WORKSPACE_ROOT_MARKERS: &[(&str, &str)] = &[
    ("tsconfig.json", ".ts"),
    ("Cargo.toml", ".rs"),
    ("go.mod", ".go"),
    ("pyproject.toml", ".py"),
    ("setup.py", ".py"),
    ("setup.cfg", ".py"),
    ("Pipfile", ".py"),
    ("pyrightconfig.json", ".py"),
    ("requirements.txt", ".py"),
    ("go.work", ".go"),
    ("compile_commands.json", ".cpp"),
    ("compile_flags.txt", ".cpp"),
    (".clangd", ".cpp"),
    ("CMakeLists.txt", ".cpp"),
    ("meson.build", ".cpp"),
    ("pom.xml", ".java"),
    ("build.gradle", ".java"),
    ("build.gradle.kts", ".java"),
    ("settings.gradle", ".java"),
    ("settings.gradle.kts", ".java"),
    ("build.sbt", ".scala"),
    ("build.sc", ".scala"),
    ("build.mill", ".scala"),
    ("*.sln", ".cs"),
    ("*.slnx", ".cs"),
    ("*.csproj", ".cs"),
    ("jsconfig.json", ".js"),
    ("package.json", ".ts"),
];

/// The project markers that select a language server for a workspace root,
/// in priority order (each once).
pub fn workspace_root_marker_names() -> impl Iterator<Item = &'static str> {
    WORKSPACE_ROOT_MARKERS.iter().map(|(marker, _)| *marker)
}

/// Representative-source extensions (`.ts`, `.rs`, `.go`, `.py`) of every
/// project marker at `workspace_root`, in server-selection order and
/// deduplicated. The first is the language a root-only query uses; the rest
/// are other projects sharing the root (e.g. a Cargo workspace with a
/// tsconfig), which a root-only query does not search.
#[must_use]
pub fn workspace_root_languages(workspace_root: &str) -> Vec<&'static str> {
    let mut languages: Vec<&'static str> = Vec::new();
    for extension in root_markers(Path::new(workspace_root)) {
        let family = js_family(extension);
        if !languages.contains(&family) {
            languages.push(family);
        }
    }
    languages
}

/// The probe extensions of the project markers at `root`, in
/// server-selection order: a language whose manifest here declares a
/// workspace (a Cargo `[workspace]`, `package.json` `workspaces` or
/// `pnpm-workspace.yaml`, `go.work`) first, because the root is that
/// language's project and the other markers are its tooling (a Cargo
/// workspace with a `tsconfig.json` for its JS bindings); otherwise
/// [`WORKSPACE_ROOT_MARKERS`] order.
fn root_markers(root: &Path) -> Vec<&'static str> {
    let mut found = WORKSPACE_ROOT_MARKERS
        .iter()
        .filter(|(marker, _)| crate::lsp::workspace::dir_holds(root, marker))
        .map(|(_, extension)| *extension)
        .collect::<Vec<_>>();
    found.sort_by_key(|extension| !declares_language_workspace(root, js_family(extension)));
    found
}

fn js_family(extension: &str) -> &str {
    if extension == ".js" { ".ts" } else { extension }
}

/// Whether `root` holds a workspace manifest of the language `family`.
fn declares_language_workspace(root: &Path, family: &str) -> bool {
    match family {
        ".rs" => crate::lsp::workspace::declares_cargo_workspace(root),
        ".ts" => {
            root.join("pnpm-workspace.yaml").is_file()
                || std::fs::read_to_string(root.join("package.json"))
                    .ok()
                    .and_then(|text| serde_json::from_str::<Value>(&text).ok())
                    .is_some_and(|manifest| manifest.get("workspaces").is_some())
        }
        ".go" => root.join("go.work").is_file(),
        _ => false,
    }
}

/// A source file inside `workspace_root`, in one language its project
/// markers imply (`extension`, as [`workspace_root_languages`] returns it),
/// to open so servers that need an open document before answering
/// workspace-wide queries (tsserver: "No Project") have a project loaded.
/// Prefers where the project's sources live — tsconfig/jsconfig `include`
/// roots, then `src/` — so a root-level script outside the compiled project
/// (`build.mjs`, `*.config.js`) does not become the representative and load
/// an inferred project that knows none of the workspace symbols. Each search
/// is a breadth-first walk that skips hidden, vendored, and build dirs.
#[must_use]
pub fn representative_source_for(workspace_root: &str, extension: &str) -> Option<String> {
    let root = Path::new(workspace_root);
    let extension = js_family(extension);
    let (_, family) = SOURCE_FAMILIES
        .iter()
        .find(|(probe, _)| *probe == extension)?;
    let project = if extension == ".ts" {
        ProjectFiles::read(root)
    } else {
        ProjectFiles::default()
    };
    let mut preferred = project.roots.clone();
    preferred.push(root.join("src"));
    let preferred = preferred
        .into_iter()
        .filter(|candidate| candidate.starts_with(root))
        .chain(std::iter::once(root.to_path_buf()))
        .collect::<Vec<_>>();
    // A file the project config compiles first: a literal include root such
    // as `packages` (of `packages/*/src/**/*`) also holds files outside it
    // (`packages/a/vitest.config.ts`). Any source only when none matches.
    // A translation unit before a header: a header opened alone is no
    // compiled file (clangd parses it without its includer's context).
    let is_unit = |path: &Path| {
        !HEADER_EXTENSIONS.contains(&extension_of(&path.to_string_lossy(), true, "").as_str())
    };
    let is_implementation = |path: &Path| {
        is_unit(path)
            && !oxc_span::SourceType::from_path(path)
                .is_ok_and(oxc_span::SourceType::is_typescript_definition)
    };
    let any = |_: &Path| true;
    let categories: [&dyn Fn(&Path) -> bool; 3] = [&is_implementation, &is_unit, &any];
    categories.into_iter().find_map(|category| {
        let compiled = |path: &Path| category(path) && project.compiles(root, path);
        preferred
            .iter()
            .find_map(|candidate| first_source_under(candidate, family, &compiled))
            .or_else(|| {
                preferred
                    .iter()
                    .find_map(|candidate| first_source_under(candidate, family, category))
            })
    })
}

/// C-family header extensions: included by a translation unit, not compiled
/// on their own.
const HEADER_EXTENSIONS: &[&str] = &["h", "hpp", "hh", "hxx", "cuh"];

/// The file patterns of a root tsconfig/jsconfig: the literal leading
/// directories of its `files`/`include` entries (`"src/**/*.ts"` -> `src`),
/// where a representative is searched first, and the full `include`/`exclude`
/// globs a representative must satisfy. A config that is not plain JSON
/// (comments, trailing commas) yields nothing and the caller falls back.
#[derive(Default)]
struct ProjectFiles {
    roots: Vec<std::path::PathBuf>,
    include: Option<globset::GlobSet>,
    exclude: Option<globset::GlobSet>,
    /// The config is a tsconfig that does not compile JavaScript
    /// (`allowJs`/`checkJs` unset): a `.js`/`.cjs` its `include` matches is
    /// no file of the project, and a server opening it loads an inferred
    /// project instead.
    skips_js: bool,
}

impl ProjectFiles {
    fn read(root: &Path) -> Self {
        let Some((name, text)) = ["tsconfig.json", "jsconfig.json"]
            .iter()
            .find_map(|name| Some((*name, read_config(&root.join(name))?)))
        else {
            return Self::default();
        };
        let Some(config) = serde_json::from_str::<Value>(&text).ok() else {
            return Self::default();
        };
        let patterns = |keys: &[&str]| {
            keys.iter()
                .filter_map(|key| config.get(*key).and_then(Value::as_array))
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect::<Vec<_>>()
        };
        let included = patterns(&["files", "include"]);
        Self {
            roots: include_roots(root, &included),
            include: glob_set(&included),
            exclude: glob_set(&patterns(&["exclude"])),
            skips_js: name == "tsconfig.json" && !compiles_js(root, &text, &config),
        }
    }

    /// Whether the config compiles `path`: it matches an include (every file
    /// when the config lists none) and no exclude.
    fn compiles(&self, root: &Path, path: &Path) -> bool {
        let Ok(relative) = path.strip_prefix(root) else {
            return false;
        };
        if self.skips_js
            && JS_EXTENSIONS.contains(&extension_of(&path.to_string_lossy(), true, "").as_str())
        {
            return false;
        }
        self.include
            .as_ref()
            .is_none_or(|set| set.is_match(relative))
            && !self
                .exclude
                .as_ref()
                .is_some_and(|set| set.is_match(relative))
    }
}

/// Whether the tsconfig `text` (read from `root`) compiles JavaScript: it
/// or a config it `extends` (relative paths, read as text: a shared base is
/// often JSONC) sets `allowJs` or `checkJs` to `true`.
fn compiles_js(root: &Path, text: &str, config: &Value) -> bool {
    let extended = match &config["extends"] {
        Value::String(path) => vec![path.as_str()],
        Value::Array(paths) => paths.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    };
    sets_js(text)
        || extended
            .into_iter()
            .filter(|path| path.starts_with('.'))
            .any(|path| {
                let mut base = root.join(path);
                if base.extension().is_none() {
                    base.set_extension("json");
                }
                read_config(&base).is_some_and(|text| sets_js(&text))
            })
}

/// A project config file's text.
fn read_config(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

/// Whether config text sets `allowJs` or `checkJs` to `true`.
fn sets_js(text: &str) -> bool {
    ["\"allowJs\"", "\"checkJs\""].iter().any(|key| {
        text.match_indices(key).any(|(at, _)| {
            text.get(at + key.len()..)
                .map(str::trim_start)
                .and_then(|rest| rest.strip_prefix(':'))
                .is_some_and(|rest| rest.trim_start().starts_with("true"))
        })
    })
}

/// tsconfig globs: `*` stays within one path segment, and a pattern names
/// a file or a directory (whose whole tree it covers). `None` for no
/// pattern, or one globset rejects.
fn glob_set(patterns: &[String]) -> Option<globset::GlobSet> {
    if patterns.is_empty() {
        return None;
    }
    let mut builder = globset::GlobSetBuilder::new();
    for pattern in patterns {
        let pattern = pattern.trim_start_matches("./").trim_end_matches('/');
        for form in [pattern.to_owned(), format!("{pattern}/**")] {
            builder.add(
                globset::GlobBuilder::new(&form)
                    .literal_separator(true)
                    .build()
                    .ok()?,
            );
        }
    }
    builder.build().ok()
}

/// Literal leading directories (or files) of `files`/`include` patterns.
fn include_roots(root: &Path, patterns: &[String]) -> Vec<std::path::PathBuf> {
    patterns
        .iter()
        .filter_map(|pattern| {
            let literal = Path::new(pattern)
                .components()
                .take_while(|component| {
                    let part = component.as_os_str().to_string_lossy();
                    !part.contains(crate::search::glob::GLOB_META)
                })
                .collect::<std::path::PathBuf>();
            (!literal.as_os_str().is_empty() && literal != Path::new("."))
                .then(|| root.join(literal))
        })
        .collect()
}

/// First source file of `family` at or under `start` (a file or directory)
/// that `accept` keeps.
fn first_source_under(
    start: &Path,
    family: &[&str],
    accept: &dyn Fn(&Path) -> bool,
) -> Option<String> {
    let is_source = |name: &str| family.contains(&extension_of(name, true, "").as_str());
    if start.is_file() {
        let name = start.file_name()?.to_string_lossy();
        return (is_source(&name) && accept(start)).then(|| start.to_string_lossy().into_owned());
    }
    let mut queue = std::collections::VecDeque::from([start.to_path_buf()]);
    while let Some(dir) = queue.pop_front() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut entries = entries.filter_map(Result::ok).collect::<Vec<_>>();
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                if !name.starts_with('.') && !SKIPPED_DIRS.contains(&name.as_str()) {
                    queue.push_back(entry.path());
                }
            } else if file_type.is_file() && is_source(&name) && accept(&entry.path()) {
                return Some(entry.path().to_string_lossy().into_owned());
            }
        }
    }
    None
}
