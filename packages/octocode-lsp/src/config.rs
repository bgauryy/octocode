use crate::grammar::grammar_for_file;
use crate::types::JsLanguageServerConfig;
use napi::{Error, Result, Status};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

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
    let command = spec
        .env_var
        .and_then(|key| std::env::var(key).ok())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| spec.command.to_owned());

    Some(JsLanguageServerConfig {
        command,
        args: Some(spec.args.iter().map(|arg| (*arg).to_owned()).collect()),
        workspace_root,
        language_id: Some(spec.language_id.to_owned()),
        initialization_options: None,
    })
}

pub fn is_command_available(command: String) -> Result<bool> {
    if is_rejected_shell(&command) {
        return Ok(false);
    }
    if command
        == std::env::current_exe()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default()
    {
        return Ok(true);
    }
    if Path::new(&command).is_absolute() {
        return Ok(Path::new(&command).exists());
    }
    which::which(&command)
        .map(|_| true)
        .or_else(|err| match err {
            which::Error::CannotFindBinaryPath => Ok(false),
            other => Err(Error::new(Status::GenericFailure, other.to_string())),
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
        ".sh" | ".bash" | ".zsh" => ServerSpec {
            language_id: "shellscript",
            command: "bash-language-server",
            args: &["start"],
            env_var: Some("OCTOCODE_BASH_SERVER_PATH"),
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
        ".toml" => ServerSpec {
            language_id: "toml",
            command: "taplo",
            args: &["lsp", "stdio"],
            env_var: Some("OCTOCODE_TOML_SERVER_PATH"),
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
        return Some(JsLanguageServerConfig {
            command: server.command.clone(),
            args: Some(server.args.clone()),
            workspace_root: workspace_root.to_owned(),
            language_id: Some(server.language_id.clone()),
            initialization_options: server.initialization_options.clone(),
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

fn is_rejected_shell(command: &str) -> bool {
    let name = Path::new(command)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_ascii_lowercase();
    matches!(
        name.as_str(),
        "sh" | "bash"
            | "zsh"
            | "fish"
            | "cmd"
            | "cmd.exe"
            | "powershell"
            | "powershell.exe"
            | "pwsh"
            | "pwsh.exe"
    )
}

#[cfg(test)]
mod tests {
    use super::{default_server_for_file, detect_language_id};

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
}
