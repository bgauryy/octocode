//! Human command families from the native CLI RFC.
use super::{ExecuteOptions, execute};
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
    /// Language for the AST pattern (required for directories; inferred from file extension).
    /// Values: rust, typescript, tsx, javascript, python, go, java, kotlin, ruby, php, csharp, cpp, c, swift, scala.
    #[arg(long, short = 'l')]
    pub lang: Option<String>,
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
    /// Restrict results to a primary programming language.
    #[arg(long)]
    pub language: Option<String>,
    /// GitHub star range, e.g. `>100` or `10..50`.
    #[arg(long)]
    pub stars: Option<String>,
    /// Repository result ordering.
    #[arg(long, value_parser = ["stars", "forks", "help-wanted-issues", "updated", "best-match"])]
    pub sort: Option<String>,
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
    /// History operation: searches, single-item reads, or `compare` for two refs.
    #[arg(value_parser = ["prs", "issues", "commits", "pr", "issue", "commit", "compare"])]
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
    /// Base ref for `compare`.
    #[arg(long, requires = "head")]
    pub base: Option<String>,
    /// Head ref for `compare`.
    #[arg(long, requires = "base")]
    pub head: Option<String>,
    #[command(flatten)]
    pub output: OutputOpts,
}

/// Directories that are always excluded from file walks unless the user
/// explicitly scopes the search below them. Mirrors the defaults already
/// applied by `localSearch` and `astSearch`.
const DEFAULT_EXCLUDE_DIRS: &[&str] = &[
    "node_modules",
    "target",
    ".git",
    ".svn",
    "dist",
    "out",
    ".next",
    "build",
    ".turbo",
    "coverage",
];

pub async fn files(runtime: &ToolRuntime, args: FilesArgs) -> u8 {
    let mut query = json!({
        "operation": "files",
        "path": args.path,
        "excludeDir": DEFAULT_EXCLUDE_DIRS,
    });
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
    // Infer language from file extension, or require --lang for directories.
    let lang = args
        .lang
        .or_else(|| lang_from_path(&args.path).map(|s| s.to_owned()));
    if lang.is_none() && std::path::Path::new(&args.path).is_dir() {
        eprintln!(
            "ast: cannot infer language for directory `{}`. Add --lang rust (or ts, py, go, …)",
            args.path
        );
        return 2;
    }
    let mut query = json!({"operation":"match","path":args.path,"pattern":args.pattern});
    if let Some(l) = lang {
        query["langType"] = json!(l);
    }
    run(runtime, "astSearch", query, !args.output.pretty).await
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
        eprintln!(
            "rewrite: cannot infer language from `{}`. Add --lang rust (or ts, py, go, java, …)",
            args.path
        );
        return 2;
    };
    let mut query = json!({
        "path": args.path,
        "pattern": args.pattern,
        "rewrite": args.replacement,
        "ruleKind": "pattern",
        "langType": lang,
    });
    if !args.apply {
        return run(runtime, "astRewrite", query, !pretty).await;
    }

    // Applying is deliberately a two-step transaction. The preview supplies the
    // snapshot and per-file hashes required by the canonical apply contract.
    let preview = match runtime
        .execute(
            "cli-rewrite-preview".into(),
            "astRewrite".into(),
            query.clone(),
        )
        .await
    {
        Ok(outcome) => outcome,
        Err(error) => {
            if let Some(payload) = error.payload {
                return super::write_json(&payload, !pretty).max(2);
            }
            return super::write_json(
                &json!({"errorCode": error.code, "error": error.message}),
                !pretty,
            )
            .max(5);
        }
    };
    let Some(data) = preview.structured_content.pointer("/results/0/data") else {
        return super::write_json(
            &json!({"errorCode":"invalidPreview","error":"Rewrite preview returned no result data"}),
            !pretty,
        )
        .max(5);
    };
    let Some(snapshot) = data.get("snapshot").and_then(Value::as_str) else {
        // An empty preview has no transaction to apply; return it unchanged.
        return super::write_json(&preview.structured_content, !pretty);
    };
    let mut expected_hashes = serde_json::Map::new();
    for file in data
        .get("files")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let (Some(path), Some(hash)) = (
            file.get("absolutePath").and_then(Value::as_str),
            file.get("beforeHash").and_then(Value::as_str),
        ) {
            expected_hashes.insert(path.to_owned(), json!(hash));
        }
    }
    if expected_hashes.is_empty() {
        return super::write_json(
            &json!({"errorCode":"invalidPreview","error":"Rewrite preview returned no guarded file hashes"}),
            !pretty,
        )
        .max(5);
    }
    query["apply"] = json!(true);
    query["snapshot"] = json!(snapshot);
    query["expectedHashes"] = Value::Object(expected_hashes);
    run(runtime, "astRewrite", query, !pretty).await
}

pub async fn lsp(runtime: &ToolRuntime, operation: &str, args: LspArgs) -> u8 {
    let op = args.operation.as_deref().unwrap_or(operation);
    let mut query = json!({"operation": op, "uri": args.uri});
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

/// Show hover documentation for a symbol.
pub async fn hover(runtime: &ToolRuntime, args: LspArgs) -> u8 {
    lsp(runtime, "hover", args).await
}

/// Find callers of a function (incoming call hierarchy).
pub async fn callers(runtime: &ToolRuntime, args: LspArgs) -> u8 {
    lsp(runtime, "callers", args).await
}

/// Find callees of a function (outgoing call hierarchy).
pub async fn callees(runtime: &ToolRuntime, args: LspArgs) -> u8 {
    lsp(runtime, "callees", args).await
}

/// Jump to the type definition of a symbol.
pub async fn type_def(runtime: &ToolRuntime, args: LspArgs) -> u8 {
    lsp(runtime, "typeDefinition", args).await
}

/// Find all implementations of a trait or interface.
pub async fn implementation(runtime: &ToolRuntime, args: LspArgs) -> u8 {
    lsp(runtime, "implementation", args).await
}

/// Find supertypes of a type in the type hierarchy.
pub async fn supertypes(runtime: &ToolRuntime, args: LspArgs) -> u8 {
    lsp(runtime, "supertypes", args).await
}

/// Find subtypes of a type in the type hierarchy.
pub async fn subtypes(runtime: &ToolRuntime, args: LspArgs) -> u8 {
    lsp(runtime, "subtypes", args).await
}

fn repos_query(args: &ReposArgs) -> Value {
    let mut query = json!({"operation":"repositories","keywords":[args.query]});
    for (field, value) in [
        ("owner", &args.owner),
        ("language", &args.language),
        ("stars", &args.stars),
        ("sort", &args.sort),
    ] {
        if let Some(value) = value {
            query[field] = json!(value);
        }
    }
    query
}

pub async fn repos(runtime: &ToolRuntime, args: ReposArgs) -> u8 {
    let query = repos_query(&args);
    run(runtime, "ghSearch", query, !args.output.pretty).await
}

/// Search code on GitHub by keyword.
#[derive(Args, Debug)]
pub struct CodeArgs {
    /// Keywords to search for in code.
    pub query: String,
    /// Restrict to this GitHub owner (user or org).
    #[arg(long)]
    pub owner: Option<String>,
    /// Restrict to this repository (owner/repo).
    #[arg(long)]
    pub repo: Option<String>,
    /// File path glob filter (e.g. `*.rs`).
    #[arg(long)]
    pub path: Option<String>,
    /// Programming language filter (e.g. `rust`, `typescript`).
    #[arg(long)]
    pub lang: Option<String>,
    #[command(flatten)]
    pub output: OutputOpts,
}

fn code_query(args: &CodeArgs) -> Value {
    let mut query = json!({"operation":"code","keywords":[args.query]});
    if let Some(owner) = &args.owner {
        query["owner"] = json!(owner);
    }
    if let Some(repo) = &args.repo {
        // Split owner/repo into separate fields if both present.
        if let Some((owner, repo)) = repo.split_once('/') {
            query["owner"] = json!(owner);
            query["repo"] = json!(repo);
        } else {
            query["repo"] = json!(repo);
        }
    }
    if let Some(path) = &args.path {
        query["path"] = json!(path);
    }
    if let Some(language) = &args.lang {
        query["language"] = json!(language);
    }
    query
}

pub async fn code_search(runtime: &ToolRuntime, args: CodeArgs) -> u8 {
    let query = code_query(&args);
    run(runtime, "ghSearch", query, !args.output.pretty).await
}

/// Browse a known GitHub repository tree.
#[derive(Args, Debug)]
pub struct GhTreeArgs {
    /// GitHub repository in `OWNER/REPO` format.
    pub repo: String,
    /// Subdirectory path within the repository (default: root).
    pub path: Option<String>,
    /// Branch, tag, or commit SHA (defaults to default branch).
    #[arg(long)]
    pub branch: Option<String>,
    #[command(flatten)]
    pub output: OutputOpts,
}

pub async fn gh_tree(runtime: &ToolRuntime, args: GhTreeArgs) -> u8 {
    let Some((owner, repo)) = args.repo.split_once('/') else {
        eprintln!("Usage: octocode gh-tree OWNER/REPO [PATH]");
        return 2;
    };
    let mut query = json!({
        "operation": "tree",
        "owner": owner,
        "repo": repo,
        "path": args.path.as_deref().unwrap_or("")
    });
    if let Some(branch) = args.branch {
        query["branch"] = json!(branch);
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
        "pr" | "issue" => {
            let operation = if args.operation == "pr" {
                "pullRequest"
            } else {
                "issue"
            };
            let Some(number) = args.number else {
                eprintln!(
                    "history {}: requires --number N\n  Usage: octocode history {} --repo {owner}/{repo} --number N",
                    args.operation, args.operation
                );
                return 2;
            };
            let query = json!({"operation":operation,"owner":owner,"repo":repo,"number":number});
            ("ghGetHistoryItem", query)
        }
        "commit" => {
            let Some(sha) = args.r#ref else {
                eprintln!(
                    "history commit: requires --ref <SHA>\n  Usage: octocode history commit --repo {owner}/{repo} --ref <SHA>"
                );
                return 2;
            };
            let query = json!({"operation":"commit","owner":owner,"repo":repo,"ref":sha});
            ("ghGetHistoryItem", query)
        }
        "compare" => {
            let (Some(base), Some(head)) = (args.base, args.head) else {
                eprintln!(
                    "history compare: requires --base <REF> --head <REF>\n  Usage: octocode history compare --repo {owner}/{repo} --base main --head feature"
                );
                return 2;
            };
            let query = json!({
                "operation":"compare",
                "owner":owner,
                "repo":repo,
                "base":base,
                "head":head
            });
            ("ghGetHistoryItem", query)
        }
        other => {
            eprintln!("Unknown history operation: {other}");
            return 2;
        }
    };
    run(runtime, tool, query, !args.output.pretty).await
}

/// Group tool names into CLI families.
pub fn tool_family(name: &str) -> &'static str {
    match name {
        "ghSearch" | "ghGetFileContent" | "ghSearchHistory" | "ghGetHistoryItem"
        | "ghCloneRepo" => "GitHub",
        "localSearch" | "localFetch" | "astSearch" | "astRewrite" | "lspSearch" => "Local Code",
        "artifactSearch" => "Package",
        _ => "Other",
    }
}

pub async fn context(runtime: &ToolRuntime, json_out: bool, full: bool, minimal: bool) -> u8 {
    match runtime.catalog() {
        Ok(catalog) => {
            if json_out {
                return super::write_json(&catalog, true);
            }
            let tools_arr = catalog["tools"].as_array();
            let enabled = tools_arr
                .map(|t| t.iter().filter(|v| v["available"] == true).count())
                .unwrap_or(0);
            let total = tools_arr.map(|t| t.len()).unwrap_or(0);
            if minimal {
                let protocol = catalog["protocol"].as_str().unwrap_or("mcp");
                println!("{enabled}/{total} tools enabled  protocol:{protocol}");
                return 0;
            }
            if full && let Some(text) = catalog["mcpInstructions"].as_str() {
                println!("{text}");
                return 0;
            }
            // Default: compact agent context block
            println!("Octocode Native CLI — Agent Context");
            println!("Compact context. Full MCP instructions: `context --full`.");
            println!();
            println!("Human commands (ergonomic wrappers):");
            println!("  search <text> [path]           lexical/regex search");
            println!("  read <path>                    read a local file");
            println!("  fetch owner/repo/path[@ref]    read a GitHub file");
            println!("  files <path> [--names '*.rs']  find files by name/glob");
            println!("  tree <path>                    directory or syntax tree");
            println!("  symbols <path>                 list declarations");
            println!("  ast <path> <pattern> [--lang]  structural pattern match");
            println!("  graph <path> <analysis>        import graph topology");
            println!("  rewrite <path> <pat> --to <t>  structural find-and-replace");
            println!("  def <file> --line <n>          LSP definition");
            println!("  refs <file> --line <n>         LSP references");
            println!("  callers <file> --line <n>      LSP incoming calls");
            println!("  callees <file> --line <n>      LSP outgoing calls");
            println!("  hover <file> --line <n>        LSP hover docs");
            println!("  type-def <file> --line <n>     LSP type definition");
            println!("  implementation <file> --line   LSP implementations");
            println!("  diagnostics <file>             LSP errors/warnings");
            println!("  repos <query>                  GitHub repo search");
            println!("  code <query>                   GitHub code search");
            println!("  gh-tree OWNER/REPO [path]      GitHub repo tree");
            println!("  history <op> --repo O/R        PR/issue/commit search");
            println!("  package <query> [--ecosystem]  package registry lookup");
            println!("  clone OWNER/REPO               cache a shallow clone");
            println!();
            println!("Direct tool dispatch (raw JSON — no wrapper overhead):");
            println!("  <toolName> '<json>'            e.g. localSearch '{{\"queries\":[...]}}'");
            println!("  <toolName> --scheme            print tool input schema");
            println!("  <toolName> --scheme --pretty   indented schema");
            println!();
            println!("Batch independent queries in queries[]; keep dependent probes sequential.");
            println!(
                "Follow next.* continuations unchanged. localSearch/astSearch find candidates; lspSearch proves identity."
            );
            println!();
            println!(
                "Output: compact JSON by default. --pretty: indented. Exit: 0=ok 2=input 3=notfound 4=auth 5=tool 7=ratelimit"
            );
            println!();
            // Grouped tool list
            if let Some(tools) = tools_arr {
                println!("Tools ({enabled} enabled / {total} cataloged):");
                let families = ["GitHub", "Local Code", "Package", "Other"];
                for family in families {
                    let family_tools: Vec<_> = tools
                        .iter()
                        .filter(|t| tool_family(t["name"].as_str().unwrap_or("")) == family)
                        .collect();
                    if family_tools.is_empty() {
                        continue;
                    }
                    println!("  {family}:");
                    for tool in family_tools {
                        let name = tool["name"].as_str().unwrap_or_default();
                        let available = tool["available"].as_bool().unwrap_or(false);
                        let desc = tool["description"].as_str().unwrap_or("");
                        let short_desc = if desc.len() > 70 { &desc[..70] } else { desc };
                        let flag = if available { "" } else { " [disabled]" };
                        println!("    {name}{flag} — {short_desc}");
                    }
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

/// Resolve authentication: checks env vars, then OS keychain, then credentials.json
/// at OCTOCODE_HOME, then gh CLI.
/// Returns `(authenticated, username, source, raw_token)`.
/// - `username` is populated natively only for the platform-keychain source.
/// - `raw_token` is the credential secret when we can expose it (env / file / gh-cli);
///   callers may use it for a live GH API call to resolve `username`.
fn configured_github_host(runtime: &ToolRuntime) -> String {
    runtime
        .config()
        .resolved
        .github
        .api_url
        .parse::<url::Url>()
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .map(|host| {
            if host == "api.github.com" {
                "github.com".to_owned()
            } else {
                host
            }
        })
        .unwrap_or_else(|| "github.com".into())
}

fn oauth_client_id<'a>(runtime: &'a ToolRuntime, host: &str) -> Option<&'a str> {
    runtime
        .config()
        .env_value("OCTOCODE_GITHUB_CLIENT_ID")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or((host == "github.com")
            .then_some(octocode_native::providers::github::login::GITHUB_APP_CLIENT_ID))
}

fn resolve_auth(
    runtime: &ToolRuntime,
    host: &str,
) -> (bool, Option<String>, &'static str, Option<String>) {
    use octocode_native::providers::github::{
        CredentialSourceProvider, GhCliCredentialSource, LegacyCredentialStore,
        PlatformCredentialStore, load_stored_credentials,
    };
    use secrecy::ExposeSecret;
    // 1. Environment variables (fast, no I/O)
    for key in [
        "OCTOCODE_TOKEN",
        "GH_TOKEN",
        "GITHUB_TOKEN",
        "GITHUB_PERSONAL_ACCESS_TOKEN",
    ] {
        if let Some(v) = runtime.config().env_value(key).filter(|v| !v.is_empty()) {
            return (true, None, "env", Some(v.to_owned()));
        }
    }
    let home = runtime.inspect_config().home;
    // 2. OS platform keychain — username comes from keychain metadata directly
    if matches!(PlatformCredentialStore.load_blocking(host), Ok(Some(_))) {
        let username = load_stored_credentials(host)
            .ok()
            .flatten()
            .map(|c| c.username)
            .filter(|u| !u.is_empty());
        return (true, username, "platform", None);
    }
    // 3. credentials.json at OCTOCODE_HOME (written by the JS CLI)
    if let Ok(Some(secret)) = LegacyCredentialStore::new(&home).load_blocking(host) {
        let token = secret.expose_secret().to_owned();
        return (true, None, "file", Some(token));
    }
    // 4. gh CLI token
    if let Ok(Some(secret)) = GhCliCredentialSource.load_blocking(host) {
        let token = secret.expose_secret().to_owned();
        return (true, None, "gh-cli", Some(token));
    }
    (false, None, "none", None)
}

/// Call `GET /user` on the GitHub API with the given token and return the
/// `login` field. Times out after 5 s and silently returns `None` on any error.
async fn fetch_github_username(token: &str, api_base: &str) -> Option<String> {
    let url = format!("{}/user", api_base.trim_end_matches('/'));
    let resp = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        reqwest::Client::new()
            .get(&url)
            .header("Authorization", format!("Bearer {token}"))
            .header(
                "User-Agent",
                concat!("octocode-native/", env!("CARGO_PKG_VERSION")),
            )
            .header("Accept", "application/vnd.github.v3+json")
            .send(),
    )
    .await
    .ok()?
    .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let json: serde_json::Value = resp.json().await.ok()?;
    json.get("login")
        .and_then(|v| v.as_str())
        .map(str::to_owned)
}

/// Returns total bytes in a directory tree (best-effort; skips unreadable entries).
fn dir_size_bytes(path: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    entries
        .filter_map(|e| e.ok())
        .map(|e| {
            let meta = e.metadata().ok();
            if meta.as_ref().is_some_and(|m| m.is_dir()) {
                dir_size_bytes(&e.path())
            } else {
                meta.map(|m| m.len()).unwrap_or(0)
            }
        })
        .sum()
}

/// All known MCP client IDs and their config format.
const ALL_IDES: &[(&str, &str)] = &[
    ("cursor", "json"),
    ("claude-desktop", "json"),
    ("claude-code", "json"),
    ("windsurf", "json"),
    ("vscode-cline", "json"),
    ("vscode-roo", "json"),
    ("vscode-continue", "json"),
    ("zed", "json"),
    ("opencode", "json"),
    ("gemini-cli", "json"),
    ("kiro", "json"),
    ("trae", "json"),
    ("antigravity", "json"),
    ("codex", "toml"),
    ("goose", "yaml"),
];

/// Detect which IDEs have an Octocode MCP entry in their config.
/// Returns ALL known IDEs — configured:true only when the config file exists
/// and contains an octocode entry.
fn detect_mcp_clients(_home: &Path) -> Vec<(&'static str, bool)> {
    use super::mcp_install::config_path;
    ALL_IDES
        .iter()
        .map(|(ide, fmt)| {
            let configured = config_path(ide)
                .and_then(|path| std::fs::read_to_string(&path).ok())
                .map(|content| match *fmt {
                    "toml" => {
                        content.contains("[mcpServers.octocode]") || content.contains("octocode")
                    }
                    "yaml" => content.contains("octocode:") || content.contains("- octocode"),
                    _ => serde_json::from_str::<serde_json::Value>(&content)
                        .ok()
                        .and_then(|v| {
                            v["mcpServers"]
                                .as_object()
                                .map(|s| s.contains_key("octocode"))
                        })
                        .unwrap_or(false),
                })
                .unwrap_or(false);
            (*ide, configured)
        })
        .collect()
}

/// Format bytes as a human-readable size string.
fn human_bytes(bytes: u64) -> String {
    if bytes == 0 {
        return "0 B".into();
    }
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    if bytes < 1_048_576 {
        return format!("{:.1} KB", bytes as f64 / 1024.0);
    }
    format!("{:.1} MB", bytes as f64 / 1_048_576.0)
}

pub async fn status(
    runtime: &ToolRuntime,
    hostname: Option<&str>,
    json_out: bool,
    sync: bool,
) -> u8 {
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
    let hostname = hostname
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| configured_github_host(runtime));
    let (auth, username, source, _token) = resolve_auth(runtime, &hostname);
    // MCP client detection (all 15 IDEs)
    let mcp_clients = detect_mcp_clients(&view.home);
    let configured_count = mcp_clients.iter().filter(|(_, has)| *has).count();
    // Cache size — sub-directory breakdown
    let cache_root = view.home.join("tmp");
    let cache_dirs = ["tmp", "clone", "tree", "response"];
    let mut cache_totals: Vec<(_, u64)> = cache_dirs
        .iter()
        .map(|d| (*d, dir_size_bytes(&view.home.join(d))))
        .collect();
    // legacy: everything in tmp/ is counted under "tmp" already
    let total_cache: u64 = cache_totals.iter().map(|(_, b)| b).sum();
    // keep tmp as the raw dir_size (may overlap); give clone/tree/response their own paths
    cache_totals[0] = ("tmp", dir_size_bytes(&cache_root));
    let sync_analysis = if sync {
        let snapshots = super::mcp_sync::read_all_client_configs();
        Some(super::mcp_sync::analyze(&snapshots))
    } else {
        None
    };
    if json_out {
        let mut payload = json!({
            "home": view.home,
            "storage": view.storage_mode,
            "auth": {
                "authenticated": auth,
                "username": username,
                "hostname": hostname,
                "tokenPresent": auth,
                "tokenSource": source,
            },
            "config": {
                "source": "file",
                "storageMode": view.storage_mode,
            },
            "availableTools": available,
            "mcpClients": mcp_clients.iter().map(|(ide, has)| json!({
                "client": ide,
                "octocodeInstalled": has
            })).collect::<Vec<_>>(),
            "cache": {
                "totalBytes": total_cache,
                "details": {
                    "tmp": cache_totals[0].1,
                    "clone": cache_totals[1].1,
                    "tree": cache_totals[2].1,
                    "response": cache_totals[3].1,
                }
            }
        });
        if let Some(ref analysis) = sync_analysis {
            payload["sync"] = super::mcp_sync::sync_json(analysis);
        }
        return super::write_json(&payload, true);
    }
    // Human output
    let auth_line = match (auth, &username) {
        (true, Some(user)) => format!("authenticated as {user} (source: {source})"),
        (true, None) => format!("authenticated (source: {source})"),
        (false, _) => "unauthenticated".into(),
    };
    println!("home:    {}", view.home.display());
    println!("storage: {}", view.storage_mode);
    println!("auth:    {auth_line}");
    println!();
    println!(
        "MCP Clients  ({configured_count}/{} configured)",
        mcp_clients.len()
    );
    for (ide, has) in &mcp_clients {
        println!("  {} {ide}", if *has { "✓" } else { "○" });
    }
    println!();
    println!("Cache  {} total", human_bytes(total_cache));
    for (label, bytes) in &cache_totals {
        println!("  {label:<10} {}", human_bytes(*bytes));
    }
    println!();
    println!("Tools  ({} enabled):", available.len());
    println!("  {}", available.join(" "));
    if let Some(ref analysis) = sync_analysis {
        println!();
        println!(
            "Sync  ({} unique MCPs across {} clients)",
            analysis.summary.total_unique_mcps, analysis.summary.clients_with_config
        );
        if analysis.summary.consistent > 0 {
            println!("  ✓ {} fully synced", analysis.summary.consistent);
        }
        if analysis.summary.needs_sync > 0 {
            println!(
                "  ○ {} missing in some configs",
                analysis.summary.needs_sync
            );
        }
        if analysis.summary.conflicts > 0 {
            println!(
                "  ! {} conflicts across MCP configs",
                analysis.summary.conflicts
            );
        }
    }
    0
}

pub async fn auth_status(runtime: &ToolRuntime, json_out: bool) -> u8 {
    let api_base = &runtime.config().resolved.github.api_url;
    let hostname = configured_github_host(runtime);
    let (authenticated, username, source, token) = resolve_auth(runtime, &hostname);
    // Resolve username via GH API when the credential source doesn't carry it
    let username = if authenticated && username.is_none() {
        match &token {
            Some(tok) => fetch_github_username(tok, api_base).await,
            None => None,
        }
    } else {
        username
    };
    if json_out {
        return super::write_json(
            &json!({
                "success": true,
                "authenticated": authenticated,
                "username": username,
                "hostname": hostname,
                "tokenPresent": authenticated,
                "tokenConfigured": authenticated,
                "tokenSource": source,
                "publicGitHubAccess": if authenticated { "authenticated" } else { "unauthenticated" }
            }),
            true,
        );
    }
    if authenticated {
        if let Some(user) = &username {
            println!("authenticated as {user} (source: {source})");
        } else {
            println!("authenticated (source: {source})");
        }
        0
    } else {
        eprintln!("unauthenticated");
        eprintln!("Run `octocode login` or set GITHUB_TOKEN / GH_TOKEN.");
        1
    }
}

pub async fn login(runtime: &ToolRuntime, refresh: bool) -> u8 {
    let host = configured_github_host(runtime);
    let client_id = oauth_client_id(runtime, &host);
    if host != "github.com" && client_id.is_none() {
        eprintln!(
            "OCTOCODE_GITHUB_CLIENT_ID is required for GitHub Enterprise device login and refresh."
        );
        return 1;
    }
    if refresh {
        let result = octocode_native::providers::github::login::refresh_auth_token_result(
            Some(&host),
            client_id,
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
    let api = &runtime.config().resolved.github.api_url;
    let endpoints = octocode_native::providers::github::login::LoginEndpoints::from_api_url(api);
    match octocode_native::providers::github::login::login_device_flow_with_client_id(
        &endpoints,
        client_id.expect("public GitHub or validated enterprise client ID"),
    )
    .await
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
    execute(
        runtime,
        tool,
        query,
        ExecuteOptions {
            structured: true,
            compact,
            ..ExecuteOptions::default()
        },
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::{CodeArgs, OutputOpts, ReposArgs, code_query, lang_from_path, repos_query};

    #[test]
    fn code_query_uses_the_canonical_language_field() {
        let query = code_query(&CodeArgs {
            query: "ToolRuntime".into(),
            owner: None,
            repo: Some("bgauryy/octocode".into()),
            path: Some("src".into()),
            lang: Some("rust".into()),
            output: OutputOpts::default(),
        });
        assert_eq!(query["language"], "rust");
        assert!(query.get("langType").is_none());
        assert_eq!(query["owner"], "bgauryy");
        assert_eq!(query["repo"], "octocode");
        octocode_native::contracts::prepare_and_validate(
            "ghSearch",
            query,
            octocode_native::contracts::PrepareOptions::default(),
        )
        .expect("human code query remains canonical");
    }

    #[test]
    fn repository_query_exposes_canonical_filters() {
        let query = repos_query(&ReposArgs {
            query: "parser".into(),
            owner: Some("rust-lang".into()),
            language: Some("rust".into()),
            stars: Some(">100".into()),
            sort: Some("stars".into()),
            output: OutputOpts::default(),
        });
        octocode_native::contracts::prepare_and_validate(
            "ghSearch",
            query,
            octocode_native::contracts::PrepareOptions::default(),
        )
        .expect("human repository query remains canonical");
    }

    #[test]
    fn infers_language_from_common_extensions() {
        assert_eq!(lang_from_path("/tmp/a.rs"), Some("rust"));
        assert_eq!(lang_from_path("/tmp/a.ts"), Some("typescript"));
        assert_eq!(lang_from_path("/tmp/a.py"), Some("python"));
        assert_eq!(lang_from_path("/tmp/dir"), None);
    }
}
