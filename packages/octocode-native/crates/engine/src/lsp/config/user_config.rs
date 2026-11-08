//! `lsp-servers.json`: the user's own server entries, consulted before the
//! built-in routes (see [`super::catalog`]).

use super::catalog::spec_for_extension;
use super::invocation::{is_interpreter_eval_launch, resolve_server_invocation};
use super::types::LspDiscoveryOptions;
use crate::lsp::commands::is_rejected_shell;
use crate::lsp::types::{LspLanguageServerConfig, ServerSource};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

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

impl UserServerSpec {
    /// A launch no entry may use: a shell, or an interpreter told to evaluate
    /// its arguments.
    fn is_rejected_launch(&self) -> bool {
        is_rejected_shell(&self.command) || is_interpreter_eval_launch(&self.command, &self.args)
    }
}

/// The fields of one entry; any other is reported, never fatal.
const ENTRY_FIELDS: [&str; 6] = [
    "command",
    "args",
    "languageId",
    "initializationOptions",
    "maxMemoryMb",
    "env",
];

/// One `lsp-servers.json`: the entries that can route, and what is wrong with
/// the rest, by extension (`None`: the whole file). A wrong entry costs only
/// itself. Messages name fields, never values: an `env` entry may hold a secret.
struct ParsedConfig {
    servers: HashMap<String, UserServerSpec>,
    problems: Vec<(Option<String>, String)>,
}

fn parse_user_config(content: &str) -> ParsedConfig {
    let mut parsed = ParsedConfig {
        servers: HashMap::new(),
        problems: Vec::new(),
    };
    let root = match serde_json::from_str::<Value>(content) {
        Ok(root) => root,
        Err(error) => {
            parsed.problems.push((
                None,
                format!(
                    "is not valid JSON (line {}, column {}), so none of its servers apply",
                    error.line(),
                    error.column()
                ),
            ));
            return parsed;
        }
    };
    let Some(entries) = root.get("languageServers").and_then(Value::as_object) else {
        parsed.problems.push((
            None,
            "has no `languageServers` object, so none of its servers apply".to_owned(),
        ));
        return parsed;
    };
    for (extension, entry) in entries {
        let mut problem = |message: String| {
            parsed.problems.push((Some(extension.clone()), message));
        };
        if !extension.starts_with('.') {
            problem(
                "is no file extension: a key starts with `.` (`.c`), so it routes nothing"
                    .to_owned(),
            );
            continue;
        }
        if let Some(fields) = entry.as_object() {
            for unknown in fields
                .keys()
                .filter(|field| !ENTRY_FIELDS.contains(&field.as_str()))
            {
                problem(format!(
                    "has unknown field `{unknown}`, which is ignored (fields: {})",
                    ENTRY_FIELDS.join(", ")
                ));
            }
        }
        let Ok(server) = serde_json::from_value::<UserServerSpec>(entry.clone()) else {
            problem(
                "is ignored: `command` and `languageId` are required strings, `args` a string array, `maxMemoryMb` a number, `env` an object of strings"
                    .to_owned(),
            );
            continue;
        };
        if server.is_rejected_launch() {
            problem(
                "is ignored: its `command` is a shell or an interpreter told to evaluate its arguments"
                    .to_owned(),
            );
            continue;
        }
        parsed.servers.insert(extension.clone(), server);
    }
    parsed
}

/// What one consulted `lsp-servers.json` holds, for `lsp-server status`.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserConfigReport {
    pub path: String,
    /// `user` or `project`.
    pub source: &'static str,
    /// `missing`, `ignored` (a project file `lsp.trustProjectConfig` does not
    /// trust), `unreadable` or `read`.
    pub state: &'static str,
    /// Extensions it routes.
    pub extensions: Vec<String>,
    pub problems: Vec<String>,
}

/// One report per `lsp-servers.json` a lookup in `workspace_root` could read,
/// a project file nothing trusts included.
pub fn user_config_reports(
    workspace_root: &str,
    options: &LspDiscoveryOptions,
) -> Vec<UserConfigReport> {
    let trusted = user_config_paths(workspace_root, options);
    let project = Path::new(workspace_root).join(PROJECT_CONFIG);
    let ignored = (!options.trusts_project()).then_some((project, ServerSource::Project));
    trusted
        .into_iter()
        .map(|entry| (entry, true))
        .chain(ignored.map(|entry| (entry, false)))
        .map(|((path, source), consulted)| {
            let mut report = UserConfigReport {
                path: path.to_string_lossy().into_owned(),
                source: if matches!(source, ServerSource::Project) {
                    "project"
                } else {
                    "user"
                },
                state: "missing",
                extensions: Vec::new(),
                problems: Vec::new(),
            };
            if !path.exists() {
                return report;
            }
            if !consulted {
                report.state = "ignored";
                report.problems.push(
                    "is ignored: a project's servers apply only with lsp.trustProjectConfig"
                        .to_owned(),
                );
                return report;
            }
            let Ok(content) = std::fs::read_to_string(&path) else {
                report.state = "unreadable";
                report
                    .problems
                    .push("cannot be read, so none of its servers apply".to_owned());
                return report;
            };
            let parsed = parse_user_config(&content);
            report.state = "read";
            report.extensions = parsed.servers.keys().cloned().collect();
            report.extensions.sort();
            report.problems = parsed
                .problems
                .into_iter()
                .map(|(extension, message)| match extension {
                    Some(extension) => format!("`{extension}` {message}"),
                    None => message,
                })
                .collect();
            report
        })
        .collect()
}

/// What is wrong in a consulted `lsp-servers.json` that changes how `extension`
/// routes: the whole file, or an entry for it or a language-family sibling.
pub fn user_config_problems_for(
    extension: &str,
    workspace_root: &str,
    options: &LspDiscoveryOptions,
) -> Vec<String> {
    let mut problems = Vec::new();
    for (path, _) in user_config_paths(workspace_root, options) {
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let name = path.to_string_lossy();
        for (entry, message) in parse_user_config(&content).problems {
            let applies = entry.as_deref().is_none_or(|entry| {
                entry == extension || family_siblings(extension).contains(&entry)
            });
            if applies {
                problems.push(match entry {
                    Some(entry) => format!("{name}: `{entry}` {message}"),
                    None => format!("{name} {message}"),
                });
            }
        }
    }
    problems
}

/// What is wrong with the route of `path`: the `lsp-servers.json` problems that
/// change it, a database it names that holds nothing, and a user's entry whose
/// program does not resolve (a built-in that is not installed is no
/// misconfiguration).
pub fn route_problems(
    path: &str,
    config: Option<&LspLanguageServerConfig>,
    workspace_root: &str,
    options: &LspDiscoveryOptions,
) -> Vec<String> {
    if options.opens_fetched_code(path, workspace_root) {
        return vec![super::types::FETCHED_CODE_REFUSAL.to_owned()];
    }
    let extension = Path::new(path)
        .extension()
        .map(|extension| format!(".{}", extension.to_string_lossy()))
        .unwrap_or_default();
    let mut problems = user_config_problems_for(&extension, workspace_root, options);
    let Some(config) = config else {
        return problems;
    };
    if crate::lsp::family::is_c_family(config.language_id.as_deref()) {
        problems.extend(crate::lsp::clangd_database::configured_database_problem(
            config,
        ));
    }
    if matches!(config.source, ServerSource::User | ServerSource::Project)
        && !super::probe::server_command_resolves(config)
    {
        problems.push(format!(
            "`{}` from lsp-servers.json does not resolve to an executable",
            config.command
        ));
    }
    problems
}

/// One entry's launch, and the command the entry spells (what resolution
/// was asked for, before it chose a program).
pub(super) struct UserServer {
    pub(super) config: LspLanguageServerConfig,
    pub(super) command: String,
}

pub(super) fn user_server_for_extension(
    extension: &str,
    workspace_root: &str,
    options: &LspDiscoveryOptions,
) -> Option<UserServer> {
    for (config_path, source) in user_config_paths(workspace_root, options) {
        let Ok(content) = std::fs::read_to_string(config_path) else {
            continue;
        };
        let parsed = parse_user_config(&content);
        // An entry for one extension of a language family routes the whole
        // family (`.ts` also `.mts`, `.cts`, `.tsx`): one project, one
        // server, not two project models split by spelling. The entry's own
        // extension keeps its language id; a sibling takes the built-in one.
        let (server, language_id) = match parsed.servers.get(extension) {
            Some(server) => (server, server.language_id.clone()),
            None => {
                let Some((server, spec)) = family_siblings(extension).iter().find_map(|sibling| {
                    let server = parsed.servers.get(*sibling)?;
                    Some((server, spec_for_extension(extension)?))
                }) else {
                    continue;
                };
                (server, spec.language_id.to_owned())
            }
        };
        if server.is_rejected_launch() {
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
        return Some(UserServer {
            config: LspLanguageServerConfig {
                command,
                args: Some(args),
                workspace_root: workspace_root.to_owned(),
                language_id: Some(language_id),
                initialization_options: server.initialization_options.clone(),
                env: server.env.clone(),
                max_memory_mb: server.max_memory_mb,
                source,
            },
            command: server.command.clone(),
        });
    }
    None
}

/// The extensions one `lsp-servers.json` entry also routes, in the order
/// they are tried: the other spellings of a language, whose files (headers and
/// sources, stubs and modules) are one project to one server.
fn family_siblings(extension: &str) -> &'static [&'static str] {
    match extension {
        ".ts" | ".mts" | ".cts" | ".tsx" => &[".ts", ".tsx", ".mts", ".cts"],
        ".js" | ".mjs" | ".cjs" | ".jsx" => &[".js", ".jsx", ".mjs", ".cjs"],
        ".c" | ".h" | ".cpp" | ".cc" | ".cxx" | ".hpp" | ".hh" | ".hxx" | ".cu" | ".cuh" => &[
            ".c", ".cpp", ".cc", ".cxx", ".h", ".hpp", ".hh", ".hxx", ".cu", ".cuh",
        ],
        ".py" | ".pyi" => &[".py", ".pyi"],
        ".scala" | ".sc" | ".sbt" => &[".scala", ".sc", ".sbt"],
        _ => &[],
    }
}

/// A project's own `lsp-servers.json`, relative to its root.
const PROJECT_CONFIG: &str = ".octocode/lsp-servers.json";

/// The `lsp-servers.json` files consulted, in order, each with the source
/// its entries report.
pub(super) fn user_config_paths(
    workspace_root: &str,
    options: &LspDiscoveryOptions,
) -> Vec<(PathBuf, ServerSource)> {
    let mut paths = Vec::new();
    if options.trusts_project() {
        paths.push((
            Path::new(workspace_root).join(PROJECT_CONFIG),
            ServerSource::Project,
        ));
    }
    if let Some(home) = &options.octocode_home {
        paths.push((home.join("lsp-servers.json"), ServerSource::User));
    }
    paths
}

#[cfg(test)]
mod tests;
