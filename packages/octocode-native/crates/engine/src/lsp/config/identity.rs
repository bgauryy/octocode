//! What decides how a launched language server behaves, for the pool key.
//!
//! A server keyed by its raw environment strings is replaced every time a
//! client whose `PATH`, `LANG` or `TMPDIR` spells differently asks for it:
//! a GUI host and a terminal reach the same executable and the same
//! toolchain through different strings. The key takes the launch's identity:
//!
//! - the resolved program: the file `command` runs under the call's `PATH`,
//!   through rustup's selection for a rustup proxy, as (device, inode);
//! - the resolved toolchain the server runs: for each tool the language's
//!   server shells out to (`rustc` and `cargo`, `python3`, `go`, `java`,
//!   `node`), the file the call's `PATH` resolves it to, through rustup's own
//!   selection (`RUSTUP_TOOLCHAIN`, directory overrides, toolchain files)
//!   for a proxy. This replaces `PATH` and the `RUSTUP_*` variables;
//! - the variables that select a toolchain or a store (`CARGO_HOME` with its
//!   default filled in, `VIRTUAL_ENV`, the `GO*` variables, `JAVA_HOME`,
//!   `DOTNET_*`) and anything the server's own entry configures, as given.
//!
//! Dropped, because they change no answer a server gives: `TMPDIR`, `TEMP`
//! and `TMP` (scratch location: a server writes there, and an answer never
//! depends on where), and `LANG` and `LC_ALL` for every server but the JVM
//! ones, whose default charset follows the locale. A language with no
//! profile here keeps its whole environment as given.
//!
//! The one server a key names keeps the environment of the call that started
//! it: a later call whose key is equal reuses that server and its environment.

use crate::lsp::commands::{has_path_separator, is_executable_path};
use crate::lsp::config::invocation::rustup::{RustupView, selected_component};
use crate::lsp::config::invocation::{is_rust_analyzer_command, resolve_known_server_command};
use crate::lsp::types::LspLanguageServerConfig;
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

/// What a language's server reads from the environment.
struct Profile {
    /// Tools the server runs, which the call's `PATH` resolves.
    tools: &'static [&'static str],
    /// Whether the server reads `CARGO_HOME`.
    cargo: bool,
    /// Whether the server's behavior follows `LANG` and `LC_ALL`.
    locale: bool,
}

fn profile(language_id: &str) -> Option<Profile> {
    use crate::lsp::family::{is_c_family, is_typescript_family};
    let id = Some(language_id);
    if is_typescript_family(id) {
        return Some(Profile {
            tools: &["node"],
            cargo: false,
            locale: false,
        });
    }
    if is_c_family(id) {
        return Some(Profile {
            tools: &["cc", "c++"],
            cargo: false,
            locale: false,
        });
    }
    let (tools, cargo, locale): (&'static [&'static str], bool, bool) = match language_id {
        "rust" => (&["rustc", "cargo"], true, false),
        "python" => (&["python3", "python"], false, false),
        "go" => (&["go"], false, false),
        "java" | "scala" => (&["java"], false, true),
        "csharp" => (&["dotnet"], false, false),
        "swift" => (&["swift"], false, false),
        _ => return None,
    };
    Some(Profile {
        tools,
        cargo,
        locale,
    })
}

/// The part of the pool key that stands for `config`'s launch: its program,
/// the toolchain its server runs and the environment that selects behavior.
pub(crate) fn launch_identity(config: &LspLanguageServerConfig) -> Value {
    let env = config.env.clone().unwrap_or_default();
    let workspace = Path::new(&config.workspace_root);
    let view = RustupView::of(&env);
    let program = resolve_program(config, &view, workspace)
        .map_or_else(|| Value::String(config.command.clone()), |p| file_id(&p));
    let Some(profile) = config.language_id.as_deref().and_then(profile) else {
        let env: std::collections::BTreeMap<_, _> = env.into_iter().collect();
        return serde_json::json!({ "program": program, "env": env });
    };
    let toolchain: Map<String, Value> = profile
        .tools
        .iter()
        .map(|tool| {
            let resolved =
                resolve_tool(&view, workspace, tool).map_or(Value::Null, |p| file_id(&p));
            ((*tool).to_owned(), resolved)
        })
        .collect();
    let mut kept: std::collections::BTreeMap<String, String> = env
        .iter()
        .filter(|(name, _)| keeps(name, &profile))
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect();
    if profile.cargo {
        let home = kept.get("CARGO_HOME").map(PathBuf::from).or_else(|| {
            crate::process_env::call_var(&env, "HOME")
                .map(PathBuf::from)
                .or_else(std::env::home_dir)
                .map(|home| home.join(".cargo"))
        });
        if let Some(home) = home {
            let home = std::fs::canonicalize(&home).unwrap_or(home);
            kept.insert("CARGO_HOME".to_owned(), home.to_string_lossy().into_owned());
        }
    }
    serde_json::json!({ "program": program, "toolchain": toolchain, "env": kept })
}

/// Whether the variable `name` stays in a known language's key as given.
fn keeps(name: &str, profile: &Profile) -> bool {
    match name {
        // Resolved above into the program and the toolchain.
        "PATH" | "RUSTUP_HOME" | "RUSTUP_TOOLCHAIN" => false,
        "CARGO_HOME" => profile.cargo,
        "LANG" | "LC_ALL" => profile.locale,
        "TMPDIR" | "TEMP" | "TMP" => false,
        _ => true,
    }
}

/// The file `config.command` runs: found on the call's `PATH`, and for the
/// rustup proxy of rust-analyzer the toolchain's own binary.
fn resolve_program(
    config: &LspLanguageServerConfig,
    view: &RustupView,
    workspace: &Path,
) -> Option<PathBuf> {
    let env = config.env.clone().unwrap_or_default();
    let command = resolve_known_server_command(&config.command, &env);
    let program = if Path::new(&command).is_absolute() || has_path_separator(&command) {
        PathBuf::from(&command)
    } else {
        which::which_in(&command, view.path_var.as_ref(), workspace).ok()?
    };
    if !is_executable_path(&program) {
        return None;
    }
    if (is_rust_analyzer_command(&command) || is_rust_analyzer_command(&config.command))
        && let Ok(Some(component)) = selected_component(view, &program, workspace)
    {
        return Some(component);
    }
    Some(program)
}

/// The file `tool` resolves to on the call's `PATH`: through rustup's
/// selection when it is a rustup proxy.
fn resolve_tool(view: &RustupView, workspace: &Path, tool: &str) -> Option<PathBuf> {
    let found = which::which_in(tool, view.path_var.as_ref(), workspace).ok()?;
    match selected_component(view, &found, workspace) {
        Ok(Some(component)) => Some(component),
        Ok(None) | Err(_) => Some(found),
    }
}

/// A file's identity: (device, inode) where the platform has them, else its
/// real path. A symlink and its target are one file.
fn file_id(path: &Path) -> Value {
    #[cfg(unix)]
    if let Ok(meta) = std::fs::metadata(path) {
        use std::os::unix::fs::MetadataExt;
        return Value::String(format!("{}:{}", meta.dev(), meta.ino()));
    }
    let real = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    Value::String(real.to_string_lossy().into_owned())
}

#[cfg(test)]
#[cfg(unix)]
pub(crate) mod fixture;
#[cfg(test)]
#[cfg(unix)]
mod tests;
