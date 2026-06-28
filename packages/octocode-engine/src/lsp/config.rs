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
    grammar_for_file(&file_path)
        .map(|spec| spec.language_id.to_owned())
        .or_else(|| spec_for_file(&file_path).map(|spec| spec.language_id.to_owned()))
}

pub fn default_server_for_file(
    file_path: String,
    workspace_root: String,
) -> Option<JsLanguageServerConfig> {
    let extension = extension_key(&file_path)?;
    if let Some(config) = user_server_for_extension(&extension, &workspace_root) {
        return Some(config);
    }

    let spec = spec_for_extension(&extension)?;
    let (command, args) = resolve_spec_invocation(&spec, &workspace_root);

    Some(JsLanguageServerConfig {
        command,
        args: Some(args),
        workspace_root,
        language_id: Some(spec.language_id.to_owned()),
        initialization_options: None,
        env: None,
    })
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

/// Resolve a spec's command + args. JS/TS gets the Track-T backend ladder:
/// `OCTOCODE_TS_SERVER_PATH` → `tsgo` on PATH → `typescript-language-server`
/// (zero-config default). Every other language keeps its single spec.
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
        // 2) tsgo on PATH (opt-in, no flag) — preferred when present.
        if which::which("tsgo").is_ok() {
            return resolve_server_invocation("tsgo", tsgo_args(), workspace_root);
        }
        // 3) Fall through to the typescript-language-server default below.
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
        ".cpp" | ".cc" | ".cxx" | ".hpp" => ServerSpec {
            language_id: "cpp",
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
        ".json" | ".jsonc" => ServerSpec {
            language_id: "json",
            command: "vscode-json-language-server",
            args: &["--stdio"],
            env_var: Some("OCTOCODE_JSON_SERVER_PATH"),
        },
        ".yaml" | ".yml" => ServerSpec {
            language_id: "yaml",
            command: "yaml-language-server",
            args: &["--stdio"],
            env_var: Some("OCTOCODE_YAML_SERVER_PATH"),
        },
        ".html" | ".htm" => ServerSpec {
            language_id: "html",
            command: "vscode-html-language-server",
            args: &["--stdio"],
            env_var: Some("OCTOCODE_HTML_SERVER_PATH"),
        },
        ".css" => ServerSpec {
            language_id: "css",
            command: "vscode-css-language-server",
            args: &["--stdio"],
            env_var: Some("OCTOCODE_CSS_SERVER_PATH"),
        },
        ".scss" => ServerSpec {
            language_id: "scss",
            command: "vscode-css-language-server",
            args: &["--stdio"],
            env_var: Some("OCTOCODE_CSS_SERVER_PATH"),
        },
        ".less" => ServerSpec {
            language_id: "less",
            command: "vscode-css-language-server",
            args: &["--stdio"],
            env_var: Some("OCTOCODE_CSS_SERVER_PATH"),
        },
        ".php" => ServerSpec {
            language_id: "php",
            command: "intelephense",
            args: &["--stdio"],
            env_var: Some("OCTOCODE_PHP_SERVER_PATH"),
        },
        ".sql" => ServerSpec {
            language_id: "sql",
            command: "sqls",
            args: &[],
            env_var: Some("OCTOCODE_SQL_SERVER_PATH"),
        },
        ".swift" => ServerSpec {
            language_id: "swift",
            command: "sourcekit-lsp",
            args: &[],
            env_var: Some("OCTOCODE_SWIFT_SERVER_PATH"),
        },
        _ => return None,
    };
    Some(spec)
}

fn user_server_for_extension(
    extension: &str,
    workspace_root: &str,
) -> Option<JsLanguageServerConfig> {
    for config_path in user_config_paths(workspace_root) {
        let Ok(content) = std::fs::read_to_string(config_path) else {
            continue;
        };
        let Ok(parsed) = serde_json::from_str::<UserConfigFile>(&content) else {
            continue;
        };
        let Some(server) = parsed.language_servers.get(extension) else {
            continue;
        };
        if is_rejected_shell(&server.command) {
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
        });
    }
    None
}

fn user_config_paths(workspace_root: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(path) = std::env::var("OCTOCODE_LSP_CONFIG") {
        if !path.trim().is_empty() {
            paths.push(PathBuf::from(path));
        }
    }
    paths.push(Path::new(workspace_root).join(".octocode/lsp-servers.json"));
    if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        paths.push(PathBuf::from(home).join(".octocode/lsp-servers.json"));
    }
    paths
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
    ) {
        if let Some(node_command) = current_node_command() {
            let mut resolved_args = Vec::with_capacity(args.len() + 1);
            resolved_args.push(cli_path.to_string_lossy().into_owned());
            resolved_args.extend(args);
            return (node_command, resolved_args);
        }
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

fn current_node_command() -> Option<String> {
    std::env::current_exe()
        .ok()
        .filter(|path| is_executable_path(path))
        .map(|path| path.to_string_lossy().into_owned())
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
        command_is_tsgo, command_resolves_to_executable, current_node_command,
        default_server_for_file, detect_language_id, is_command_available,
        is_rust_analyzer_command, resolve_known_server_command, resolve_server_invocation,
        resolve_server_invocation_with_environment,
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
    fn detects_requested_language_matrix_from_native_grammar_registry() {
        let cases = [
            ("demo.ts", "typescript"),
            ("demo.tsx", "typescriptreact"),
            ("demo.js", "javascript"),
            ("demo.jsx", "javascript"),
            ("demo.py", "python"),
            ("demo.go", "go"),
            ("demo.rs", "rust"),
            ("demo.java", "java"),
            ("demo.c", "c"),
            ("demo.cpp", "cpp"),
            ("demo.cs", "csharp"),
            ("demo.sh", "shellscript"),
            ("demo.json", "json"),
            ("demo.yaml", "yaml"),
            ("demo.toml", "toml"),
            ("demo.html", "html"),
            ("demo.css", "css"),
            ("demo.scss", "scss"),
            ("demo.less", "less"),
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
    fn maps_scss_and_less_to_css_language_server_with_specific_language_ids() {
        let workspace_root = "/workspace".to_owned();
        let cases = [("demo.scss", "scss"), ("demo.less", "less")];

        for (file_name, expected_language_id) in cases {
            let Some(config) =
                default_server_for_file(file_name.to_owned(), workspace_root.clone())
            else {
                panic!("missing server config for {file_name}");
            };
            assert_eq!(config.command, "vscode-css-language-server");
            assert_eq!(config.language_id.as_deref(), Some(expected_language_id));
        }
    }

    #[test]
    fn detects_rust_analyzer_command_names() {
        assert!(is_rust_analyzer_command("rust-analyzer"));
        assert!(is_rust_analyzer_command("/usr/local/bin/rust-analyzer"));
        assert!(!is_rust_analyzer_command("typescript-language-server"));
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

    #[cfg(unix)]
    fn temp_test_root(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("{name}-{}-{nanos}", std::process::id()))
    }
}
