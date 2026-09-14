//! Human command families from the native CLI RFC.
use super::execute;
use clap::Args;
use octocode_engine_tools_core::runtime::ToolRuntime;
use serde_json::{Value, json};
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Args, Debug)]
pub struct FilesArgs {
    pub path: String,
    #[arg(long)]
    pub names: Option<String>,
}

#[derive(Args, Debug)]
pub struct TreeArgs {
    pub path: String,
    #[arg(long)]
    pub syntax: bool,
}

#[derive(Args, Debug)]
pub struct SymbolsArgs {
    pub path: String,
    #[arg(long)]
    pub name: Option<String>,
}

#[derive(Args, Debug)]
pub struct AstArgs {
    pub path: String,
    pub pattern: String,
}

#[derive(Args, Debug)]
pub struct GraphArgs {
    pub path: String,
    pub analysis: String,
    #[arg(long)]
    pub file: Option<String>,
    #[arg(long)]
    pub target: Option<String>,
}

#[derive(Args, Debug)]
pub struct RewriteArgs {
    pub path: String,
    pub pattern: String,
    #[arg(long = "to")]
    pub replacement: String,
    #[arg(long)]
    pub lang: Option<String>,
    #[arg(long)]
    pub apply: bool,
}

#[derive(Args, Debug)]
pub struct LspArgs {
    pub uri: String,
    #[arg(long)]
    pub line: Option<u32>,
    #[arg(long)]
    pub character: Option<u32>,
    #[arg(long)]
    pub symbol: Option<String>,
    #[arg(long)]
    pub operation: Option<String>,
}

#[derive(Args, Debug)]
pub struct ReposArgs {
    pub query: String,
    #[arg(long)]
    pub owner: Option<String>,
}

#[derive(Args, Debug)]
pub struct CloneArgs {
    pub repo: String,
    #[arg(long)]
    pub branch: Option<String>,
    #[arg(long)]
    pub sparse_path: Option<String>,
}

#[derive(Args, Debug)]
pub struct PackageArgs {
    pub query: String,
    #[arg(long, default_value = "npm")]
    pub ecosystem: String,
    #[arg(long)]
    pub info: bool,
}

#[derive(Args, Debug)]
pub struct HistoryArgs {
    pub operation: String,
    #[arg(long)]
    pub repo: String,
    #[arg(long)]
    pub keywords: Option<String>,
}

pub async fn files(runtime: &ToolRuntime, args: FilesArgs) -> u8 {
    let mut query = json!({"operation":"files","path":args.path});
    if let Some(names) = args.names {
        query["names"] = json!(names.split(',').collect::<Vec<_>>());
    }
    run(runtime, "astSearch", query).await
}

pub async fn tree(runtime: &ToolRuntime, args: TreeArgs) -> u8 {
    run(
        runtime,
        "astSearch",
        json!({
            "operation":"tree",
            "path":args.path,
            "treeKind": if args.syntax { "syntax" } else { "filesystem" }
        }),
    )
    .await
}

pub async fn symbols(runtime: &ToolRuntime, args: SymbolsArgs) -> u8 {
    let mut query = json!({"operation":"symbols","path":args.path});
    if let Some(name) = args.name {
        query["name"] = json!(name);
    }
    run(runtime, "astSearch", query).await
}

pub async fn ast(runtime: &ToolRuntime, args: AstArgs) -> u8 {
    run(
        runtime,
        "astSearch",
        json!({"operation":"match","path":args.path,"pattern":args.pattern}),
    )
    .await
}

pub async fn graph(runtime: &ToolRuntime, args: GraphArgs) -> u8 {
    let mut query = json!({
        "operation":"topology",
        "path":args.path,
        "analysis":args.analysis
    });
    if let Some(file) = args.file {
        query["file"] = json!(file);
    }
    if let Some(target) = args.target {
        query["target"] = json!(target);
    }
    run(runtime, "astSearch", query).await
}

pub async fn rewrite(runtime: &ToolRuntime, args: RewriteArgs) -> u8 {
    let lang = args
        .lang
        .or_else(|| lang_from_path(&args.path).map(|value| value.to_owned()));
    let Some(lang) = lang else {
        eprintln!("rewrite requires --lang for this path");
        return 2;
    };
    run(
        runtime,
        "astRewrite",
        json!({
            "path": args.path,
            "pattern": args.pattern,
            "rewrite": args.replacement,
            "ruleKind": "pattern",
            "langType": lang,
            "apply": args.apply
        }),
    )
    .await
}

pub async fn lsp(runtime: &ToolRuntime, operation: &str, args: LspArgs) -> u8 {
    let mut query = json!({
        "operation": args.operation.as_deref().unwrap_or(operation),
        "uri": args.uri
    });
    if let Some(symbol) = args.symbol {
        query["symbolName"] = json!(symbol);
        if let Some(line) = args.line {
            query["lineHint"] = json!(line);
        }
    } else if let Some(line) = args.line {
        query["position"] = json!({
            "line": line.saturating_sub(1),
            "character": args.character.unwrap_or(0)
        });
    }
    run(runtime, "lspSearch", query).await
}

pub async fn repos(runtime: &ToolRuntime, args: ReposArgs) -> u8 {
    let mut query = json!({"operation":"repositories","keywords":[args.query]});
    if let Some(owner) = args.owner {
        query["owner"] = json!(owner);
    }
    run(runtime, "ghSearch", query).await
}

pub async fn clone_repo(runtime: &ToolRuntime, args: CloneArgs) -> u8 {
    let Some((owner, repo)) = args.repo.split_once('/') else {
        eprintln!("Usage: octocode clone OWNER/REPO");
        return 2;
    };
    let mut query = json!({"owner":owner,"repo":repo});
    if let Some(branch) = args.branch {
        query["branch"] = json!(branch);
    }
    if let Some(path) = args.sparse_path {
        query["sparsePath"] = json!(path);
    }
    run(runtime, "ghCloneRepo", query).await
}

pub async fn package(runtime: &ToolRuntime, args: PackageArgs) -> u8 {
    let mut query = json!({"type":args.ecosystem});
    if args.info {
        query["packageName"] = json!(args.query);
    } else {
        query["keywords"] = json!([args.query]);
    }
    run(runtime, "artifactSearch", query).await
}

pub async fn history(runtime: &ToolRuntime, args: HistoryArgs) -> u8 {
    let Some((owner, repo)) = args.repo.split_once('/') else {
        eprintln!("Usage: octocode history <prs|issues|commits|pr|issue|commit> --repo OWNER/REPO");
        return 2;
    };
    let (tool, query) = match args.operation.as_str() {
        "prs" | "issues" | "commits" => {
            let operation = match args.operation.as_str() {
                "prs" => "pullRequests",
                other => other,
            };
            let mut query = json!({"operation":operation,"owner":owner,"repo":repo});
            if let Some(keywords) = args.keywords {
                query["keywords"] = json!(keywords.split_whitespace().collect::<Vec<_>>());
            }
            ("ghSearchHistory", query)
        }
        "pr" | "issue" | "commit" => {
            let operation = match args.operation.as_str() {
                "pr" => "pullRequest",
                "issue" => "issue",
                _ => "commit",
            };
            (
                "ghGetHistoryItem",
                json!({"operation":operation,"owner":owner,"repo":repo}),
            )
        }
        other => {
            eprintln!("Unknown history operation: {other}");
            return 2;
        }
    };
    run(runtime, tool, query).await
}

pub async fn context(runtime: &ToolRuntime, json_out: bool, full: bool) -> u8 {
    match runtime.catalog() {
        Ok(catalog) => {
            if json_out {
                return super::write_json(&catalog, true);
            }
            if full {
                if let Some(text) = catalog["mcpInstructions"].as_str() {
                    println!("{text}");
                }
            }
            if let Some(tools) = catalog["tools"].as_array() {
                for tool in tools {
                    let name = tool["name"].as_str().unwrap_or_default();
                    let available = tool["available"].as_bool().unwrap_or(false);
                    let mark = if available { "on" } else { "off" };
                    println!("{name} {mark}");
                }
            }
            0
        }
        Err(error) => {
            eprintln!("{}", error.message);
            5
        }
    }
}

pub async fn status(runtime: &ToolRuntime, json_out: bool) -> u8 {
    let view = runtime.inspect_config();
    let catalog = runtime.catalog().ok();
    let available = catalog
        .as_ref()
        .and_then(|value| value["tools"].as_array())
        .map(|tools| {
            tools
                .iter()
                .filter(|tool| tool["available"] == true)
                .filter_map(|tool| tool["name"].as_str())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let token = ["GITHUB_TOKEN", "GH_TOKEN", "OCTOCODE_TOKEN"]
        .iter()
        .any(|key| {
            runtime
                .config()
                .env_value(key)
                .is_some_and(|value| !value.is_empty())
        });
    if json_out {
        return super::write_json(
            &json!({
                "home": view.home,
                "storage": view.storage_mode,
                "auth": if token { "set" } else { "unset" },
                "availableTools": available
            }),
            true,
        );
    }
    println!(
        "home: {}\nstorage: {}\nauth: {}\ntools: {}",
        view.home.display(),
        view.storage_mode,
        if token { "set" } else { "unset" },
        available.join(" ")
    );
    0
}

pub fn auth_status(runtime: &ToolRuntime, json_out: bool) -> u8 {
    let token = ["GITHUB_TOKEN", "GH_TOKEN", "OCTOCODE_TOKEN"]
        .iter()
        .any(|key| {
            runtime
                .config()
                .env_value(key)
                .is_some_and(|value| !value.is_empty())
        });
    if json_out {
        return super::write_json(&json!({"authenticated": token}), true);
    }
    println!(
        "{}",
        if token {
            "authenticated"
        } else {
            "unauthenticated"
        }
    );
    if token { 0 } else { 1 }
}

pub fn login() -> u8 {
    eprintln!("Set GITHUB_TOKEN or GH_TOKEN, or run `gh auth login` and retry.");
    1
}

pub fn logout(runtime: &ToolRuntime) -> u8 {
    let view = runtime.inspect_config();
    let host = runtime
        .config()
        .resolved
        .github
        .api_url
        .parse::<url::Url>()
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .unwrap_or_else(|| "github.com".into());
    let host = if host == "api.github.com" {
        "github.com".into()
    } else {
        host
    };
    match octocode_engine_tools_core::providers::github::delete_platform_credential(&host) {
        Ok(()) => {
            eprintln!(
                "Removed native keychain credentials for {host}. Environment tokens are unchanged. Stored files under {} were not printed.",
                view.home.display()
            );
            0
        }
        Err(error) => {
            eprintln!("{}", error.message);
            1
        }
    }
}

pub fn cache(runtime: &ToolRuntime, action: &str) -> u8 {
    match action {
        "status" => {
            let view = runtime.inspect_config();
            println!("cache home: {}", view.home.join("tmp").display());
            0
        }
        "clear" => {
            runtime.clear_github_cache();
            println!("cleared GitHub content cache");
            0
        }
        other => {
            eprintln!("Usage: octocode cache <status|clear>");
            let _ = other;
            2
        }
    }
}

pub fn skill(args: &[String]) -> u8 {
    match resolve_node_octocode() {
        None => {
            eprintln!("{}", missing_octocode_message(args));
            1
        }
        Some(program) => match Command::new(program).arg("skill").args(args).status() {
            Ok(status) => status
                .code()
                .and_then(|code| u8::try_from(code).ok())
                .unwrap_or(1),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                eprintln!("{}", missing_octocode_message(args));
                1
            }
            Err(error) => {
                eprintln!("failed to spawn octocode skill: {error}");
                1
            }
        },
    }
}

fn resolve_node_octocode() -> Option<PathBuf> {
    let self_exe = std::env::current_exe().ok();
    let directories = std::env::var_os("PATH")
        .map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_default();
    resolve_node_octocode_from(directories, self_exe.as_deref())
}

fn resolve_node_octocode_from(
    directories: impl IntoIterator<Item = PathBuf>,
    self_exe: Option<&Path>,
) -> Option<PathBuf> {
    for directory in directories {
        for name in octocode_path_names() {
            let candidate = directory.join(name);
            if !is_runnable(&candidate) {
                continue;
            }
            if self_exe.is_some_and(|exe| same_executable(&candidate, exe)) {
                continue;
            }
            return Some(candidate);
        }
    }
    None
}

fn octocode_path_names() -> Vec<OsString> {
    let mut names = Vec::new();
    if cfg!(windows) {
        let pathext =
            std::env::var_os("PATHEXT").unwrap_or_else(|| OsString::from(".COM;.EXE;.BAT;.CMD"));
        for ext in pathext.to_string_lossy().split(';') {
            let ext = ext.trim();
            if ext.is_empty() {
                continue;
            }
            let mut name = OsString::from("octocode");
            if !ext.starts_with('.') {
                name.push(".");
            }
            name.push(ext);
            names.push(name);
        }
    }
    names.push(OsString::from("octocode"));
    names
}

fn is_runnable(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn same_executable(left: &Path, right: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let Ok(left_meta) = fs::metadata(left) else {
            return false;
        };
        let Ok(right_meta) = fs::metadata(right) else {
            return false;
        };
        left_meta.dev() == right_meta.dev() && left_meta.ino() == right_meta.ino()
    }
    #[cfg(windows)]
    {
        let Ok(left) = fs::canonicalize(left) else {
            return false;
        };
        let Ok(right) = fs::canonicalize(right) else {
            return false;
        };
        left.as_os_str().eq_ignore_ascii_case(right.as_os_str())
    }
    #[cfg(not(any(unix, windows)))]
    {
        fs::canonicalize(left).ok() == fs::canonicalize(right).ok()
            && fs::canonicalize(left).is_ok()
    }
}

fn missing_octocode_message(args: &[String]) -> String {
    let invoked = format_skill_command(args);
    format!(
        "octo skill requires the Node CLI (`octocode`) on PATH.\n\
         Install: npm i -g octocode\n\
         Then:    {invoked}\n\
         Or:      npx -y {invoked}"
    )
}

fn format_skill_command(args: &[String]) -> String {
    let mut command = String::from("octocode skill");
    for arg in args {
        command.push(' ');
        command.push_str(&quote_cli_arg(arg));
    }
    command
}

fn quote_cli_arg(arg: &str) -> String {
    if !arg.is_empty()
        && arg.chars().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | '=' | ':' | '+')
        })
    {
        arg.to_owned()
    } else {
        format!("'{}'", arg.replace('\'', r#"'\''"#))
    }
}

fn lang_from_path(path: &str) -> Option<&'static str> {
    let ext = Path::new(path)
        .extension()
        .and_then(|value| value.to_str())?
        .to_ascii_lowercase();
    Some(match ext.as_str() {
        "rs" => "rust",
        "ts" => "typescript",
        "tsx" => "tsx",
        "js" | "mjs" | "cjs" | "jsx" => "javascript",
        "py" => "python",
        "go" => "go",
        "java" => "java",
        "kt" | "kts" => "kotlin",
        "rb" => "ruby",
        "php" => "php",
        "cs" => "csharp",
        "cpp" | "cc" | "cxx" | "hpp" | "hh" => "cpp",
        "c" | "h" => "c",
        "swift" => "swift",
        "scala" => "scala",
        "json" => "json",
        "yml" | "yaml" => "yaml",
        "md" => "markdown",
        "sh" | "bash" => "bash",
        _ => return None,
    })
}

async fn run(runtime: &ToolRuntime, tool: &str, query: Value) -> u8 {
    execute(runtime, tool, query, true, true, false, None).await
}

#[cfg(test)]
mod tests {
    use super::lang_from_path;

    #[test]
    fn infers_language_from_common_extensions() {
        assert_eq!(lang_from_path("/tmp/a.rs"), Some("rust"));
        assert_eq!(lang_from_path("/tmp/a.ts"), Some("typescript"));
        assert_eq!(lang_from_path("/tmp/a.py"), Some("python"));
        assert_eq!(lang_from_path("/tmp/dir"), None);
    }

    #[test]
    fn login_fails_explicitly() {
        assert_eq!(super::login(), 1);
    }

    #[test]
    fn missing_octocode_message_includes_user_args() {
        assert_eq!(
            super::missing_octocode_message(&["install".into(), "--all".into()]),
            "octo skill requires the Node CLI (`octocode`) on PATH.\n\
             Install: npm i -g octocode\n\
             Then:    octocode skill install --all\n\
             Or:      npx -y octocode skill install --all"
        );
    }

    #[test]
    fn missing_octocode_message_without_extra_args() {
        assert_eq!(
            super::missing_octocode_message(&[]),
            "octo skill requires the Node CLI (`octocode`) on PATH.\n\
             Install: npm i -g octocode\n\
             Then:    octocode skill\n\
             Or:      npx -y octocode skill"
        );
    }

    #[test]
    fn missing_octocode_message_quotes_args_with_spaces() {
        assert_eq!(super::quote_cli_arg("my skill"), "'my skill'");
        assert_eq!(super::quote_cli_arg("it's"), r#"'it'\''s'"#);
        assert_eq!(
            super::missing_octocode_message(&["install".into(), "my skill".into()]),
            "octo skill requires the Node CLI (`octocode`) on PATH.\n\
             Install: npm i -g octocode\n\
             Then:    octocode skill install 'my skill'\n\
             Or:      npx -y octocode skill install 'my skill'"
        );
    }

    #[cfg(unix)]
    #[test]
    fn resolve_skips_self_and_uses_a_later_path_entry() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().expect("tmp");
        let native_dir = root.path().join("native");
        let node_dir = root.path().join("node");
        std::fs::create_dir_all(&native_dir).expect("native dir");
        std::fs::create_dir_all(&node_dir).expect("node dir");
        let native_bin = native_dir.join("octocode");
        let node_bin = node_dir.join("octocode");
        std::fs::write(&native_bin, b"native").expect("native bin");
        std::fs::write(&node_bin, b"node").expect("node bin");
        std::fs::set_permissions(&native_bin, std::fs::Permissions::from_mode(0o755))
            .expect("chmod native");
        std::fs::set_permissions(&node_bin, std::fs::Permissions::from_mode(0o755))
            .expect("chmod node");
        assert_eq!(
            super::resolve_node_octocode_from(vec![native_dir.clone()], Some(&native_bin)),
            None
        );
        assert_eq!(
            super::resolve_node_octocode_from(vec![native_dir, node_dir], Some(&native_bin)),
            Some(node_bin)
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_path_names_include_cmd_and_exe() {
        let names: Vec<String> = super::octocode_path_names()
            .into_iter()
            .map(|name| name.to_string_lossy().into_owned())
            .collect();
        assert!(
            names
                .iter()
                .any(|name| name.eq_ignore_ascii_case("octocode.cmd")),
            "{names:?}"
        );
        assert!(
            names
                .iter()
                .any(|name| name.eq_ignore_ascii_case("octocode.exe")),
            "{names:?}"
        );
    }
}
