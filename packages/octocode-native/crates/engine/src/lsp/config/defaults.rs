use crate::lsp::types::LspLanguageServerConfig;
use serde_json::{Value, json};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

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
        "cachePriming": { "enable": false },
        // The client sends no `workspace/didChangeWatchedFiles`: rust-analyzer
        // watches the workspace itself, so an edit to a file no request
        // opened still reaches its index.
        "files": { "watcher": "server" }
    })
}

/// clangd flags that keep it read-only and quiet: no background index (it
/// writes `.cache/clangd/` into the repository), no clang-tidy, errors-only
/// logging, and precompiled preambles in memory rather than temp files.
pub(crate) const CLANGD_HEADLESS_ARGS: &[&str] = &[
    "--background-index=false",
    "--clang-tidy=false",
    "--log=error",
    "--pch-storage=memory",
];

/// What clangd resolves a file's compile command from, relative to the file's
/// directory and each ancestor: a compilation database, its `build/` variant,
/// a flags file, and the `.clangd` project config.
pub(crate) const CLANGD_PROJECT_CONFIG_FILES: &[&str] = &[
    "compile_commands.json",
    "build/compile_commands.json",
    "compile_flags.txt",
    ".clangd",
];

/// The files a server reads once per project and never again for a document
/// it already holds, so a pooled server must be replaced when one changes
/// (see `lsp::project_config`). Empty for a server that re-reads its own
/// configuration.
pub(crate) fn project_config_files(command: &str) -> &'static [&'static str] {
    match server_stem(command).as_deref() {
        Some("clangd") => CLANGD_PROJECT_CONFIG_FILES,
        _ => &[],
    }
}

/// clangd `initializationOptions.fallbackFlags` for CUDA: the flags of the
/// command clangd invents (`clang <file>`) for a file with no compile
/// command, where they replace that command's missing language and target.
///
/// - `-xcuda`: clang takes `.cuh` for a CUDA *header* and plans no compile
///   job for it, so clangd reports `invalid AST` for every request.
/// - `--cuda-host-only`: without it the driver plans host and device jobs
///   and needs a GPU architecture and `libdevice` to plan the device one.
///   Host-only is one job on any machine, with or without a CUDA SDK; with
///   one installed clang still finds it and force-includes its headers.
///
/// They are used only when no compile command applies: a compilation
/// database or `compile_flags.txt` entry overrides them completely.
pub(crate) const CLANGD_CUDA_FALLBACK_FLAGS: &[&str] = &["-xcuda", "--cuda-host-only"];

/// clangd `initializationOptions` by language.
fn clangd_default_options(language_id: Option<&str>) -> Option<Value> {
    (language_id == Some("cuda")).then(|| json!({ "fallbackFlags": CLANGD_CUDA_FALLBACK_FLAGS }))
}

/// jdtls settings (sent as `initializationOptions.settings`): no automatic
/// workspace build, and Eclipse metadata (`.project`, `.classpath`,
/// `.settings/`) kept in the `-data` directory, not the project root.
pub(crate) fn jdtls_headless_options() -> Value {
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
pub(crate) fn metals_headless_options() -> Value {
    json!({
        "isHttpEnabled": false,
        "statusBarProvider": "off"
    })
}

/// typescript-language-server `initializationOptions` for a headless,
/// read-only run: no network typings acquisition, one tsserver process (no
/// separate syntax server), no package.json auto-import scan, and a V8 heap
/// bound 512 MiB under the memory `cap_mb` so the GC works before the
/// watchdog fires (no bound when the cap is disabled).
pub(crate) fn typescript_headless_options(cap_mb: Option<u32>) -> Value {
    let mut options = json!({
        "disableAutomaticTypingAcquisition": true,
        "tsserver": { "useSyntaxServer": "never" },
        "preferences": { "includePackageJsonAutoImports": "off" }
    });
    if let Some(cap) = cap_mb {
        options["maxTsServerMemory"] = json!(cap.saturating_sub(512).max(1024));
    }
    options
}

/// The per-workspace jdtls data directory, outside the repository:
/// `<octocode home>/lsp-workspaces/jdtls/<sha256(workspace root)[..16]>`.
pub(crate) fn jdtls_data_dir(octocode_home: &Path, workspace_root: &str) -> PathBuf {
    let digest = crate::digest::sha256(workspace_root.as_bytes());
    octocode_home
        .join("lsp-workspaces")
        .join("jdtls")
        .join(&digest[..16])
}

/// Whether the server answers overlapping requests in parallel, so a caller
/// with many independent requests may pipeline them. tsgo does; tsserver
/// (behind typescript-language-server) works through them one at a time,
/// where queueing only spends each request's timeout waiting.
pub fn serves_requests_concurrently(config: &LspLanguageServerConfig) -> bool {
    server_stem(&config.command).as_deref() == Some("tsgo")
}

/// Most `workspace/symbol` results the server returns for one query, for a
/// server that drops the rest without saying so: tsgo answers
/// `min(n, 256)` (typescript-go `internal/ls/symbols.go`).
pub fn workspace_symbol_limit(config: &LspLanguageServerConfig) -> Option<usize> {
    (server_stem(&config.command).as_deref() == Some("tsgo")).then_some(TSGO_WORKSPACE_SYMBOL_LIMIT)
}

const TSGO_WORKSPACE_SYMBOL_LIMIT: usize = 256;

/// Apply per-server safe defaults to a resolved launch configuration, on
/// every launch path (discovery, runtime). rust-analyzer runs no
/// repository code (build scripts, proc-macros, `cargo check`); clangd writes
/// no index into the repository; jdtls keeps its workspace data outside it
/// and does not auto-build; Metals opens no HTTP port. Defaults sit
/// *underneath* the configuration: an `initializationOptions` key the user
/// (or `rustContext`) set wins, and a flag the user already passed (by name,
/// before any `=`) is not added again. A non-object user value is left
/// untouched. Idempotent. Without `octocode_home` (a launch outside
/// discovery) jdtls gets no `-data` directory.
pub(crate) fn apply_server_default_options(
    config: &mut LspLanguageServerConfig,
    octocode_home: Option<&Path>,
) {
    match server_stem(&config.command).as_deref() {
        Some("rust-analyzer") => apply_rust_analyzer_defaults(config),
        Some("clangd") => {
            add_default_args(config, CLANGD_HEADLESS_ARGS);
            if let Some(options) = clangd_default_options(config.language_id.as_deref()) {
                merge_default_options(config, options);
            }
        }
        Some("jdtls") => {
            let has_data = config
                .args
                .as_deref()
                .is_some_and(|args| args.iter().any(|arg| arg == "-data"));
            if !has_data && let Some(home) = octocode_home {
                let dir = jdtls_data_dir(home, &config.workspace_root);
                let args = config.args.get_or_insert_with(Vec::new);
                args.push("-data".to_owned());
                args.push(dir.to_string_lossy().into_owned());
            }
            merge_default_options(config, jdtls_headless_options());
        }
        Some("tsgo") => {
            let cap = crate::lsp::spawn_limits::effective_memory_cap_mb(config.max_memory_mb);
            if let Some(cap) = cap {
                config
                    .env
                    .get_or_insert_with(Default::default)
                    .entry("GOMEMLIMIT".to_owned())
                    .or_insert_with(|| format!("{}MiB", cap.saturating_sub(512).max(1024)));
            }
        }
        Some("metals") => merge_default_options(config, metals_headless_options()),
        Some("typescript-language-server") => {
            let cap = crate::lsp::spawn_limits::effective_memory_cap_mb(config.max_memory_mb);
            merge_default_options(config, typescript_headless_options(cap));
        }
        _ if runs_typescript_language_server(config) => {
            let cap = crate::lsp::spawn_limits::effective_memory_cap_mb(config.max_memory_mb);
            merge_default_options(config, typescript_headless_options(cap));
        }
        _ => {}
    }
}

/// The server runs `typescript-language-server`'s CLI under `node`.
fn runs_typescript_language_server(config: &LspLanguageServerConfig) -> bool {
    config.args.iter().flatten().any(|arg| {
        Path::new(arg).file_name().and_then(OsStr::to_str) == Some("cli.mjs")
            && arg.contains("typescript-language-server")
    })
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
fn add_default_args(config: &mut LspLanguageServerConfig, defaults: &[&str]) {
    let args = config.args.get_or_insert_with(Vec::new);
    let flag_name = |arg: &str| arg.split_once('=').map_or(arg, |(name, _)| name).to_owned();
    for default in defaults {
        let name = flag_name(default);
        if !args.iter().any(|arg| flag_name(arg) == name) {
            args.push((*default).to_owned());
        }
    }
}

fn merge_default_options(config: &mut LspLanguageServerConfig, defaults: Value) {
    config.initialization_options = Some(match config.initialization_options.take() {
        None | Some(Value::Null) => defaults,
        Some(user @ Value::Object(_)) => merge_under(defaults, user),
        Some(other) => other,
    });
}

pub(super) fn apply_rust_analyzer_defaults(config: &mut LspLanguageServerConfig) {
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
