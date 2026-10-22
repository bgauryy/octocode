use super::catalog::{default_server_for_file, typescript_server_fallback};
use super::types::LspDiscoveryOptions;
use crate::lsp::types::LspLanguageServerConfig;
use crate::text::file_extension::{JS_TS_EXTENSIONS, extension_of};
use serde_json::Value;
use std::path::Path;

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
    ("jsconfig.json", ".js"),
    ("package.json", ".ts"),
];

/// The project markers that select a language server for a workspace root,
/// in priority order (each once).
pub fn workspace_root_marker_names() -> impl Iterator<Item = &'static str> {
    WORKSPACE_ROOT_MARKERS.iter().map(|(marker, _)| *marker)
}

/// Resolve the language server for a workspace root directory (the
/// `workspaceRoot`-only query shape), inferring the language from project
/// markers since a directory has no file extension. Returns `None` when no
/// marker is present.
pub fn default_server_for_workspace_root(
    workspace_root: &str,
    options: &LspDiscoveryOptions,
) -> Option<LspLanguageServerConfig> {
    let representative = workspace_root_probe_path(workspace_root)?;
    default_server_for_file(&representative, workspace_root, options)
}

/// [`typescript_server_fallback`] for a `workspaceRoot`-only query.
pub fn typescript_server_fallback_for_workspace_root(
    workspace_root: &str,
    options: &LspDiscoveryOptions,
) -> Option<LspLanguageServerConfig> {
    let representative = workspace_root_probe_path(workspace_root)?;
    typescript_server_fallback(&representative, workspace_root, options)
}

/// A synthetic file path in the language the root's project markers imply:
/// discovery keys only on its extension, and the file is never opened or
/// synced.
fn workspace_root_probe_path(workspace_root: &str) -> Option<String> {
    let root = Path::new(workspace_root);
    let extension = root_markers(root).into_iter().next()?;
    Some(
        root.join(format!("workspace{extension}"))
            .to_string_lossy()
            .into_owned(),
    )
}

/// A source file inside `workspace_root` (in the language its project markers
/// imply) to open so servers that need an open document before answering
/// workspace-wide queries (tsserver: "No Project") have a project loaded.
/// Prefers where the project's sources live — tsconfig/jsconfig `include`
/// roots, then `src/` — so a root-level script outside the compiled project
/// (`build.mjs`, `*.config.js`) does not become the representative and load
/// an inferred project that knows none of the workspace symbols. Each search
/// is a bounded breadth-first walk that skips hidden, vendored, and build dirs.
pub fn workspace_root_representative_source(workspace_root: &str) -> Option<String> {
    let extension = workspace_root_languages(workspace_root)
        .into_iter()
        .next()?;
    representative_source_for(workspace_root, extension)
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
        .filter(|(marker, _)| root.join(marker).is_file())
        .map(|(_, extension)| *extension)
        .collect::<Vec<_>>();
    found.sort_by_key(|extension| !declares_language_workspace(root, js_family(extension)));
    found
}

fn js_family(extension: &'static str) -> &'static str {
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

/// [`workspace_root_representative_source`] for one language, as returned
/// by [`workspace_root_languages`].
#[must_use]
pub fn representative_source_for(workspace_root: &str, extension: &str) -> Option<String> {
    let root = Path::new(workspace_root);
    let family: &[&str] = match extension {
        ".ts" | ".js" => JS_TS_EXTENSIONS,
        ".rs" => &["rs"],
        ".go" => &["go"],
        ".py" => &["py"],
        _ => return None,
    };
    let mut preferred = if matches!(extension, ".ts" | ".js") {
        project_include_roots(root)
    } else {
        Vec::new()
    };
    preferred.push(root.join("src"));
    preferred
        .iter()
        .filter(|candidate| candidate.starts_with(root))
        .find_map(|candidate| first_source_under(candidate, family))
        .or_else(|| first_source_under(root, family))
}

/// Literal leading directories (or files) of a tsconfig/jsconfig `include`
/// and `files` list: `"src/**/*.ts"` -> `src`. A config that is not plain JSON
/// (comments, trailing commas) yields nothing and the caller falls back.
fn project_include_roots(root: &Path) -> Vec<std::path::PathBuf> {
    let Some(config) = ["tsconfig.json", "jsconfig.json"]
        .iter()
        .find_map(|name| std::fs::read_to_string(root.join(name)).ok())
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
    else {
        return Vec::new();
    };
    ["files", "include"]
        .iter()
        .filter_map(|key| config.get(*key).and_then(Value::as_array))
        .flatten()
        .filter_map(Value::as_str)
        .filter_map(|pattern| {
            let literal = Path::new(pattern)
                .components()
                .take_while(|component| {
                    let part = component.as_os_str().to_string_lossy();
                    !part.contains(['*', '?', '{', '['])
                })
                .collect::<std::path::PathBuf>();
            (!literal.as_os_str().is_empty() && literal != Path::new("."))
                .then(|| root.join(literal))
        })
        .collect()
}

/// First source file of `family` at or under `start` (a file or directory).
fn first_source_under(start: &Path, family: &[&str]) -> Option<String> {
    const MAX_ENTRIES: usize = 2_000;
    const MAX_DEPTH: usize = 6;
    // Deliberately not the runtime prune policy (`policy::prune`). This walk
    // only picks one representative first-party file to choose and root a
    // language server; it hides nothing from results. `vendor/` is skipped
    // here because a vendored dependency tree is the wrong file to root a
    // server on, while search and syntax walks keep `vendor/` because it is
    // routinely real, reviewable source.
    const SKIPPED_DIRS: &[&str] = &["node_modules", "target", "dist", "build", "out", "vendor"];
    let is_source = |name: &str| {
        !name.ends_with(".d.ts") && family.contains(&extension_of(name, true, "").as_str())
    };
    if start.is_file() {
        let name = start.file_name()?.to_string_lossy();
        return is_source(&name).then(|| start.to_string_lossy().into_owned());
    }
    let mut queue = std::collections::VecDeque::from([(start.to_path_buf(), 0usize)]);
    let mut seen = 0usize;
    while let Some((dir, depth)) = queue.pop_front() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut entries = entries.filter_map(Result::ok).collect::<Vec<_>>();
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            seen += 1;
            if seen > MAX_ENTRIES {
                return None;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                if depth < MAX_DEPTH
                    && !name.starts_with('.')
                    && !SKIPPED_DIRS.contains(&name.as_str())
                {
                    queue.push_back((entry.path(), depth + 1));
                }
            } else if file_type.is_file() && is_source(&name) {
                return Some(entry.path().to_string_lossy().into_owned());
            }
        }
    }
    None
}
