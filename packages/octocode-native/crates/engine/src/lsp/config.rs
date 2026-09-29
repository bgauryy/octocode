use crate::lsp::commands::{command_resolves_to_executable, is_executable_path, is_rejected_shell};
use crate::lsp::grammar::grammar_for_file;
use crate::lsp::types::JsLanguageServerConfig;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Copy)]
struct ServerSpec {
    language_id: &'static str,
    command: &'static str,
    args: &'static [&'static str],
    env_var: Option<&'static str>,
}

#[derive(Deserialize)]
struct UserConfigFile {
    #[serde(rename = "languageServers")]
    language_servers: HashMap<String, UserServerSpec>,
}

#[derive(Deserialize)]
struct UserServerSpec {
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(rename = "languageId")]
    language_id: String,
    #[serde(rename = "initializationOptions")]
    initialization_options: Option<Value>,
}

pub fn detect_language_id(file_path: String) -> Option<String> {
    // The protocol ID can differ from the parser grammar: JSX uses the
    // JavaScript grammar but must be opened as javascriptreact by the server.
    spec_for_file(&file_path)
        .map(|spec| spec.language_id.to_owned())
        .or_else(|| grammar_for_file(&file_path).map(|spec| spec.language_id.to_owned()))
}

#[derive(Clone, Debug, Default)]
pub struct LspDiscoveryOptions {
    pub config_path: Option<PathBuf>,
    pub trust_project_config: bool,
}

pub fn default_server_for_file(
    file_path: String,
    workspace_root: String,
) -> Option<JsLanguageServerConfig> {
    let options = LspDiscoveryOptions {
        config_path: std::env::var("OCTOCODE_LSP_CONFIG")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .map(PathBuf::from),
        trust_project_config: project_lsp_config_trusted(),
    };
    default_server_for_file_with_options(file_path, workspace_root, &options)
}

pub fn default_server_for_file_with_options(
    file_path: String,
    workspace_root: String,
    options: &LspDiscoveryOptions,
) -> Option<JsLanguageServerConfig> {
    let extension = extension_key(&file_path)?;

    // Assembly has no built-in default server (ARCHITECTURE: it requires trusted
    // custom configuration). An explicit `OCTOCODE_ASM_SERVER_PATH` is that
    // configuration expressed via env and wins over a config file; without it,
    // only a user `lsp-servers.json` entry can launch a server — never a default.
    if matches!(extension.as_str(), ".asm" | ".assembly" | ".s") {
        if let Some(command) = std::env::var("OCTOCODE_ASM_SERVER_PATH")
            .ok()
            .filter(|value| !value.trim().is_empty())
        {
            return Some(JsLanguageServerConfig {
                command,
                args: Some(Vec::new()),
                workspace_root,
                language_id: Some("asm".to_owned()),
                initialization_options: None,
                env: None,
                max_memory_mb: None,
            });
        }
        return user_server_for_extension(&extension, &workspace_root, options);
    }

    let spec = spec_for_extension(&extension);

    // Explicit env overrides are the top of the resolution ladder for known
    // languages. They must win even when .octocode/lsp-servers.json exists.
    let trust_workspace = options.trust_project_config || project_lsp_config_trusted();
    if let Some(spec) = spec.filter(spec_has_env_override) {
        return Some(config_from_spec(spec, workspace_root, trust_workspace));
    }

    if let Some(mut config) = user_server_for_extension(&extension, &workspace_root, options) {
        apply_server_default_options(&mut config);
        return Some(config);
    }

    spec.map(|spec| config_from_spec(spec, workspace_root, trust_workspace))
}

/// rust-analyzer settings that keep it from running repository code: no
/// build scripts (`build.rs`), no proc-macro expansion, no `cargo check` on
/// save, and no cache priming. `cargo.targetDir: true` keeps any cargo work
/// rust-analyzer still does out of the repository's own `target/`. Keys use
/// the same shapes as the runtime's `rustContext` overlay, which sets
/// `cargo.buildScripts`, `procMacro`, `checkOnSave`, and `cargo.targetDir` on
/// top of these when a caller opts in.
pub fn rust_analyzer_headless_options() -> Value {
    json!({
        "cargo": {
            "buildScripts": { "enable": false },
            "targetDir": true
        },
        "procMacro": { "enable": false },
        "checkOnSave": false,
        "cachePriming": { "enable": false }
    })
}

/// clangd flags that keep it read-only and quiet: no background index (it
/// writes `.cache/clangd/` into the repository), no clang-tidy, errors-only
/// logging, and precompiled preambles in memory rather than temp files.
pub const CLANGD_HEADLESS_ARGS: &[&str] = &[
    "--background-index=false",
    "--clang-tidy=false",
    "--log=error",
    "--pch-storage=memory",
];

/// jdtls settings (sent as `initializationOptions.settings`): no automatic
/// workspace build, and Eclipse metadata (`.project`, `.classpath`,
/// `.settings/`) kept in the `-data` directory, not the project root.
pub fn jdtls_headless_options() -> Value {
    json!({
        "settings": {
            "java": {
                "autobuild": { "enabled": false },
                "import": { "generatesMetadataFilesAtProjectRoot": false }
            }
        }
    })
}

/// Metals `initializationOptions`: no HTTP server (no listening port) and no
/// status-bar traffic.
pub fn metals_headless_options() -> Value {
    json!({
        "isHttpEnabled": false,
        "statusBarProvider": "off"
    })
}

/// The per-workspace jdtls data directory, outside the repository:
/// `<octocode home>/lsp-workspaces/jdtls/<sha256(workspace root)[..16]>`,
/// where the octocode home is `OCTOCODE_HOME` (when absolute) or
/// `~/.octocode`. `None` when no home directory is known.
pub fn jdtls_data_dir(workspace_root: &str) -> Option<PathBuf> {
    use sha2::{Digest, Sha256};
    let home = std::env::var_os("OCTOCODE_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME")
                .or_else(|| std::env::var_os("USERPROFILE"))
                .map(|home| PathBuf::from(home).join(".octocode"))
        })?;
    let digest = hex::encode(Sha256::digest(workspace_root.as_bytes()));
    Some(
        home.join("lsp-workspaces")
            .join("jdtls")
            .join(&digest[..16]),
    )
}

/// Apply per-server safe defaults to a resolved launch configuration, on
/// every launch path (discovery, runtime, napi). rust-analyzer runs no
/// repository code (build scripts, proc-macros, `cargo check`); clangd writes
/// no index into the repository; jdtls keeps its workspace data outside it
/// and does not auto-build; Metals opens no HTTP port. Defaults sit
/// *underneath* the configuration: an `initializationOptions` key the user
/// (or `rustContext`) set wins, and a flag the user already passed (by name,
/// before any `=`) is not added again. A non-object user value is left
/// untouched. Idempotent.
pub fn apply_server_default_options(config: &mut JsLanguageServerConfig) {
    match server_stem(&config.command).as_deref() {
        Some("rust-analyzer") => apply_rust_analyzer_defaults(config),
        Some("clangd") => add_default_args(config, CLANGD_HEADLESS_ARGS),
        Some("jdtls") => {
            let has_data = config
                .args
                .as_deref()
                .is_some_and(|args| args.iter().any(|arg| arg == "-data"));
            if !has_data && let Some(dir) = jdtls_data_dir(&config.workspace_root) {
                let args = config.args.get_or_insert_with(Vec::new);
                args.push("-data".to_owned());
                args.push(dir.to_string_lossy().into_owned());
            }
            merge_default_options(config, jdtls_headless_options());
        }
        Some("metals") => merge_default_options(config, metals_headless_options()),
        _ => {}
    }
}

/// Lower-cased file stem of a server command (`/usr/bin/clangd-18` →
/// `clangd`: a trailing `-<version>` is dropped), with `.exe`/`.cmd`/`.bat`
/// removed.
fn server_stem(command: &str) -> Option<String> {
    let name = Path::new(command)
        .file_name()?
        .to_str()?
        .to_ascii_lowercase();
    let stem = [".exe", ".cmd", ".bat"]
        .iter()
        .find_map(|suffix| name.strip_suffix(suffix))
        .unwrap_or(&name);
    let base = match stem.rsplit_once('-') {
        Some((base, version))
            if !version.is_empty() && version.chars().all(|c| c.is_ascii_digit() || c == '.') =>
        {
            base
        }
        _ => stem,
    };
    Some(base.to_owned())
}

/// Append each default flag whose name (the part before `=`) the args do
/// not already set.
fn add_default_args(config: &mut JsLanguageServerConfig, defaults: &[&str]) {
    let args = config.args.get_or_insert_with(Vec::new);
    let flag_name = |arg: &str| arg.split_once('=').map_or(arg, |(name, _)| name).to_owned();
    for default in defaults {
        let name = flag_name(default);
        if !args.iter().any(|arg| flag_name(arg) == name) {
            args.push((*default).to_owned());
        }
    }
}

fn merge_default_options(config: &mut JsLanguageServerConfig, defaults: Value) {
    config.initialization_options = Some(match config.initialization_options.take() {
        None | Some(Value::Null) => defaults,
        Some(user @ Value::Object(_)) => merge_under(defaults, user),
        Some(other) => other,
    });
}

fn apply_rust_analyzer_defaults(config: &mut JsLanguageServerConfig) {
    merge_default_options(config, rust_analyzer_headless_options());
}

/// Deep-merge `overlay` onto `base`: objects merge key by key, and any other
/// overlay value replaces the base value.
fn merge_under(base: Value, overlay: Value) -> Value {
    match (base, overlay) {
        (Value::Object(mut base), Value::Object(overlay)) => {
            for (key, value) in overlay {
                let merged = match base.remove(&key) {
                    Some(existing) => merge_under(existing, value),
                    None => value,
                };
                base.insert(key, merged);
            }
            Value::Object(base)
        }
        (_, overlay) => overlay,
    }
}

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

/// Resolve the language server for a workspace root directory (the
/// `workspaceRoot`-only query shape), inferring the language from project
/// markers since a directory has no file extension. Returns `None` when no
/// marker is present.
pub fn default_server_for_workspace_root_with_options(
    workspace_root: String,
    options: &LspDiscoveryOptions,
) -> Option<JsLanguageServerConfig> {
    let root = Path::new(&workspace_root);
    let extension = WORKSPACE_ROOT_MARKERS
        .iter()
        .find(|(marker, _)| root.join(marker).is_file())
        .map(|(_, extension)| *extension)?;
    // A synthetic representative path: discovery keys only on its extension,
    // and the file is never opened or synced.
    let representative = root
        .join(format!("workspace{extension}"))
        .to_string_lossy()
        .into_owned();
    default_server_for_file_with_options(representative, workspace_root, options)
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
    let root = Path::new(workspace_root);
    let mut languages: Vec<&'static str> = Vec::new();
    for (marker, extension) in WORKSPACE_ROOT_MARKERS {
        let family = if *extension == ".js" {
            ".ts"
        } else {
            *extension
        };
        if root.join(marker).is_file() && !languages.contains(&family) {
            languages.push(family);
        }
    }
    languages
}

/// [`workspace_root_representative_source`] for one language, as returned
/// by [`workspace_root_languages`].
#[must_use]
pub fn representative_source_for(workspace_root: &str, extension: &str) -> Option<String> {
    let root = Path::new(workspace_root);
    let family: &[&str] = match extension {
        ".ts" | ".js" => &["ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs"],
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
        !name.ends_with(".d.ts")
            && Path::new(name)
                .extension()
                .is_some_and(|ext| family.contains(&ext.to_string_lossy().as_ref()))
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

fn spec_has_env_override(spec: &ServerSpec) -> bool {
    spec.env_var
        .and_then(|key| std::env::var(key).ok())
        .is_some_and(|value| !value.trim().is_empty())
}

fn config_from_spec(
    spec: ServerSpec,
    workspace_root: String,
    trust_workspace: bool,
) -> JsLanguageServerConfig {
    let (command, args) = resolve_spec_invocation(&spec, &workspace_root, trust_workspace);
    let mut config = JsLanguageServerConfig {
        command,
        args: Some(args),
        workspace_root,
        language_id: Some(spec.language_id.to_owned()),
        initialization_options: None,
        env: None,
        max_memory_mb: None,
    };
    // The built-in Rust route is rust-analyzer even when an override points
    // at a differently named binary (e.g. `rust-analyzer-nightly`).
    if spec.env_var == Some("OCTOCODE_RUST_SERVER_PATH") {
        apply_rust_analyzer_defaults(&mut config);
    } else {
        apply_server_default_options(&mut config);
    }
    config
}

/// True for the JS/TS server spec (the one fronting the TS backend selection).
fn is_typescript_spec(spec: &ServerSpec) -> bool {
    spec.env_var == Some("OCTOCODE_TS_SERVER_PATH")
}

/// `tsgo` (Microsoft's Go-native TypeScript server) speaks LSP over stdio with
/// `--lsp -stdio`, unlike `typescript-language-server`'s `--stdio`.
fn command_is_tsgo(command: &str) -> bool {
    Path::new(command)
        .file_stem()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("tsgo"))
}

fn tsgo_args() -> Vec<String> {
    vec!["--lsp".to_owned(), "-stdio".to_owned()]
}

/// Resolve a spec's command + args. JS/TS keeps
/// `typescript-language-server` as the stable default. An explicit
/// `OCTOCODE_TS_SERVER_PATH` may select `tsgo`, whose invocation differs.
fn resolve_spec_invocation(
    spec: &ServerSpec,
    workspace_root: &str,
    trust_workspace: bool,
) -> (String, Vec<String>) {
    let env_override = spec
        .env_var
        .and_then(|key| std::env::var(key).ok())
        .filter(|value| !value.trim().is_empty());

    if is_typescript_spec(spec) {
        // 1) Explicit override — pick args by whether it points at tsgo.
        if let Some(command) = env_override {
            let args = if command_is_tsgo(&command) {
                tsgo_args()
            } else {
                spec.args.iter().map(|arg| (*arg).to_owned()).collect()
            };
            return resolve_server_invocation(&command, args, workspace_root, trust_workspace);
        }
        // Automatic tsgo preference is intentionally disabled until the
        // held-out parity matrix covers all public LSP operations.
    }

    if is_python_spec(spec) && env_override.is_none() {
        let path_var = std::env::var_os("PATH");
        if let Some(invocation) = resolve_pyright_family(workspace_root, path_var.as_deref()) {
            return invocation;
        }
    }

    let command = env_override.unwrap_or_else(|| spec.command.to_owned());
    resolve_server_invocation(
        &command,
        spec.args.iter().map(|arg| (*arg).to_owned()).collect(),
        workspace_root,
        trust_workspace,
    )
}

fn is_python_spec(spec: &ServerSpec) -> bool {
    spec.env_var == Some("OCTOCODE_PYTHON_SERVER_PATH")
}

/// Python servers in preference order. basedpyright and pyright implement
/// callHierarchy, workspace/symbol, and implementation; pylsp (the fallback)
/// implements none of them.
const PYRIGHT_FAMILY: &[&str] = &["basedpyright-langserver", "pyright-langserver"];

/// The first pyright-family server installed on `path_var`, as
/// `(command, ["--stdio"])`. Only `PATH` is searched: this preference is
/// automatic, so a checkout's own `node_modules/.bin` must not be able to
/// swap in an executable it ships. Users who want a workspace-local server set
/// `OCTOCODE_PYTHON_SERVER_PATH` or an LSP config entry. `None` means neither
/// is installed and the caller falls back to `pylsp`.
fn resolve_pyright_family(
    workspace_root: &str,
    path_var: Option<&OsStr>,
) -> Option<(String, Vec<String>)> {
    let cwd = Path::new(workspace_root);
    PYRIGHT_FAMILY.iter().find_map(|name| {
        let found = path_var
            .and_then(|paths| which::which_in(name, Some(paths), cwd).ok())
            .filter(|path| is_executable_path(path))?;
        Some((
            found.to_string_lossy().into_owned(),
            vec!["--stdio".to_owned()],
        ))
    })
}

/// Upper bound for an availability probe that must execute the command (a
/// rustup proxy can otherwise block for minutes installing a toolchain).
pub const COMMAND_PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

/// Run `command args..` with null stdio and report whether it exited
/// successfully within `timeout`. On timeout the child is killed and reaped and
/// the probe reports `false`. Blocking: call from a blocking thread, never from
/// an async executor or the Node main thread when avoidable.
pub(crate) fn probe_command_succeeds(
    command: &str,
    args: &[&str],
    timeout: std::time::Duration,
) -> bool {
    use std::process::Stdio;
    let mut probe = Command::new(command);
    probe
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // Own process group, so a timeout kills whatever the command spawned
    // too (a rustup proxy execs or forks the real toolchain binary).
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        probe.process_group(0);
    }
    let Ok(mut child) = probe.spawn() else {
        return false;
    };
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            _ => {
                kill_probe_group(&child);
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    }
}

/// SIGKILL the probe's whole tree: its process group plus every descendant
/// found through parent links, so a child that called `setsid` (and left the
/// group) dies too. Runs before the leader is reaped, so neither its pid nor
/// its group id can have been recycled.
#[cfg(unix)]
fn kill_probe_group(child: &std::process::Child) {
    crate::lsp::process_tree::kill_tree(child.id());
}

#[cfg(not(unix))]
fn kill_probe_group(_child: &std::process::Child) {}

/// Whether `command` resolves to a usable language-server executable. Bounded
/// by [`COMMAND_PROBE_TIMEOUT`] when the check has to execute the command.
pub fn is_command_available(command: String) -> Result<bool, String> {
    let command = resolve_known_server_command(&command);
    if is_rejected_shell(&command) {
        return Ok(false);
    }
    if typescript_cli_from_command(&command).is_some() {
        return Ok(current_node_command().is_some());
    }
    if is_rust_analyzer_command(&command) {
        return Ok(probe_command_succeeds(
            &command,
            &["--version"],
            COMMAND_PROBE_TIMEOUT,
        ));
    }
    if command
        == std::env::current_exe()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default()
    {
        return Ok(true);
    }
    if Path::new(&command).is_absolute() {
        return Ok(is_executable_path(Path::new(&command)));
    }
    which::which(&command)
        .map(|path| is_executable_path(&path))
        .or_else(|err| match err {
            which::Error::CannotFindBinaryPath => Ok(false),
            other => Err(other.to_string()),
        })
}

fn spec_for_file(file_path: &str) -> Option<ServerSpec> {
    spec_for_extension(&extension_key(file_path)?)
}

fn extension_key(file_path: &str) -> Option<String> {
    Path::new(file_path)
        .extension()
        .map(|ext| format!(".{}", ext.to_string_lossy().to_ascii_lowercase()))
}

fn spec_for_extension(extension: &str) -> Option<ServerSpec> {
    let spec = match extension {
        ".ts" | ".mts" | ".cts" => ServerSpec {
            language_id: "typescript",
            command: "typescript-language-server",
            args: &["--stdio"],
            env_var: Some("OCTOCODE_TS_SERVER_PATH"),
        },
        ".tsx" => ServerSpec {
            language_id: "typescriptreact",
            command: "typescript-language-server",
            args: &["--stdio"],
            env_var: Some("OCTOCODE_TS_SERVER_PATH"),
        },
        ".js" | ".mjs" | ".cjs" => ServerSpec {
            language_id: "javascript",
            command: "typescript-language-server",
            args: &["--stdio"],
            env_var: Some("OCTOCODE_TS_SERVER_PATH"),
        },
        ".jsx" => ServerSpec {
            language_id: "javascriptreact",
            command: "typescript-language-server",
            args: &["--stdio"],
            env_var: Some("OCTOCODE_TS_SERVER_PATH"),
        },
        ".py" | ".pyi" => ServerSpec {
            language_id: "python",
            command: "pylsp",
            args: &[],
            env_var: Some("OCTOCODE_PYTHON_SERVER_PATH"),
        },
        ".go" => ServerSpec {
            language_id: "go",
            command: "gopls",
            args: &["serve"],
            env_var: Some("OCTOCODE_GO_SERVER_PATH"),
        },
        ".rs" => ServerSpec {
            language_id: "rust",
            command: "rust-analyzer",
            args: &[],
            env_var: Some("OCTOCODE_RUST_SERVER_PATH"),
        },
        ".java" => ServerSpec {
            language_id: "java",
            command: "jdtls",
            args: &[],
            env_var: Some("OCTOCODE_JAVA_SERVER_PATH"),
        },
        ".c" | ".h" => ServerSpec {
            language_id: "c",
            command: "clangd",
            args: &[],
            env_var: Some("OCTOCODE_CLANGD_SERVER_PATH"),
        },
        ".cpp" | ".cc" | ".cxx" | ".hpp" | ".hh" | ".hxx" => ServerSpec {
            language_id: "cpp",
            command: "clangd",
            args: &[],
            env_var: Some("OCTOCODE_CLANGD_SERVER_PATH"),
        },
        ".cu" | ".cuh" => ServerSpec {
            language_id: "cuda",
            command: "clangd",
            args: &[],
            env_var: Some("OCTOCODE_CLANGD_SERVER_PATH"),
        },
        ".cs" => ServerSpec {
            language_id: "csharp",
            command: "csharp-ls",
            args: &[],
            env_var: Some("OCTOCODE_CSHARP_SERVER_PATH"),
        },
        ".scala" | ".sc" | ".sbt" => ServerSpec {
            language_id: "scala",
            command: "metals",
            args: &[],
            env_var: Some("OCTOCODE_SCALA_SERVER_PATH"),
        },
        _ => return None,
    };
    Some(spec)
}

fn user_server_for_extension(
    extension: &str,
    workspace_root: &str,
    options: &LspDiscoveryOptions,
) -> Option<JsLanguageServerConfig> {
    for config_path in user_config_paths(workspace_root, options) {
        let Ok(content) = std::fs::read_to_string(config_path) else {
            continue;
        };
        let Ok(parsed) = serde_json::from_str::<UserConfigFile>(&content) else {
            continue;
        };
        let Some(server) = parsed.language_servers.get(extension) else {
            continue;
        };
        if is_rejected_shell(&server.command)
            || is_interpreter_eval_launch(&server.command, &server.args)
        {
            continue;
        }
        let trust_workspace = options.trust_project_config || project_lsp_config_trusted();
        let (command, args) = resolve_server_invocation(
            &server.command,
            server.args.clone(),
            workspace_root,
            trust_workspace,
        );
        return Some(JsLanguageServerConfig {
            command,
            args: Some(args),
            workspace_root: workspace_root.to_owned(),
            language_id: Some(server.language_id.clone()),
            initialization_options: server.initialization_options.clone(),
            env: None,
            max_memory_mb: None,
        });
    }
    None
}

fn user_config_paths(workspace_root: &str, options: &LspDiscoveryOptions) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(path) = options.config_path.as_ref() {
        paths.push(path.clone());
    }
    if options.trust_project_config {
        paths.push(Path::new(workspace_root).join(".octocode/lsp-servers.json"));
    }
    if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        paths.push(PathBuf::from(home).join(".octocode/lsp-servers.json"));
    }
    paths
}

fn project_lsp_config_trusted() -> bool {
    std::env::var("OCTOCODE_TRUST_PROJECT_LSP_CONFIG")
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

fn is_interpreter_eval_launch(command: &str, args: &[String]) -> bool {
    if !is_generic_interpreter_command(command) {
        return false;
    }
    args.iter().any(|arg| {
        matches!(
            arg.as_str(),
            "-e" | "--eval" | "-c" | "--command" | "-p" | "--print"
        )
    })
}

fn is_generic_interpreter_command(command: &str) -> bool {
    let Some(stem) = Path::new(command)
        .file_stem()
        .and_then(|name| name.to_str())
    else {
        return false;
    };
    let normalized = stem.to_ascii_lowercase();
    normalized == "node"
        || normalized == "python"
        || normalized.starts_with("python3")
        || normalized == "ruby"
        || normalized == "perl"
        || normalized == "php"
}

/// `rust-analyzer` or `rust-analyzer.exe` by file name (any directory).
pub(crate) fn is_rust_analyzer_command(command: &str) -> bool {
    Path::new(command)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            let name = name.to_ascii_lowercase();
            name == "rust-analyzer" || name == "rust-analyzer.exe"
        })
}

/// Resolve a server command to `(program, args)`. `trust_workspace` is the
/// explicit opt-in (`OCTOCODE_TRUST_PROJECT_LSP_CONFIG` / trusted project
/// config) that lets the workspace's own `node_modules` supply the
/// TypeScript server; see [`resolve_typescript_server_cli`].
fn resolve_server_invocation(
    command: &str,
    args: Vec<String>,
    workspace_root: &str,
    trust_workspace: bool,
) -> (String, Vec<String>) {
    let current_dir = std::env::current_dir().ok();
    let managed_start = std::env::current_exe()
        .ok()
        .map(|exe| std::fs::canonicalize(&exe).unwrap_or(exe));
    let workspace_trusted =
        trust_workspace || workspace_is_invocation_root(workspace_root, current_dir.as_deref());
    resolve_server_invocation_with_environment(
        command,
        args,
        workspace_root,
        &TypeScriptServerSearch {
            managed_start: managed_start.as_deref(),
            current_dir: current_dir.as_deref(),
            workspace_trusted,
        },
        command_resolves_to_executable(command),
    )
}

/// Where the TypeScript server may be looked up when it is not on `PATH`.
struct TypeScriptServerSearch<'a> {
    /// The running octocode executable: its install tree's `node_modules`
    /// (octocode ships `typescript-language-server`) is octocode-managed.
    managed_start: Option<&'a Path>,
    /// The invocation directory: the user's own project.
    current_dir: Option<&'a Path>,
    /// Whether the workspace's own `node_modules` may supply the server.
    workspace_trusted: bool,
}

/// `true` when `workspace_root` is the directory octocode was invoked in, or
/// inside it — the user's own project rather than an arbitrary scanned
/// checkout (a clone under the clone cache, say). An invocation directory
/// that is the filesystem root or the home directory proves nothing and
/// never counts.
fn workspace_is_invocation_root(workspace_root: &str, current_dir: Option<&Path>) -> bool {
    let Some(cwd) = current_dir.map(|cwd| std::fs::canonicalize(cwd).unwrap_or(cwd.to_path_buf()))
    else {
        return false;
    };
    if cwd.parent().is_none() {
        return false;
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .map(|home| std::fs::canonicalize(&home).unwrap_or(home));
    if home.as_deref() == Some(cwd.as_path()) {
        return false;
    }
    let workspace = Path::new(workspace_root);
    std::fs::canonicalize(workspace)
        .unwrap_or_else(|_| workspace.to_path_buf())
        .starts_with(&cwd)
}

fn resolve_server_invocation_with_environment(
    command: &str,
    args: Vec<String>,
    workspace_root: &str,
    search: &TypeScriptServerSearch<'_>,
    command_available_on_path: bool,
) -> (String, Vec<String>) {
    if let Some(cli_path) =
        resolve_typescript_server_cli(command, workspace_root, search, command_available_on_path)
        && let Some(node_command) = current_node_command()
    {
        let mut resolved_args = Vec::with_capacity(args.len() + 1);
        resolved_args.push(cli_path.to_string_lossy().into_owned());
        resolved_args.extend(args);
        return (node_command, resolved_args);
    }

    (resolve_known_server_command(command), args)
}

fn resolve_known_server_command(command: &str) -> String {
    if Path::new(command).is_absolute() || command_resolves_to_executable(command) {
        return command.to_owned();
    }
    match command {
        "pylsp" => find_python_user_script("pylsp").unwrap_or_else(|| command.to_owned()),
        _ => command.to_owned(),
    }
}

/// The `typescript-language-server` CLI to run under node when the command
/// is not on `PATH`. Safe by default: a scanned checkout must not be able to
/// swap in an executable it ships, so the lookup order is an explicit
/// `cli.mjs` path, then octocode's own install tree (ancestors of the running
/// executable), then — only when `workspace_trusted` — the workspace's
/// `node_modules`, then the invocation directory's (the user's own project).
fn resolve_typescript_server_cli(
    command: &str,
    workspace_root: &str,
    search: &TypeScriptServerSearch<'_>,
    command_available_on_path: bool,
) -> Option<PathBuf> {
    const CLI: &str = "typescript-language-server/lib/cli.mjs";
    if !is_typescript_server_command(command) {
        return None;
    }
    if command_available_on_path || is_executable_path(Path::new(command)) {
        return None;
    }
    typescript_cli_from_command(command)
        .or_else(|| {
            search
                .managed_start
                .and_then(|exe| exe.parent())
                .and_then(|dir| find_node_module_file_from(dir, CLI))
        })
        .or_else(|| {
            search
                .workspace_trusted
                .then(|| find_node_module_file_from(Path::new(workspace_root), CLI))
                .flatten()
        })
        .or_else(|| {
            search
                .current_dir
                .and_then(|cwd| find_node_module_file_from(cwd, CLI))
        })
}

fn is_typescript_server_command(command: &str) -> bool {
    let path = Path::new(command);
    let file_name = path.file_name().and_then(|name| name.to_str());
    matches!(file_name, Some("typescript-language-server" | "cli.mjs"))
        && command.contains("typescript-language-server")
}

fn typescript_cli_from_command(command: &str) -> Option<PathBuf> {
    let path = Path::new(command);
    if !path.exists() {
        return None;
    }
    let candidate = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if candidate.file_name().and_then(|name| name.to_str()) == Some("cli.mjs")
        && candidate
            .to_string_lossy()
            .contains("typescript-language-server")
    {
        return Some(candidate);
    }
    None
}

fn find_node_module_file_from(start: &Path, package_relative_path: &str) -> Option<PathBuf> {
    for ancestor in start.ancestors() {
        let candidate = ancestor.join("node_modules").join(package_relative_path);
        if candidate.exists() {
            return Some(std::fs::canonicalize(&candidate).unwrap_or(candidate));
        }
    }
    None
}

pub(super) fn current_node_command() -> Option<String> {
    std::env::current_exe()
        .ok()
        .filter(|path| is_executable_path(path) && is_node_executable(path))
        .or_else(|| which::which("node").ok())
        .filter(|path| is_executable_path(path))
        .map(|path| std::fs::canonicalize(&path).unwrap_or(path))
        .map(|path| path.to_string_lossy().into_owned())
}

fn is_node_executable(path: &Path) -> bool {
    path.file_stem()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("node"))
}

fn find_python_user_script(script_name: &str) -> Option<String> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    let home = PathBuf::from(home);
    for candidate in [home.join(".local/bin").join(script_name)] {
        if candidate.exists() {
            return Some(candidate.to_string_lossy().into_owned());
        }
    }

    let python_dir = home.join("Library/Python");
    let Ok(entries) = std::fs::read_dir(python_dir) else {
        return None;
    };
    for entry in entries.flatten() {
        let candidate = entry.path().join("bin").join(script_name);
        if candidate.exists() {
            return Some(candidate.to_string_lossy().into_owned());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{
        LspDiscoveryOptions, command_is_tsgo, command_resolves_to_executable, current_node_command,
        default_server_for_file, default_server_for_file_with_options,
        default_server_for_workspace_root_with_options, detect_language_id, is_command_available,
        is_node_executable, is_rust_analyzer_command, resolve_known_server_command,
        resolve_server_invocation, resolve_server_invocation_with_environment,
        workspace_root_representative_source,
    };
    use serde_json::json;
    use std::path::PathBuf;

    #[test]
    fn recognizes_tsgo_command_by_stem() {
        assert!(command_is_tsgo("tsgo"));
        assert!(command_is_tsgo("/usr/local/bin/tsgo"));
        assert!(command_is_tsgo("TSGO")); // case-insensitive
        assert!(!command_is_tsgo("typescript-language-server"));
        assert!(!command_is_tsgo(
            "/opt/node_modules/.bin/typescript-language-server"
        ));
        assert!(!command_is_tsgo("tsserver"));
    }

    #[test]
    fn node_launcher_never_treats_the_native_octocode_binary_as_node() {
        assert!(is_node_executable(std::path::Path::new(
            "/usr/local/bin/node"
        )));
        assert!(is_node_executable(std::path::Path::new("C:/node.exe")));
        assert!(!is_node_executable(std::path::Path::new(
            "/usr/local/bin/octocode"
        )));
    }

    #[test]
    fn routes_scala_to_metals_without_spurious_stdio_arguments() {
        for extension in [".scala", ".sc", ".sbt"] {
            let spec = super::spec_for_extension(extension).expect("Scala provider");
            assert_eq!(spec.language_id, "scala");
            assert_eq!(spec.command, "metals");
            assert!(spec.args.is_empty());
            assert_eq!(spec.env_var, Some("OCTOCODE_SCALA_SERVER_PATH"));
        }
    }

    #[test]
    fn detects_protocol_language_ids_with_grammar_fallback() {
        let cases = [
            ("demo.ts", "typescript"),
            ("demo.tsx", "typescriptreact"),
            ("demo.js", "javascript"),
            ("demo.jsx", "javascriptreact"),
            ("demo.JSX", "javascriptreact"),
            ("demo.py", "python"),
            ("demo.go", "go"),
            ("demo.rs", "rust"),
            ("demo.java", "java"),
            ("demo.c", "c"),
            ("demo.cpp", "cpp"),
            ("demo.cu", "cuda"),
            ("demo.cuh", "cuda"),
            ("demo.asm", "asm"),
            ("demo.assembly", "asm"),
            ("demo.S", "asm"),
            ("demo.cs", "csharp"),
            ("demo.scala", "scala"),
            ("demo.sbt", "scala"),
        ];

        for (file_name, expected) in cases {
            assert_eq!(
                detect_language_id(file_name.to_owned()).as_deref(),
                Some(expected),
                "{file_name}"
            );
        }
    }

    #[test]
    fn builtin_routes_intersect_enabled_grammars_exactly() {
        let mut expected = vec![
            "c", "cjs", "cts", "go", "h", "java", "js", "jsx", "mjs", "mts", "py", "pyi", "rs",
            "ts", "tsx",
        ];
        if cfg!(feature = "tree-sitter-cpp") {
            expected.extend(["cc", "cpp", "cxx", "hh", "hpp", "hxx"]);
        }
        if cfg!(feature = "tree-sitter-c-sharp") {
            expected.push("cs");
        }
        if cfg!(feature = "tree-sitter-cuda") {
            expected.extend(["cu", "cuh"]);
        }
        if cfg!(feature = "tree-sitter-scala") {
            expected.extend(["sbt", "sc", "scala"]);
        }
        expected.sort_unstable();
        let mut actual: Vec<_> = crate::signatures::languages::supported_extensions()
            .into_iter()
            .filter(|extension| super::spec_for_extension(&format!(".{extension}")).is_some())
            .collect();
        actual.sort_unstable();
        assert_eq!(actual, expected, "built-in LSP route inventory drift");
    }

    #[test]
    fn all_cpp_extensions_resolve_to_clangd() {
        for extension in ["cpp", "cc", "cxx", "hpp", "hh", "hxx"] {
            let config =
                default_server_for_file(format!("fixture.{extension}"), "/workspace".to_owned())
                    .expect(extension);
            assert_eq!(config.command, "clangd");
            assert_eq!(config.language_id.as_deref(), Some("cpp"));
        }
    }

    #[test]
    fn workspace_root_infers_its_server_from_project_markers() {
        let base = std::env::temp_dir().join(format!(
            "octocode-lsp-root-markers-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos())
                .unwrap_or(0)
        ));
        let cases = [
            ("rust", &["Cargo.toml"][..], "rust"),
            ("ts", &["tsconfig.json"][..], "typescript"),
            ("node", &["package.json"][..], "typescript"),
            // A tsconfig is the strongest TypeScript signal; Cargo beats a bare
            // package.json (npm wrappers around Rust crates).
            ("mixed", &["package.json", "Cargo.toml"][..], "rust"),
        ];
        let options = LspDiscoveryOptions::default();
        for (name, markers, expected) in cases {
            let root = base.join(name);
            std::fs::create_dir_all(&root).expect("root");
            for marker in markers {
                std::fs::write(root.join(marker), "").expect("marker");
            }
            let root = root.to_string_lossy().into_owned();
            let config = default_server_for_workspace_root_with_options(root.clone(), &options)
                .unwrap_or_else(|| panic!("{name}: no server inferred"));
            assert_eq!(config.language_id.as_deref(), Some(expected), "{name}");
            assert_eq!(config.workspace_root, root, "{name}");
        }
        let empty = base.join("empty");
        std::fs::create_dir_all(&empty).expect("empty root");
        assert!(
            default_server_for_workspace_root_with_options(
                empty.to_string_lossy().into_owned(),
                &options
            )
            .is_none()
        );
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn workspace_root_representative_source_skips_vendored_dirs() {
        let root = std::env::temp_dir().join(format!(
            "octocode-lsp-root-representative-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(root.join("node_modules/dep")).expect("node_modules");
        std::fs::create_dir_all(root.join("src/nested")).expect("src");
        std::fs::write(root.join("tsconfig.json"), "{}").expect("tsconfig");
        std::fs::write(root.join("node_modules/dep/index.ts"), "").expect("vendored");
        std::fs::write(root.join("README.md"), "").expect("readme");
        std::fs::write(root.join("src/nested/app.ts"), "").expect("source");
        let found = workspace_root_representative_source(&root.to_string_lossy())
            .expect("a representative TypeScript source");
        assert!(found.ends_with("app.ts"), "{found}");

        let empty = root.join("src/nested");
        std::fs::remove_file(empty.join("app.ts")).expect("remove");
        assert!(workspace_root_representative_source(&root.to_string_lossy()).is_none());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn workspace_root_representative_source_prefers_project_include_roots() {
        let root = std::env::temp_dir().join(format!(
            "octocode-lsp-root-include-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(root.join("lib/core")).expect("lib");
        std::fs::create_dir_all(root.join("src")).expect("src");
        // A root-level build script sorts first but is outside the project.
        std::fs::write(root.join("build.mjs"), "").expect("build script");
        std::fs::write(root.join("lib/core/index.ts"), "").expect("included source");
        std::fs::write(root.join("src/other.ts"), "").expect("src source");
        std::fs::write(
            root.join("tsconfig.json"),
            r#"{"include": ["lib/**/*.ts"]}"#,
        )
        .expect("tsconfig");
        let found = workspace_root_representative_source(&root.to_string_lossy())
            .expect("an included TypeScript source");
        assert!(found.ends_with("lib/core/index.ts"), "{found}");

        // Without a parseable include list, `src/` still beats root scripts.
        std::fs::write(root.join("tsconfig.json"), "{ // jsonc\n}").expect("jsonc");
        let found = workspace_root_representative_source(&root.to_string_lossy())
            .expect("a src TypeScript source");
        assert!(found.ends_with("src/other.ts"), "{found}");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn all_cuda_extensions_resolve_to_clangd() {
        for extension in ["cu", "cuh"] {
            let config =
                default_server_for_file(format!("fixture.{extension}"), "/workspace".to_owned())
                    .expect(extension);
            assert_eq!(config.command, "clangd");
            assert_eq!(config.language_id.as_deref(), Some("cuda"));
        }
    }

    #[test]
    fn assembly_server_requires_an_explicit_override_or_config() {
        for extension in ["asm", "assembly", "s", "S"] {
            assert!(
                default_server_for_file(format!("fixture.{extension}"), "/workspace".to_owned())
                    .is_none(),
                ".{extension} must not auto-launch a server"
            );
        }

        let _override = EnvGuard::set("OCTOCODE_ASM_SERVER_PATH", "/opt/asm-lsp");
        for extension in ["asm", "assembly", "s", "S"] {
            let config =
                default_server_for_file(format!("fixture.{extension}"), "/workspace".to_owned())
                    .expect("explicit Assembly server override");
            assert_eq!(config.command, "/opt/asm-lsp");
            assert_eq!(config.args.as_deref(), Some(&[][..]));
            assert_eq!(config.language_id.as_deref(), Some("asm"));
        }
    }

    #[test]
    fn removed_languages_have_no_builtin_server() {
        for extension in [
            "sh", "php", "json", "yaml", "html", "css", "scss", "less", "sql", "swift", "rb", "kt",
            "ex",
        ] {
            assert!(
                default_server_for_file(format!("fixture.{extension}"), "/workspace".to_owned())
                    .is_none(),
                ".{extension} must require trusted custom configuration"
            );
        }
    }

    #[test]
    fn typescript_language_server_remains_the_stable_default() {
        let spec = super::spec_for_extension(".ts").expect("TypeScript provider");
        assert_eq!(spec.command, "typescript-language-server");
        assert_eq!(spec.args, ["--stdio"]);
    }

    #[test]
    fn detects_rust_analyzer_command_names() {
        assert!(is_rust_analyzer_command("rust-analyzer"));
        assert!(is_rust_analyzer_command("/usr/local/bin/rust-analyzer"));
        assert!(!is_rust_analyzer_command("typescript-language-server"));
    }

    #[test]
    fn workspace_lsp_config_is_ignored_by_default() {
        let root = temp_test_root("octocode-engine-untrusted-lsp-config");
        write_lsp_config(
            &root,
            r#"{"languageServers":{".ts":{"command":"node","args":["-e","process.exit(99)"],"languageId":"typescript"}}}"#,
        );

        let Some(root_str) = root.to_str() else {
            panic!("temporary root is not utf-8");
        };
        let config = default_server_for_file("demo.ts".to_owned(), root_str.to_owned())
            .expect("default ts server config");

        assert_ne!(config.command, "node");
        assert_ne!(
            config.args.as_deref(),
            Some(&["-e".to_owned(), "process.exit(99)".to_owned()][..])
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn trusted_workspace_lsp_config_rejects_interpreter_eval_launch() {
        let root = temp_test_root("octocode-engine-trusted-lsp-config-eval");
        write_lsp_config(
            &root,
            r#"{"languageServers":{".ts":{"command":"node","args":["-e","process.exit(99)"],"languageId":"typescript"}}}"#,
        );
        let _guard = EnvGuard::set("OCTOCODE_TRUST_PROJECT_LSP_CONFIG", "true");

        let Some(root_str) = root.to_str() else {
            panic!("temporary root is not utf-8");
        };
        let config = default_server_for_file("demo.ts".to_owned(), root_str.to_owned())
            .expect("default ts server config");

        assert_ne!(config.command, "node");
        assert_ne!(
            config.args.as_deref(),
            Some(&["-e".to_owned(), "process.exit(99)".to_owned()][..])
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn explicit_discovery_options_support_arbitrary_extensions() {
        let root = temp_test_root("octocode-engine-explicit-lsp-config");
        std::fs::create_dir_all(&root).expect("create temporary root");
        let config_path = root.join("native-lsp.json");
        std::fs::write(
            &config_path,
            r#"{"languageServers":{".php":{"command":"custom-php-server","args":["--stdio"],"languageId":"php"}}}"#,
        )
        .expect("write explicit lsp config");
        let root_str = root.to_string_lossy().into_owned();
        let config = default_server_for_file_with_options(
            "demo.php".to_owned(),
            root_str,
            &LspDiscoveryOptions {
                config_path: Some(config_path),
                trust_project_config: false,
            },
        )
        .expect("explicit server config");
        assert_eq!(config.command, "custom-php-server");
        assert_eq!(config.language_id.as_deref(), Some("php"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn env_override_still_wins_over_workspace_lsp_config() {
        let root = temp_test_root("octocode-engine-env-wins-lsp-config");
        write_lsp_config(
            &root,
            r#"{"languageServers":{".ts":{"command":"custom-language-server","args":["--stdio"],"languageId":"typescript"}}}"#,
        );
        let _trust = EnvGuard::set("OCTOCODE_TRUST_PROJECT_LSP_CONFIG", "true");
        let _override = EnvGuard::set("OCTOCODE_TS_SERVER_PATH", "env-ts-server");

        let Some(root_str) = root.to_str() else {
            panic!("temporary root is not utf-8");
        };
        let config = default_server_for_file("demo.ts".to_owned(), root_str.to_owned())
            .expect("default ts server config");

        assert_eq!(config.command, "env-ts-server");

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn keeps_unknown_server_commands_unchanged() {
        assert_eq!(
            resolve_known_server_command("definitely-not-an-octocode-server"),
            "definitely-not-an-octocode-server"
        );
    }

    #[cfg(unix)]
    #[test]
    fn wraps_non_executable_typescript_cli_with_current_node() {
        let root = temp_test_root("octocode-engine-ts-cli");
        let cli = root
            .join("node_modules")
            .join("typescript-language-server")
            .join("lib")
            .join("cli.mjs");
        std::fs::create_dir_all(cli.parent().unwrap_or(&root))
            .expect("create temporary typescript-language-server dir");
        std::fs::write(&cli, "#!/usr/bin/env node\n").expect("write temporary cli");

        let Some(root_str) = root.to_str() else {
            panic!("temporary root is not utf-8");
        };
        let Some(cli_str) = cli.to_str() else {
            panic!("temporary cli path is not utf-8");
        };

        let (command, args) =
            resolve_server_invocation(cli_str, vec!["--stdio".to_owned()], root_str, false);
        assert_eq!(Some(command), current_node_command());
        assert_eq!(args.len(), 2);
        assert_eq!(
            PathBuf::from(&args[0]),
            std::fs::canonicalize(&cli).expect("canonicalize temporary cli")
        );
        assert_eq!(args[1], "--stdio");

        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn resolves_bundled_typescript_server_from_current_dir_for_external_workspace() {
        let workspace_root = temp_test_root("octocode-engine-external-workspace");
        let package_root = temp_test_root("octocode-engine-package-root");
        let cli = package_root
            .join("node_modules")
            .join("typescript-language-server")
            .join("lib")
            .join("cli.mjs");
        std::fs::create_dir_all(&workspace_root).expect("create external workspace");
        std::fs::create_dir_all(cli.parent().unwrap_or(&package_root))
            .expect("create bundled typescript-language-server dir");
        std::fs::write(&cli, "#!/usr/bin/env node\n").expect("write temporary cli");

        let Some(workspace_str) = workspace_root.to_str() else {
            panic!("temporary workspace path is not utf-8");
        };
        let (command, args) = resolve_server_invocation_with_environment(
            "typescript-language-server",
            vec!["--stdio".to_owned()],
            workspace_str,
            &super::TypeScriptServerSearch {
                managed_start: None,
                current_dir: Some(&package_root),
                workspace_trusted: false,
            },
            false,
        );

        assert_eq!(Some(command), current_node_command());
        assert_eq!(
            PathBuf::from(&args[0]),
            std::fs::canonicalize(&cli).expect("canonicalize temporary cli")
        );
        assert_eq!(args[1], "--stdio");

        let _ = std::fs::remove_dir_all(workspace_root);
        let _ = std::fs::remove_dir_all(package_root);
    }

    /// A scanned checkout's own `node_modules` cannot supply the
    /// TypeScript server unless the workspace is trusted (opt-in, or the
    /// user's own invocation directory); octocode's install tree and the
    /// invocation directory are searched first.
    #[cfg(unix)]
    #[test]
    fn workspace_typescript_server_requires_a_trusted_workspace() {
        let base = temp_test_root("octocode-engine-ts-trust");
        let write_cli = |root: &std::path::Path| {
            let cli = root
                .join("node_modules")
                .join("typescript-language-server")
                .join("lib")
                .join("cli.mjs");
            std::fs::create_dir_all(cli.parent().unwrap_or(root)).expect("cli dir");
            std::fs::write(&cli, "#!/usr/bin/env node\n").expect("cli");
            std::fs::canonicalize(&cli).expect("canonical cli")
        };
        let checkout = base.join("clone");
        let checkout_cli = write_cli(&checkout);
        let elsewhere = base.join("elsewhere");
        std::fs::create_dir_all(&elsewhere).expect("elsewhere");
        let checkout_str = checkout.to_string_lossy().into_owned();
        let resolve = |search: &super::TypeScriptServerSearch<'_>| {
            super::resolve_typescript_server_cli(
                "typescript-language-server",
                &checkout_str,
                search,
                false,
            )
        };

        // Untrusted checkout, nothing else: no server (never the checkout's).
        let untrusted = super::TypeScriptServerSearch {
            managed_start: None,
            current_dir: Some(&elsewhere),
            workspace_trusted: false,
        };
        assert_eq!(resolve(&untrusted), None);

        // Trusted (opt-in or invocation root): the workspace's server.
        let trusted = super::TypeScriptServerSearch {
            workspace_trusted: true,
            ..untrusted
        };
        assert_eq!(resolve(&trusted), Some(checkout_cli.clone()));

        // octocode's own install tree wins over the workspace.
        let install = base.join("octocode-install");
        let managed_cli = write_cli(&install);
        let exe = install.join("bin").join("octocode");
        let managed = super::TypeScriptServerSearch {
            managed_start: Some(&exe),
            ..trusted
        };
        assert_eq!(resolve(&managed), Some(managed_cli));

        // On PATH: the command runs as-is.
        assert_eq!(
            super::resolve_typescript_server_cli(
                "typescript-language-server",
                &checkout_str,
                &trusted,
                true
            ),
            None
        );

        // The invocation root: the workspace itself or a directory inside it.
        assert!(super::workspace_is_invocation_root(
            &checkout_str,
            Some(&checkout)
        ));
        assert!(super::workspace_is_invocation_root(
            &checkout.join("node_modules").to_string_lossy(),
            Some(&checkout)
        ));
        assert!(!super::workspace_is_invocation_root(
            &checkout_str,
            Some(&elsewhere)
        ));
        assert!(!super::workspace_is_invocation_root(
            &checkout_str,
            Some(std::path::Path::new("/"))
        ));
        if let Some(home) = std::env::var_os("HOME") {
            let home = PathBuf::from(home);
            assert!(
                !super::workspace_is_invocation_root(
                    &home.join(".octocode/clones/x").to_string_lossy(),
                    Some(&home)
                ),
                "a home-directory invocation does not trust every checkout under it"
            );
        }
        let _ = std::fs::remove_dir_all(base);
    }

    #[cfg(unix)]
    #[test]
    fn absolute_non_executable_non_node_command_is_unavailable() {
        let root = temp_test_root("octocode-engine-non-executable");
        let command = root.join("server");
        std::fs::create_dir_all(&root).expect("create temporary command dir");
        std::fs::write(&command, "not executable\n").expect("write temporary command");

        let Some(command_str) = command.to_str() else {
            panic!("temporary command path is not utf-8");
        };
        assert!(!command_resolves_to_executable(command_str));
        assert!(!is_command_available(command_str.to_owned()).expect("check command availability"));

        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn command_probe_is_bounded_and_kills_a_hung_command() {
        // System binaries, not freshly written scripts: exec'ing a file this
        // process just wrote races parallel tests' forks (ETXTBSY). Bounds are
        // generous so a loaded machine cannot turn scheduling delay into a
        // failure: the property is "returns long before the 30 s sleep ends".
        let started = std::time::Instant::now();
        assert!(!super::probe_command_succeeds(
            "/bin/sleep",
            &["30"],
            std::time::Duration::from_millis(200),
        ));
        assert!(
            started.elapsed() < std::time::Duration::from_secs(20),
            "a hung probe must be killed at its deadline"
        );
        assert!(super::probe_command_succeeds(
            "/bin/sh",
            &["-c", "exit 0"],
            std::time::Duration::from_secs(60),
        ));
        assert!(!super::probe_command_succeeds(
            "/bin/sh",
            &["-c", "exit 3"],
            std::time::Duration::from_secs(60),
        ));
        assert!(!super::probe_command_succeeds(
            "/nonexistent/octocode-probe-missing",
            &[],
            std::time::Duration::from_secs(1),
        ));
    }

    #[cfg(unix)]
    #[test]
    fn command_probe_timeout_kills_the_whole_process_group() {
        // A rustup-style proxy forks the real binary; killing only the
        // leader would leave that child running after the probe returns.
        let root = temp_test_root("octocode-engine-probe-group");
        std::fs::create_dir_all(&root).expect("root");
        let pid_file = root.join("child.pid");
        let script = format!(
            "/bin/sleep 30 & echo $! > '{}'; wait",
            pid_file.to_string_lossy()
        );
        assert!(!super::probe_command_succeeds(
            "/bin/sh",
            &["-c", &script],
            std::time::Duration::from_millis(1_500),
        ));
        let pid: i32 = std::fs::read_to_string(&pid_file)
            .expect("the probe recorded its child pid")
            .trim()
            .parse()
            .expect("numeric pid");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        // SAFETY: signal 0 only checks whether the pid still exists.
        while unsafe { libc::kill(pid, 0) } == 0 {
            assert!(
                std::time::Instant::now() < deadline,
                "the probe's grandchild outlived the timeout"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let _ = std::fs::remove_dir_all(root);
    }

    /// A probe child that calls `setsid` leaves the process group; the
    /// timeout's descendant sweep must still kill it.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn command_probe_timeout_kills_a_setsid_descendant() {
        let has_python = std::process::Command::new("python3")
            .arg("--version")
            .output()
            .is_ok_and(|output| output.status.success());
        if !has_python {
            return;
        }
        let root = temp_test_root("octocode-engine-probe-setsid");
        std::fs::create_dir_all(&root).expect("root");
        let pid_file = root.join("escaped.pid");
        let pid_arg = pid_file.to_string_lossy().into_owned();
        let script = "import os,sys,time\nif os.fork()==0:\n    os.setsid()\n    open(sys.argv[1],'w').write(str(os.getpid()))\n    time.sleep(60)\nelse:\n    time.sleep(60)\n";
        assert!(!super::probe_command_succeeds(
            "python3",
            &["-c", script, &pid_arg],
            std::time::Duration::from_millis(1_500),
        ));
        let pid: i32 = std::fs::read_to_string(&pid_file)
            .expect("the escaped child recorded its pid")
            .trim()
            .parse()
            .expect("numeric pid");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        // SAFETY: signal 0 only checks whether the pid still exists.
        while unsafe { libc::kill(pid, 0) } == 0 {
            if std::time::Instant::now() >= deadline {
                // SAFETY: clean up the leaked process before failing.
                unsafe { libc::kill(pid, libc::SIGKILL) };
                panic!("the setsid child outlived the probe timeout");
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn builtin_rust_route_runs_rust_analyzer_headless_by_default() {
        let config = default_server_for_file_with_options(
            "src/lib.rs".to_owned(),
            "/workspace".to_owned(),
            &LspDiscoveryOptions::default(),
        )
        .expect("rust route");
        let options = config
            .initialization_options
            .expect("headless rust-analyzer options");
        assert_eq!(
            options.pointer("/cargo/buildScripts/enable"),
            Some(&json!(false))
        );
        assert_eq!(options.pointer("/procMacro/enable"), Some(&json!(false)));
        assert_eq!(options.pointer("/checkOnSave"), Some(&json!(false)));
        assert_eq!(options.pointer("/cachePriming/enable"), Some(&json!(false)));
        assert_eq!(options.pointer("/cargo/targetDir"), Some(&json!(true)));
    }

    #[test]
    fn user_rust_analyzer_options_win_over_headless_defaults() {
        let root = temp_test_root("octocode-engine-ra-user-options");
        std::fs::create_dir_all(&root).expect("root");
        let config_path = root.join("lsp.json");
        std::fs::write(
            &config_path,
            r#"{"languageServers":{".rs":{"command":"rust-analyzer","languageId":"rust","initializationOptions":{"procMacro":{"enable":true},"cargo":{"features":"all"}}}}}"#,
        )
        .expect("config");
        let config = default_server_for_file_with_options(
            "src/lib.rs".to_owned(),
            root.to_string_lossy().into_owned(),
            &LspDiscoveryOptions {
                config_path: Some(config_path),
                trust_project_config: false,
            },
        )
        .expect("user rust route");
        let options = config.initialization_options.expect("options");
        // User keys win…
        assert_eq!(options.pointer("/procMacro/enable"), Some(&json!(true)));
        assert_eq!(options.pointer("/cargo/features"), Some(&json!("all")));
        // …and the defaults fill in everything the user left out.
        assert_eq!(
            options.pointer("/cargo/buildScripts/enable"),
            Some(&json!(false))
        );
        assert_eq!(options.pointer("/checkOnSave"), Some(&json!(false)));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn server_defaults_touch_only_rust_analyzer_and_are_idempotent() {
        let mut config = crate::lsp::types::JsLanguageServerConfig {
            command: "/opt/bin/gopls".to_owned(),
            args: None,
            workspace_root: "/w".to_owned(),
            language_id: Some("go".to_owned()),
            initialization_options: None,
            env: None,
            max_memory_mb: None,
        };
        super::apply_server_default_options(&mut config);
        assert!(config.initialization_options.is_none());

        config.command = "C:/tools/Rust-Analyzer.exe".to_owned();
        config.initialization_options = Some(json!({"checkOnSave": true}));
        super::apply_server_default_options(&mut config);
        let once = config.initialization_options.clone();
        super::apply_server_default_options(&mut config);
        assert_eq!(config.initialization_options, once);
        let options = once.expect("options");
        assert_eq!(options.pointer("/checkOnSave"), Some(&json!(true)));
        assert_eq!(options.pointer("/procMacro/enable"), Some(&json!(false)));
    }

    /// clangd, jdtls, and Metals start headless and read-only by
    /// default, on the built-in route and through `apply_server_default_options`;
    /// user args and options still win.
    #[test]
    fn clangd_jdtls_and_metals_routes_start_headless() {
        let options = LspDiscoveryOptions::default();
        let clangd = default_server_for_file_with_options(
            "src/main.cpp".to_owned(),
            "/workspace".to_owned(),
            &options,
        )
        .expect("clangd route");
        let args = clangd.args.expect("clangd args");
        for flag in super::CLANGD_HEADLESS_ARGS {
            assert!(args.iter().any(|arg| arg == flag), "{flag} in {args:?}");
        }

        let jdtls = default_server_for_file_with_options(
            "src/Main.java".to_owned(),
            "/workspace/java".to_owned(),
            &options,
        )
        .expect("jdtls route");
        let args = jdtls.args.expect("jdtls args");
        let data = args
            .iter()
            .position(|arg| arg == "-data")
            .and_then(|index| args.get(index + 1))
            .expect("-data <dir>");
        assert!(
            !std::path::Path::new(data).starts_with("/workspace"),
            "jdtls data stays outside the repository: {data}"
        );
        assert_eq!(
            PathBuf::from(data),
            super::jdtls_data_dir("/workspace/java").expect("data dir")
        );
        assert_ne!(
            super::jdtls_data_dir("/workspace/java"),
            super::jdtls_data_dir("/workspace/other"),
            "one data dir per workspace"
        );
        let jdtls_options = jdtls.initialization_options.expect("jdtls options");
        assert_eq!(
            jdtls_options.pointer("/settings/java/autobuild/enabled"),
            Some(&json!(false))
        );

        let metals = default_server_for_file_with_options(
            "build.sbt".to_owned(),
            "/workspace".to_owned(),
            &options,
        )
        .expect("metals route");
        let metals_options = metals.initialization_options.expect("metals options");
        assert_eq!(metals_options["isHttpEnabled"], json!(false));
        assert_eq!(metals_options["statusBarProvider"], json!("off"));

        // User values win; defaults fill the rest; re-applying is a no-op.
        let mut config = crate::lsp::types::JsLanguageServerConfig {
            command: "/usr/bin/clangd-18".to_owned(),
            args: Some(vec![
                "--background-index".to_owned(),
                "--log=verbose".to_owned(),
            ]),
            workspace_root: "/w".to_owned(),
            language_id: Some("cpp".to_owned()),
            initialization_options: None,
            env: None,
            max_memory_mb: None,
        };
        super::apply_server_default_options(&mut config);
        super::apply_server_default_options(&mut config);
        assert_eq!(
            config.args.as_deref().unwrap_or_default(),
            [
                "--background-index",
                "--log=verbose",
                "--clang-tidy=false",
                "--pch-storage=memory"
            ]
        );
        config.command = "jdtls".to_owned();
        config.args = Some(vec!["-data".to_owned(), "/mine".to_owned()]);
        config.initialization_options =
            Some(json!({"settings": {"java": {"autobuild": {"enabled": true}}}}));
        super::apply_server_default_options(&mut config);
        assert_eq!(
            config.args.as_deref().unwrap_or_default(),
            ["-data", "/mine"]
        );
        let merged = config.initialization_options.clone().expect("options");
        assert_eq!(
            merged.pointer("/settings/java/autobuild/enabled"),
            Some(&json!(true))
        );
        assert_eq!(
            merged.pointer("/settings/java/import/generatesMetadataFilesAtProjectRoot"),
            Some(&json!(false))
        );
    }

    #[cfg(unix)]
    #[test]
    fn python_route_prefers_basedpyright_then_pyright_then_pylsp() {
        use std::os::unix::fs::PermissionsExt;
        let root = temp_test_root("octocode-engine-python-route");
        let path_dir = root.join("path-bin");
        let workspace = root.join("workspace");
        let workspace_bin = workspace.join("node_modules").join(".bin");
        std::fs::create_dir_all(&path_dir).expect("path dir");
        std::fs::create_dir_all(&workspace_bin).expect("workspace bin");
        let fake = |dir: &std::path::Path, name: &str| {
            let path = dir.join(name);
            std::fs::write(&path, "#!/bin/sh\n").expect("fake server");
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
            path
        };
        let workspace_str = workspace.to_string_lossy().into_owned();
        let resolve = || super::resolve_pyright_family(&workspace_str, Some(path_dir.as_os_str()));

        // Nothing installed: the caller falls back to pylsp.
        assert_eq!(resolve(), None);

        // A checkout-shipped server is never picked automatically.
        fake(&workspace_bin, "pyright-langserver");
        fake(&workspace_bin, "basedpyright-langserver");
        assert_eq!(resolve(), None);

        // pyright on PATH is found.
        let path_pyright = fake(&path_dir, "pyright-langserver");
        let (command, args) = resolve().expect("PATH pyright");
        assert_eq!(PathBuf::from(command), path_pyright);
        assert_eq!(args, ["--stdio"]);

        // basedpyright beats pyright.
        let path_based = fake(&path_dir, "basedpyright-langserver");
        assert_eq!(
            resolve().map(|(c, _)| PathBuf::from(c)),
            Some(path_based.clone())
        );

        // A non-executable file is not a server.
        std::fs::set_permissions(&path_based, std::fs::Permissions::from_mode(0o644))
            .expect("chmod");
        assert_eq!(
            resolve().map(|(c, _)| PathBuf::from(c)),
            Some(path_dir.join("pyright-langserver"))
        );
        let _ = std::fs::remove_dir_all(root);
    }

    fn temp_test_root(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("{name}-{}-{nanos}", std::process::id()))
    }

    fn write_lsp_config(root: &std::path::Path, json: &str) {
        let config_dir = root.join(".octocode");
        std::fs::create_dir_all(&config_dir).expect("create .octocode dir");
        std::fs::write(config_dir.join("lsp-servers.json"), json).expect("write lsp config");
    }

    struct EnvGuard {
        key: &'static str,
        previous: Option<String>,
    }

    impl EnvGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let previous = std::env::var(key).ok();
            // SAFETY: test-only guard. Process-env mutation is unsafe in a
            // multithreaded program because it races concurrent env readers;
            // these LSP-config tests mutate env only through this guard and do
            // not run alongside other env access, and the guard restores the
            // prior value on drop.
            unsafe { std::env::set_var(key, value) };
            Self { key, previous }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            // SAFETY: see `EnvGuard::set` — the same test-only, no-concurrent-env
            // invariant holds for the restore path.
            if let Some(previous) = &self.previous {
                unsafe { std::env::set_var(self.key, previous) };
            } else {
                unsafe { std::env::remove_var(self.key) };
            }
        }
    }
}
