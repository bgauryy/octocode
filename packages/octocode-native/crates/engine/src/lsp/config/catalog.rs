use super::defaults::apply_server_default_options;
use super::invocation::{config_from_spec, is_interpreter_eval_launch, resolve_server_invocation};
use super::probe::server_command_resolves;
use super::types::{LspDiscoveryOptions, ServerSpec};
use crate::lsp::commands::is_rejected_shell;
use crate::lsp::grammar::grammar_for_file;
use crate::lsp::module_path::install_anchor;
use crate::lsp::types::{LspLanguageServerConfig, ServerSource};
use crate::text::extension_of;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

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
    /// Resident-memory cap for this server's process tree, in MiB; unset
    /// takes the host-derived default. `0` disables the cap.
    #[serde(rename = "maxMemoryMb")]
    max_memory_mb: Option<u32>,
    /// Extra environment variables for the server process.
    env: Option<HashMap<String, String>>,
}

pub fn detect_language_id(file_path: &str) -> Option<String> {
    // The protocol ID can differ from the parser grammar: JSX uses the
    // JavaScript grammar but must be opened as javascriptreact by the server.
    spec_for_file(file_path)
        .map(|spec| spec.language_id.to_owned())
        .or_else(|| grammar_for_file(file_path).map(|spec| spec.language_id.to_owned()))
}

/// The language server for `file_path`: a user `lsp-servers.json` entry,
/// then the built-in server (a managed install before `PATH`).
pub fn default_server_for_file(
    file_path: &str,
    workspace_root: &str,
    options: &LspDiscoveryOptions,
) -> Option<LspLanguageServerConfig> {
    default_server_for_file_from(
        file_path,
        workspace_root,
        options,
        install_anchor().as_deref(),
    )
}

/// [`default_server_for_file`] with octocode's install tree (where the
/// bundled servers live) named by `anchor`.
pub(super) fn default_server_for_file_from(
    file_path: &str,
    workspace_root: &str,
    options: &LspDiscoveryOptions,
    anchor: Option<&Path>,
) -> Option<LspLanguageServerConfig> {
    server_for_file(file_path, workspace_root, options, anchor, true)
}

/// The TypeScript server octocode falls back to when the native `tsgo` route
/// for `file_path` fails to start: `Some` only where that route is the
/// built-in choice, never for a user `lsp-servers.json` entry (the user's
/// explicit server gets no silent substitute) or a file tsgo does not serve.
pub fn typescript_server_fallback(
    file_path: &str,
    workspace_root: &str,
    options: &LspDiscoveryOptions,
) -> Option<LspLanguageServerConfig> {
    typescript_server_fallback_from(
        file_path,
        workspace_root,
        options,
        install_anchor().as_deref(),
    )
}

pub(super) fn typescript_server_fallback_from(
    file_path: &str,
    workspace_root: &str,
    options: &LspDiscoveryOptions,
    anchor: Option<&Path>,
) -> Option<LspLanguageServerConfig> {
    let native = server_for_file(file_path, workspace_root, options, anchor, true)?;
    let tsserver = server_for_file(file_path, workspace_root, options, anchor, false)?;
    (native.command != tsserver.command || native.args != tsserver.args).then_some(tsserver)
}

/// `native_typescript: false` skips the tsgo routes, leaving tsserver.
fn server_for_file(
    file_path: &str,
    workspace_root: &str,
    options: &LspDiscoveryOptions,
    anchor: Option<&Path>,
    native_typescript: bool,
) -> Option<LspLanguageServerConfig> {
    let extension = extension_key(file_path)?;

    // Assembly has no built-in default server (ARCHITECTURE: it requires trusted
    // custom configuration): only a user `lsp-servers.json` entry launches one.
    if matches!(extension.as_str(), ".asm" | ".assembly" | ".s") {
        return user_server_for_extension(&extension, workspace_root, options);
    }

    let spec = spec_for_extension(&extension);

    if let Some(mut config) = user_server_for_extension(&extension, workspace_root, options) {
        apply_server_default_options(&mut config, options.octocode_home.as_deref());
        return Some(config);
    }

    spec.map(|spec| config_from_spec(spec, workspace_root, options, anchor, native_typescript))
}

/// Short catalog labels and the representative extension that selects each
/// language's server, in the order agents see them.
const SERVER_LANGUAGE_LABELS: &[(&str, &str)] = &[
    ("ts/js", ".ts"),
    ("py", ".py"),
    ("rust", ".rs"),
    ("c/c++", ".c"),
    ("go", ".go"),
    ("c#", ".cs"),
    ("java", ".java"),
];

/// Labels (`ts/js`, `py`, `rust`, `c/c++`, `go`, `c#`, `java`) of languages
/// whose language server resolves to an executable for `workspace_root`,
/// through the discovery ladder `lspSearch` uses (env overrides, user config,
/// managed installs, pyright family, octocode's TypeScript server).
/// Resolution only: no server is spawned and no command is executed (a
/// rustup proxy counts only when a toolchain ships the component).
#[must_use]
pub fn available_server_languages(
    workspace_root: &str,
    options: &LspDiscoveryOptions,
) -> Vec<&'static str> {
    server_languages_with(
        |extension| {
            let representative = Path::new(workspace_root)
                .join(format!("workspace{extension}"))
                .to_string_lossy()
                .into_owned();
            default_server_for_file(&representative, workspace_root, options)
        },
        server_command_resolves,
    )
}

pub(super) fn server_languages_with(
    resolve: impl Fn(&str) -> Option<LspLanguageServerConfig>,
    resolves: impl Fn(&LspLanguageServerConfig) -> bool,
) -> Vec<&'static str> {
    SERVER_LANGUAGE_LABELS
        .iter()
        .filter(|(_, extension)| resolve(extension).is_some_and(|config| resolves(&config)))
        .map(|(label, _)| *label)
        .collect()
}

fn spec_for_file(file_path: &str) -> Option<ServerSpec> {
    spec_for_extension(&extension_key(file_path)?)
}

fn extension_key(file_path: &str) -> Option<String> {
    let extension = extension_of(file_path, true, "");
    (!extension.is_empty()).then(|| format!(".{extension}"))
}

pub(super) fn spec_for_extension(extension: &str) -> Option<ServerSpec> {
    let spec = match extension {
        ".ts" | ".mts" | ".cts" => ServerSpec {
            language_id: "typescript",
            command: "typescript-language-server",
            args: &["--stdio"],
            configuration_section: None,
        },
        ".tsx" => ServerSpec {
            language_id: "typescriptreact",
            command: "typescript-language-server",
            args: &["--stdio"],
            configuration_section: None,
        },
        ".js" | ".mjs" | ".cjs" => ServerSpec {
            language_id: "javascript",
            command: "typescript-language-server",
            args: &["--stdio"],
            configuration_section: None,
        },
        ".jsx" => ServerSpec {
            language_id: "javascriptreact",
            command: "typescript-language-server",
            args: &["--stdio"],
            configuration_section: None,
        },
        ".py" | ".pyi" => ServerSpec {
            language_id: "python",
            command: "pylsp",
            args: &[],
            configuration_section: None,
        },
        ".go" => ServerSpec {
            language_id: "go",
            command: "gopls",
            args: &["serve"],
            configuration_section: Some("gopls"),
        },
        ".rs" => ServerSpec {
            language_id: "rust",
            command: "rust-analyzer",
            args: &[],
            configuration_section: Some("rust-analyzer"),
        },
        ".java" => ServerSpec {
            language_id: "java",
            command: "jdtls",
            args: &[],
            configuration_section: None,
        },
        ".c" | ".h" => ServerSpec {
            language_id: "c",
            command: "clangd",
            args: &[],
            configuration_section: None,
        },
        ".cpp" | ".cc" | ".cxx" | ".hpp" | ".hh" | ".hxx" => ServerSpec {
            language_id: "cpp",
            command: "clangd",
            args: &[],
            configuration_section: None,
        },
        ".cu" | ".cuh" => ServerSpec {
            language_id: "cuda",
            command: "clangd",
            args: &[],
            configuration_section: None,
        },
        ".cs" => ServerSpec {
            language_id: "csharp",
            command: "csharp-ls",
            args: &[],
            configuration_section: None,
        },
        ".scala" | ".sc" | ".sbt" => ServerSpec {
            language_id: "scala",
            command: "metals",
            args: &[],
            configuration_section: None,
        },
        _ => return None,
    };
    Some(spec)
}

/// The `workspace/configuration` section the built-in server for `language_id`
/// takes its `initializationOptions` under: the one place a server's section
/// is declared, found by the language id every built-in spec carries.
pub(crate) fn configuration_section(language_id: Option<&str>) -> Option<&'static str> {
    let language_id = language_id?;
    crate::signatures::languages::supported_extensions()
        .into_iter()
        .filter_map(|extension| spec_for_extension(&format!(".{extension}")))
        .find(|spec| spec.language_id == language_id)
        .and_then(|spec| spec.configuration_section)
}

/// How to get each built-in server above that octocode does not install
/// itself, by bare command name.
const INSTALL_HINTS: &[(&str, &str)] = &[
    (
        "typescript-language-server",
        "octocode runs node_modules/typescript-language-server/lib/cli.mjs under node from its own install tree (its file mode, often 644 after a yarn install, is fine) and found none; install typescript-language-server there or put it on PATH",
    ),
    (
        "gopls",
        "install it with `go install golang.org/x/tools/gopls@latest` and put it on PATH",
    ),
    (
        "pylsp",
        "install it with `pip install python-lsp-server` (basedpyright or pyright on PATH is preferred) and put it on PATH",
    ),
    (
        "jdtls",
        "install the Eclipse JDT language server (for example `brew install jdtls`) and put `jdtls` on PATH",
    ),
    (
        "csharp-ls",
        "install it with `dotnet tool install --global csharp-ls` and put it on PATH",
    ),
    (
        "metals",
        "install it with `cs install metals` and put it on PATH",
    ),
];

/// How to install a built-in server that was not found: a managed server
/// (clangd, rust-analyzer) installs through octocode, any other built-in
/// through [`INSTALL_HINTS`]. `None` for a command with no built-in route.
pub(crate) fn install_hint(command: &str) -> Option<String> {
    if crate::lsp::managed::manifest().contains_key(command) {
        return Some(format!(
            "install it with `octocode lsp-server install {command}` or put it on PATH"
        ));
    }
    INSTALL_HINTS
        .iter()
        .find(|(name, _)| *name == command)
        .map(|(_, hint)| (*hint).to_owned())
}

fn user_server_for_extension(
    extension: &str,
    workspace_root: &str,
    options: &LspDiscoveryOptions,
) -> Option<LspLanguageServerConfig> {
    for (config_path, source) in user_config_paths(workspace_root, options) {
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
        let trust_workspace = options.trusts_project();
        // The entry is the user's choice wherever its program resolves.
        let (command, args, _) = resolve_server_invocation(
            &server.command,
            server.args.clone(),
            workspace_root,
            trust_workspace,
            options.invocation_dir.as_deref(),
        );
        return Some(LspLanguageServerConfig {
            command,
            args: Some(args),
            workspace_root: workspace_root.to_owned(),
            language_id: Some(server.language_id.clone()),
            initialization_options: server.initialization_options.clone(),
            env: server.env.clone(),
            max_memory_mb: server.max_memory_mb,
            source,
        });
    }
    None
}

/// The `lsp-servers.json` files consulted, in order, each with the source
/// its entries report.
fn user_config_paths(
    workspace_root: &str,
    options: &LspDiscoveryOptions,
) -> Vec<(PathBuf, ServerSource)> {
    let mut paths = Vec::new();
    if let Some(path) = options.config_path.as_ref() {
        paths.push((path.clone(), ServerSource::User));
    }
    if options.trusts_project() {
        paths.push((
            Path::new(workspace_root).join(".octocode/lsp-servers.json"),
            ServerSource::Project,
        ));
    }
    if let Some(home) = &options.octocode_home {
        paths.push((home.join("lsp-servers.json"), ServerSource::User));
    }
    paths
}
