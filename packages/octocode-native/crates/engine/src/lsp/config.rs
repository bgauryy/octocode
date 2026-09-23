use crate::lsp::commands::{command_resolves_to_executable, is_executable_path, is_rejected_shell};
use crate::lsp::grammar::grammar_for_file;
use crate::lsp::types::JsLanguageServerConfig;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
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
    if let Some(spec) = spec.filter(spec_has_env_override) {
        return Some(config_from_spec(spec, workspace_root));
    }

    if let Some(config) = user_server_for_extension(&extension, &workspace_root, options) {
        return Some(config);
    }

    spec.map(|spec| config_from_spec(spec, workspace_root))
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
/// Bounded breadth-first walk that skips hidden, vendored, and build dirs.
pub fn workspace_root_representative_source(workspace_root: &str) -> Option<String> {
    const MAX_ENTRIES: usize = 2_000;
    const MAX_DEPTH: usize = 6;
    const SKIPPED_DIRS: &[&str] = &["node_modules", "target", "dist", "build", "out", "vendor"];
    let root = Path::new(workspace_root);
    let extension = WORKSPACE_ROOT_MARKERS
        .iter()
        .find(|(marker, _)| root.join(marker).is_file())
        .map(|(_, extension)| *extension)?;
    let family: &[&str] = match extension {
        ".ts" | ".js" => &["ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs"],
        ".rs" => &["rs"],
        ".go" => &["go"],
        ".py" => &["py"],
        _ => return None,
    };
    let mut queue = std::collections::VecDeque::from([(root.to_path_buf(), 0usize)]);
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
            } else if file_type.is_file()
                && !name.ends_with(".d.ts")
                && Path::new(&name)
                    .extension()
                    .is_some_and(|ext| family.contains(&ext.to_string_lossy().as_ref()))
            {
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

fn config_from_spec(spec: ServerSpec, workspace_root: String) -> JsLanguageServerConfig {
    let (command, args) = resolve_spec_invocation(&spec, &workspace_root);
    JsLanguageServerConfig {
        command,
        args: Some(args),
        workspace_root,
        language_id: Some(spec.language_id.to_owned()),
        initialization_options: None,
        env: None,
        max_memory_mb: None,
    }
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
fn resolve_spec_invocation(spec: &ServerSpec, workspace_root: &str) -> (String, Vec<String>) {
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
            return resolve_server_invocation(&command, args, workspace_root);
        }
        // Automatic tsgo preference is intentionally disabled until the
        // held-out parity matrix covers all public LSP operations.
    }

    let command = env_override.unwrap_or_else(|| spec.command.to_owned());
    resolve_server_invocation(
        &command,
        spec.args.iter().map(|arg| (*arg).to_owned()).collect(),
        workspace_root,
    )
}

pub fn is_command_available(command: String) -> Result<bool, String> {
    let command = resolve_known_server_command(&command);
    if is_rejected_shell(&command) {
        return Ok(false);
    }
    if typescript_cli_from_command(&command).is_some() {
        return Ok(current_node_command().is_some());
    }
    if is_rust_analyzer_command(&command) {
        return Ok(Command::new(&command)
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false));
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
        let (command, args) =
            resolve_server_invocation(&server.command, server.args.clone(), workspace_root);
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

fn is_rust_analyzer_command(command: &str) -> bool {
    Path::new(command)
        .file_name()
        .map(|name| name == "rust-analyzer")
        .unwrap_or(false)
}

fn resolve_server_invocation(
    command: &str,
    args: Vec<String>,
    workspace_root: &str,
) -> (String, Vec<String>) {
    let current_dir = std::env::current_dir().ok();
    resolve_server_invocation_with_environment(
        command,
        args,
        workspace_root,
        current_dir.as_deref(),
        command_resolves_to_executable(command),
    )
}

fn resolve_server_invocation_with_environment(
    command: &str,
    args: Vec<String>,
    workspace_root: &str,
    current_dir: Option<&Path>,
    command_available_on_path: bool,
) -> (String, Vec<String>) {
    if let Some(cli_path) = resolve_typescript_server_cli(
        command,
        workspace_root,
        current_dir,
        command_available_on_path,
    ) && let Some(node_command) = current_node_command()
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

fn resolve_typescript_server_cli(
    command: &str,
    workspace_root: &str,
    current_dir: Option<&Path>,
    command_available_on_path: bool,
) -> Option<PathBuf> {
    if !is_typescript_server_command(command) {
        return None;
    }
    if command_available_on_path || is_executable_path(Path::new(command)) {
        return None;
    }
    typescript_cli_from_command(command).or_else(|| {
        find_node_module_file(
            workspace_root,
            current_dir,
            "typescript-language-server/lib/cli.mjs",
        )
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

fn find_node_module_file(
    workspace_root: &str,
    current_dir: Option<&Path>,
    package_relative_path: &str,
) -> Option<PathBuf> {
    let start = Path::new(workspace_root);
    if let Some(path) = find_node_module_file_from(start, package_relative_path) {
        return Some(path);
    }
    current_dir.and_then(|cwd| find_node_module_file_from(cwd, package_relative_path))
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
    fn builtin_routes_cover_the_exact_first_class_extension_set() {
        // Intersection of the native grammar registry with the built-in server
        // routes. CUDA (`cu`/`cuh`) re-joined the default registry in 19.1.3,
        // so it is now part of this intersection again.
        let expected = [
            "c", "cc", "cjs", "cpp", "cs", "cts", "cu", "cuh", "cxx", "go", "h", "hh", "hpp",
            "hxx", "java", "js", "jsx", "mjs", "mts", "py", "pyi", "rs", "sbt", "sc", "scala",
            "ts", "tsx",
        ];
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
            resolve_server_invocation(cli_str, vec!["--stdio".to_owned()], root_str);
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
            Some(&package_root),
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
