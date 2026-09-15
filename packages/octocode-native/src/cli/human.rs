//! Human command families from the native CLI RFC.
use super::execute;
use clap::Args;
use octocode_native::runtime::ToolRuntime;
use serde_json::{Value, json};
use std::path::Path;

/// Output formatting options shared by all human-facing commands.
#[derive(Args, Debug, Default, Clone, Copy)]
pub struct OutputOpts {
    /// Emit indented (pretty-printed) JSON instead of compact JSON.
    #[arg(long)]
    pub pretty: bool,
}

/// List files matching optional name globs.
#[derive(Args, Debug)]
pub struct FilesArgs {
    /// Local file or directory root to search.
    pub path: String,
    /// Basename globs to filter results; repeat or comma-separate (e.g. `--names '*.rs,*.ts'`).
    #[arg(long, value_delimiter = ',')]
    pub names: Vec<String>,
    #[command(flatten)]
    pub output: OutputOpts,
}

/// Show a filesystem or parsed syntax tree.
#[derive(Args, Debug)]
pub struct TreeArgs {
    /// Local file or directory root.
    pub path: String,
    /// Show the parsed syntax tree instead of the filesystem tree (requires a single source file).
    #[arg(long)]
    pub syntax: bool,
    #[command(flatten)]
    pub output: OutputOpts,
}

/// List declarations (functions, classes, types, …) in a file or directory.
#[derive(Args, Debug)]
pub struct SymbolsArgs {
    /// Local file or directory to scan for declarations.
    pub path: String,
    /// Filter by declaration name substring.
    #[arg(long)]
    pub name: Option<String>,
    #[command(flatten)]
    pub output: OutputOpts,
}

/// Structural AST pattern match using ast-grep syntax.
#[derive(Args, Debug)]
pub struct AstArgs {
    /// Local file or directory to search.
    pub path: String,
    /// ast-grep structural pattern (e.g. `fn $NAME($$$) { $$$ }`).
    pub pattern: String,
    #[command(flatten)]
    pub output: OutputOpts,
}

/// File topology and import-graph analysis.
#[derive(Args, Debug)]
pub struct GraphArgs {
    /// Repository or package root to scan.
    pub path: String,
    /// Analysis to run: deadCode, cycles, dependencies, dependents, path, reachability.
    #[arg(value_parser = ["deadCode", "cycles", "dependencies", "dependents", "path", "reachability"])]
    pub analysis: String,
    /// Source file for dependencies/dependents/path analyses (repo-relative).
    #[arg(long)]
    pub file: Option<String>,
    /// Destination file for the `path` (shortest-import-path) analysis (repo-relative).
    #[arg(long)]
    pub target: Option<String>,
    #[command(flatten)]
    pub output: OutputOpts,
}

/// Preview or apply a structural ast-grep rewrite.
#[derive(Args, Debug)]
pub struct RewriteArgs {
    /// Local file or directory to rewrite.
    pub path: String,
    /// ast-grep structural pattern to match.
    pub pattern: String,
    /// Replacement template (supports captured metavariables like `$NAME`).
    #[arg(long = "to")]
    pub replacement: String,
    /// Language for the rewrite (inferred from file extension when omitted).
    #[arg(long)]
    pub lang: Option<String>,
    /// Apply the rewrite in place (default is preview-only).
    #[arg(long)]
    pub apply: bool,
    #[command(flatten)]
    pub output: OutputOpts,
}

/// LSP-backed symbol lookup (definition, references, callers, …).
#[derive(Args, Debug)]
pub struct LspArgs {
    /// Absolute path to the source file.
    pub uri: String,
    /// 1-based source line containing the symbol.
    #[arg(long)]
    pub line: Option<u32>,
    /// 0-based UTF-16 character offset on the line (default 0).
    #[arg(long)]
    pub character: Option<u32>,
    /// Symbol name to anchor (use with --line for disambiguation).
    #[arg(long)]
    pub symbol: Option<String>,
    /// Override the default LSP operation for this command.
    #[arg(long)]
    pub operation: Option<String>,
    #[command(flatten)]
    pub output: OutputOpts,
}

/// Search GitHub repositories by keyword.
#[derive(Args, Debug)]
pub struct ReposArgs {
    /// Keyword(s) to search for.
    pub query: String,
    /// Restrict results to this GitHub owner (user or org).
    #[arg(long)]
    pub owner: Option<String>,
    #[command(flatten)]
    pub output: OutputOpts,
}

/// Clone a GitHub repository into the local Octocode cache.
#[derive(Args, Debug)]
pub struct CloneArgs {
    /// GitHub repository in `OWNER/REPO` format.
    pub repo: String,
    /// Branch, tag, or commit ref to clone (defaults to the default branch).
    #[arg(long)]
    pub branch: Option<String>,
    /// Limit the sparse checkout to this path prefix.
    #[arg(long)]
    pub sparse_path: Option<String>,
    #[command(flatten)]
    pub output: OutputOpts,
}

/// Search or look up packages across registries.
#[derive(Args, Debug)]
pub struct PackageArgs {
    /// Package name (with --info) or keyword to discover.
    pub query: String,
    /// Registry ecosystem: npm, pypi, crates, maven, nuget, go, packagist, rubygems.
    #[arg(long, default_value = "npm", value_parser = ["npm", "pypi", "crates", "maven", "nuget", "go", "packagist", "rubygems"])]
    pub ecosystem: String,
    /// Exact package lookup by name instead of keyword discovery.
    #[arg(long)]
    pub info: bool,
    #[command(flatten)]
    pub output: OutputOpts,
}

/// Search or read GitHub pull requests, issues, and commits.
#[derive(Args, Debug)]
pub struct HistoryArgs {
    /// History operation: `prs`/`issues`/`commits` for search, `pr`/`issue`/`commit` for a single item.
    #[arg(value_parser = ["prs", "issues", "commits", "pr", "issue", "commit"])]
    pub operation: String,
    /// GitHub repository in `OWNER/REPO` format.
    #[arg(long)]
    pub repo: String,
    /// Keywords to filter results (for search operations).
    #[arg(long)]
    pub keywords: Option<String>,
    /// PR or issue number for single-item reads (`pr` or `issue` operations).
    #[arg(long)]
    pub number: Option<u32>,
    /// Commit SHA for single-item reads (`commit` operation); cannot be used with --number.
    #[arg(long, conflicts_with = "number")]
    pub r#ref: Option<String>,
    #[command(flatten)]
    pub output: OutputOpts,
}

pub async fn files(runtime: &ToolRuntime, args: FilesArgs) -> u8 {
    let mut query = json!({"operation":"files","path":args.path});
    if !args.names.is_empty() {
        query["names"] = json!(args.names);
    }
    run(runtime, "astSearch", query, !args.output.pretty).await
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
        !args.output.pretty,
    )
    .await
}

pub async fn symbols(runtime: &ToolRuntime, args: SymbolsArgs) -> u8 {
    let mut query = json!({"operation":"symbols","path":args.path});
    if let Some(name) = args.name {
        query["name"] = json!(name);
    }
    run(runtime, "astSearch", query, !args.output.pretty).await
}

pub async fn ast(runtime: &ToolRuntime, args: AstArgs) -> u8 {
    run(
        runtime,
        "astSearch",
        json!({"operation":"match","path":args.path,"pattern":args.pattern}),
        !args.output.pretty,
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
    run(runtime, "astSearch", query, !args.output.pretty).await
}

pub async fn rewrite(runtime: &ToolRuntime, args: RewriteArgs) -> u8 {
    let pretty = args.output.pretty;
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
        !pretty,
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
    run(runtime, "lspSearch", query, !args.output.pretty).await
}

pub async fn repos(runtime: &ToolRuntime, args: ReposArgs) -> u8 {
    let mut query = json!({"operation":"repositories","keywords":[args.query]});
    if let Some(owner) = args.owner {
        query["owner"] = json!(owner);
    }
    run(runtime, "ghSearch", query, !args.output.pretty).await
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
    run(runtime, "ghCloneRepo", query, !args.output.pretty).await
}

pub async fn package(runtime: &ToolRuntime, args: PackageArgs) -> u8 {
    let mut query = json!({"type":args.ecosystem});
    if args.info {
        query["packageName"] = json!(args.query);
    } else {
        query["keywords"] = json!([args.query]);
    }
    run(runtime, "artifactSearch", query, !args.output.pretty).await
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
            let mut query = json!({"operation":operation,"owner":owner,"repo":repo});
            if operation == "commit" {
                match args.r#ref {
                    Some(sha) => { query["ref"] = json!(sha); }
                    None => {
                        eprintln!("commit operation requires --ref <SHA>");
                        return 2;
                    }
                }
            } else if let Some(number) = args.number {
                query["number"] = json!(number);
            }
            ("ghGetHistoryItem", query)
        }
        other => {
            eprintln!("Unknown history operation: {other}");
            return 2;
        }
    };
    run(runtime, tool, query, !args.output.pretty).await
}

pub async fn context(runtime: &ToolRuntime, json_out: bool, full: bool) -> u8 {
    match runtime.catalog() {
        Ok(catalog) => {
            if json_out {
                return super::write_json(&catalog, true);
            }
            if full && let Some(text) = catalog["mcpInstructions"].as_str() {
                println!("{text}");
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

fn has_auth_token(runtime: &ToolRuntime) -> bool {
    ["GITHUB_TOKEN", "GH_TOKEN", "OCTOCODE_TOKEN"]
        .iter()
        .any(|key| {
            runtime
                .config()
                .env_value(key)
                .is_some_and(|value| !value.is_empty())
        })
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
    let token = has_auth_token(runtime);
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
    let token = has_auth_token(runtime);
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

pub async fn login(refresh: bool) -> u8 {
    if refresh {
        let result =
            octocode_native::providers::github::login::refresh_auth_token_result(
                None, None,
            )
            .await;
        if result.success {
            eprintln!(
                "Refreshed credentials for {}",
                result.hostname.as_deref().unwrap_or("github.com")
            );
            return 0;
        }
        eprintln!(
            "{}",
            result
                .error
                .as_deref()
                .unwrap_or("credential.refreshFailed")
        );
        eprintln!("octocode login, or set GITHUB_TOKEN / GH_TOKEN.");
        return 1;
    }
    use std::io::IsTerminal;
    if !std::io::stdin().is_terminal() {
        eprintln!("login requires an interactive terminal, or set GITHUB_TOKEN / GH_TOKEN.");
        return 1;
    }
    let api = std::env::var("GITHUB_API_URL")
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "https://api.github.com".into());
    let endpoints =
        octocode_native::providers::github::login::LoginEndpoints::from_api_url(&api);
    match octocode_native::providers::github::login::login_device_flow(&endpoints).await
    {
        Ok(stored) => {
            eprintln!(
                "Authenticated as {} on {}",
                stored.username, stored.hostname
            );
            0
        }
        Err(error) => {
            eprintln!("{}", error.message);
            eprintln!("octocode login, or set GITHUB_TOKEN / GH_TOKEN.");
            1
        }
    }
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
    match octocode_native::providers::github::delete_platform_credential(&host) {
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
    let mut command = std::process::Command::new("octocode");
    command.arg("skill").args(args);
    match command.status() {
        Ok(status) => status.code().unwrap_or(1) as u8,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let rest = if args.is_empty() {
                String::new()
            } else {
                format!(" {}", args.join(" "))
            };
            eprintln!("octocode skill requires the Node CLI (`octocode`) on PATH.");
            eprintln!("Install: npm i -g octocode");
            eprintln!("Then:    octocode skill{rest}");
            eprintln!("Or:      npx -y octocode skill{rest}");
            1
        }
        Err(error) => {
            eprintln!("failed to spawn octocode skill: {error}");
            1
        }
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

async fn run(runtime: &ToolRuntime, tool: &str, query: Value, compact: bool) -> u8 {
    execute(runtime, tool, query, true, compact, false, None).await
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
}
