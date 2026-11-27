//! `octocode lsp-server status`: check the language-server configuration, every
//! route with its effective arguments, and with `--live` the servers themselves.
use octocode_engine::lsp::config::{
    LspDiscoveryOptions, ROUTED_EXTENSIONS, default_server_for_file, detect_language_id,
    is_command_available, server_command_resolves, user_config_reports,
};
use octocode_engine::lsp::managed::{manifest, resolve_cached_server};
use octocode_engine::lsp::types::{LspLanguageServerConfig, ServerSource};
use std::path::Path;

/// Resolve how `file_path`'s language server would launch, through the same
/// discovery `lspSearch` uses. Without a file, list the managed installs.
pub(super) fn run_status(
    discovery: &LspDiscoveryOptions,
    root: &Path,
    platform: &str,
    file_path: Option<&str>,
    json: bool,
) -> u8 {
    let Some(path) = file_path else {
        return run_overview(discovery, root, platform, json);
    };
    let named = path;
    let absolute = absolute_path(path);
    let path = absolute.as_str();

    let workspace = octocode_engine::lsp::config::workspace_root_for_file(path, discovery)
        .unwrap_or_else(|_| {
            std::env::current_dir()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|_| ".".to_owned())
        });
    let language_id = detect_language_id(path);
    let config = default_server_for_file(path, &workspace, discovery);
    let problems =
        octocode_engine::lsp::config::route_problems(path, config.as_ref(), &workspace, discovery);
    let effective = config.as_ref().map(effective_invocation);
    let (server_available, probe_error) = match config
        .as_ref()
        .map(|c| is_command_available(&c.command, &c.env.clone().unwrap_or_default()))
    {
        Some(Ok(available)) => (available, None),
        Some(Err(error)) => (false, Some(error)),
        None => (false, None),
    };
    let server_command: Option<String> = config.map(|c| c.command);
    let lang = language_id.as_deref().unwrap_or("unknown");

    // A path has a separator or an extension that contains no separator;
    // anything else is probably a server name passed by mistake.
    let looks_like_path = named.contains('/') || named.contains('\\') || {
        named
            .rfind('.')
            .is_some_and(|i| i + 1 < named.len() && !named[i + 1..].contains('/'))
    };
    let unresolved = !server_available
        && language_id
            .as_deref()
            .is_none_or(|l| l.is_empty() || l == "plaintext");

    if json {
        return super::super::write_json(&serde_json::json!({
            "filePath": path,
            "languageId": lang,
            "serverAvailable": server_available,
            "serverCommand": server_command,
            "probeError": probe_error,
            "workspaceRoot": workspace,
            "server": effective,
            "problems": problems,
        })) | u8::from(!problems.is_empty());
    }

    if unresolved && !looks_like_path {
        eprintln!(
            "  '{named}' looks like a server name, not a file. \
             status resolves a FILE's language \u{2192} server (e.g. src/main.rs); \
             run `lsp-server list` to see supported servers."
        );
    }

    println!("LSP status for {path}");
    println!("  language:  {lang}");
    if server_available {
        let cmd = server_command.as_deref().unwrap_or("unknown");
        println!("  resolved:  {cmd}");
    } else {
        println!("  resolved:  unavailable");
        if let Some(error) = &probe_error {
            println!("  Could not check the server command: {error}");
        }
        if let Some(lang_id) = &language_id {
            println!(
                "  No language server is available for this file (language: {lang_id}). \
                 Install a matching language server or add an lsp-servers.json entry."
            );
            if let Some(hint) = server_command
                .as_deref()
                .and_then(octocode_engine::lsp::config::install_hint)
            {
                println!("  {hint}");
            }
        } else {
            println!("  Could not determine language for this file.");
        }
    }
    println!("  workspace: {workspace}");
    if let Some(server) = &effective {
        println!(
            "  source:    {}",
            server["source"].as_str().unwrap_or_default()
        );
        println!("  args:      {}", server["args"]);
    }
    print_problems(&problems);
    u8::from(!problems.is_empty())
}

/// `path` from the working directory: a project root is found among a file's
/// ancestors, and a relative path has none past its own first component.
pub(super) fn absolute_path(path: &str) -> String {
    std::env::current_dir()
        .map(|dir| dir.join(path))
        .map_or_else(
            |_| path.to_owned(),
            |path| path.to_string_lossy().into_owned(),
        )
}

/// How a route starts: its program, every argument after the headless defaults
/// merge in (user values win), and what chose it. An argument named like a secret
/// is masked, and options are named, not shown: this reaches an agent's output.
fn effective_invocation(config: &LspLanguageServerConfig) -> serde_json::Value {
    serde_json::json!({
        "command": config.command,
        "args": masked_args(config.args.as_deref().unwrap_or_default()),
        "source": source_label(config.source),
        "languageId": config.language_id,
        "workspaceRoot": config.workspace_root,
        "initializationOptions": config
            .initialization_options
            .as_ref()
            .and_then(serde_json::Value::as_object)
            .map(|options| options.keys().cloned().collect::<Vec<_>>()),
    })
}

/// `args` with the value of a secret-named flag masked, in either spelling
/// (`--token=x`, `--token x`).
pub(super) fn masked_args(args: &[String]) -> Vec<String> {
    const SECRET_NAMES: [&str; 7] = [
        "token", "secret", "password", "passwd", "apikey", "api-key", "auth",
    ];
    let secret = |flag: &str| {
        let flag = flag.to_lowercase();
        flag.starts_with('-') && SECRET_NAMES.iter().any(|name| flag.contains(name))
    };
    let mut masked = Vec::with_capacity(args.len());
    let mut hide_next = false;
    for arg in args {
        if std::mem::take(&mut hide_next) {
            masked.push("***".to_owned());
        } else if let Some((flag, _)) = arg.split_once('=').filter(|(flag, _)| secret(flag)) {
            masked.push(format!("{flag}=***"));
        } else {
            hide_next = secret(arg);
            masked.push(arg.clone());
        }
    }
    masked
}

fn source_label(source: ServerSource) -> String {
    format!("{source:?}").to_lowercase()
}

fn print_problems(problems: &[String]) {
    if problems.is_empty() {
        println!("  problems:  none");
        return;
    }
    println!("  problems:");
    for problem in problems {
        println!("    - {problem}");
    }
}

/// The route of each extension in `root` (the catalog's, and every extension a
/// user entry adds), as `lspSearch` would resolve it. Resolution only: nothing
/// starts.
pub(super) fn routes(
    root: &str,
    discovery: &LspDiscoveryOptions,
) -> Vec<(String, Option<LspLanguageServerConfig>)> {
    let mut extensions = ROUTED_EXTENSIONS
        .iter()
        .map(|extension| (*extension).to_owned())
        .collect::<Vec<_>>();
    for report in user_config_reports(root, discovery) {
        extensions.extend(report.extensions);
    }
    extensions.sort();
    extensions.dedup();
    extensions
        .into_iter()
        .map(|extension| {
            let file = Path::new(root).join(format!("workspace{extension}"));
            let config = default_server_for_file(&file.to_string_lossy(), root, discovery);
            (extension, config)
        })
        .collect()
}

/// Everything `lsp-server status` can say without a file: managed installs,
/// each `lsp-servers.json` and what is wrong in it, and every route. Exit 1
/// when a configuration has a problem; a server that is simply not installed
/// is listed, not a failure.
fn run_overview(discovery: &LspDiscoveryOptions, root: &Path, platform: &str, json: bool) -> u8 {
    let installed: Vec<serde_json::Value> = manifest()
        .keys()
        .filter_map(|name| {
            resolve_cached_server(root, name, platform)
                .map(|bin| serde_json::json!({ "name": name, "path": bin }))
        })
        .collect();
    let workspace = std::env::current_dir()
        .map(|dir| dir.to_string_lossy().into_owned())
        .unwrap_or_else(|_| ".".to_owned());
    let reports = user_config_reports(&workspace, discovery);
    let problems = reports
        .iter()
        .flat_map(|report| {
            report
                .problems
                .iter()
                .map(move |problem| format!("{} {problem}", report.path))
        })
        .collect::<Vec<_>>();
    let routes = routes(&workspace, discovery);
    // Each route's own problems, once: a file's problem is every extension's.
    let mut problems = problems;
    for (extension, config) in &routes {
        let file = Path::new(&workspace).join(format!("workspace{extension}"));
        for problem in octocode_engine::lsp::config::route_problems(
            &file.to_string_lossy(),
            config.as_ref(),
            &workspace,
            discovery,
        ) {
            let repeats = problems.iter().any(|known| {
                known == &problem
                    || problem
                        .split_once(": ")
                        .is_some_and(|(path, rest)| known == &format!("{path} {rest}"))
            });
            if !repeats {
                problems.push(problem);
            }
        }
    }
    let failed = !problems.is_empty();
    if json {
        let rows = routes
            .iter()
            .map(|(extension, config)| {
                serde_json::json!({
                    "extension": extension,
                    "available": config.as_ref().is_some_and(server_command_resolves),
                    "server": config.as_ref().map(effective_invocation),
                })
            })
            .collect::<Vec<_>>();
        return super::super::write_json(&serde_json::json!({
            "managedRoot": root,
            "installed": installed,
            "configFiles": reports,
            "routes": rows,
            "problems": problems,
        })) | u8::from(failed);
    }
    println!("LSP status");
    println!("  managed installs: {}", root.display());
    for row in &installed {
        println!(
            "    {}  {}",
            row["name"].as_str().unwrap_or_default(),
            row["path"].as_str().unwrap_or_default()
        );
    }
    println!("  config files:");
    for report in &reports {
        println!("    {} ({}, {})", report.path, report.source, report.state);
    }
    println!("  routes:");
    for (extension, config) in &routes {
        match config {
            Some(config) if server_command_resolves(config) => println!(
                "    {extension:<7} {} [{}]",
                config.command,
                source_label(config.source)
            ),
            Some(config) => println!("    {extension:<7} {} (not installed)", config.command),
            None => println!("    {extension:<7} none"),
        }
    }
    print_problems(&problems);
    println!(
        "  Pass a file path to see how its language server resolves, or --live to start each server."
    );
    u8::from(failed)
}
