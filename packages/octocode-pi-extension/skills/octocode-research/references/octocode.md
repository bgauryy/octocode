# Octocode MCP and CLI

Load this when a task needs Octocode setup, transport choice, authentication, or command examples.

## Choose Transport

Use Octocode MCP tools directly when the host exposes them, such as `localSearchCode`, `ghSearchCode`, `ghGetFileContent`, `npmSearch`, `lspGetSemantics`, or `localBinaryInspect`. Read the tool description and input schema before calling.

When MCP tools are not exposed, prefer the CLI with `npx octocode`. Read live help before relying on flags, and read `npx octocode tools <name> --scheme` before raw tool calls.

If neither MCP nor CLI is available, continue only with clearly degraded confidence or ask the user to install/authenticate Octocode. For GitHub/private data, ask for `npx octocode auth login` only when that access is required.

## Search First Rules

- Prefer `npx octocode search` for read-only workflows: local files, GitHub, npm packages, LSP semantics, artifacts, PRs, commits, diffs, research packets, graph proof, and materialization.
- Read `npx octocode search --scheme` before OQL JSON. Use `--explain --dry-run --json` when routing or completeness is uncertain.
- Read `npx octocode tools <name> --scheme` before raw tools. Raw fields differ from CLI flags.
- Use `--json` for automation and `--compact` for low-token exploration.
- Follow returned `next.*`, pagination, char offsets, match/file/comment/commit pages, refs, and `localPath` values exactly.
- Empty results are not absence until spelling, branch/ref, path, language, filters, provider limitations, pagination, auth, and rate limits are checked.
- Materialize or run locally for predicates remote providers cannot prove exactly: AST, PCRE2-only regex, negative file queries, file metadata, LSP semantics, binary/archive inspection, and many-file repeated reads.
- Batch independent raw-tool queries up to the active schema limit; serialize dependent steps that need returned anchors.

## MCP Install

Configure the MCP server as:

```json
"octocode": {
  "command": "npx",
  "type": "stdio",
  "args": [
    "@octocodeai/mcp@latest"
  ]
}
```

Restart the host/editor after changing MCP configuration.

## CLI Usage

Run commands as `npx octocode <command>`.

Useful probes:

```bash
npx octocode --help
npx octocode auth status --json
npx octocode context
npx octocode tools
npx octocode tools <name> --scheme
npx octocode lsp-server status <file>
```

The current CLI surface is:

- Research/materialization: `search`, `unzip`, `clone`, `cache fetch`.
- Raw tools/context: `tools`, `context`.
- Management: `skill`, `install`, `auth`, `status`, `lsp-server`.

Removed quick-command aliases such as `grep`, `cat`, `ls`, `find`, `lsp`, `pr`, `pkg`, `repo`, `binary`, and `diff` should be expressed as `search` lanes.

## Command Map

| Need | Current CLI | Raw/MCP tool |
|------|-------------|--------------|
| Unified read-only research | `npx octocode search ...` | `oqlSearch` |
| Local/GitHub text or regex search | `npx octocode search <term> <path\|owner/repo> --view discovery` | `localSearchCode` / `ghSearchCode` |
| AST structural search | `npx octocode search <path> --pattern '<ast>' --lang <lang>` or `--rule '<yaml>'` | `localSearchCode(mode:"structural")` |
| Exact content read | `npx octocode search <file\|owner/repo/path> --content-view exact --match-string <s>` | `localGetFileContent` / `ghGetFileContent` |
| Tree/structure | `npx octocode search <path\|owner/repo> --tree --depth N` | `localViewStructure` / `ghViewRepoStructure` |
| File/path metadata search | `npx octocode search <query> <path> --search path --name <glob> --ext <list>` | `localFindFiles` or OQL `target:"files"` |
| LSP semantics | `npx octocode search <file> --op references|definition|callers|callees|hover --symbol S --line N` | `lspGetSemantics` |
| Package lookup | `npx octocode search <package> --target packages` | `npmSearch` |
| Repository discovery | `npx octocode search <keywords> --target repositories` | `ghSearchRepos` |
| PR list/deep-read | `npx octocode search owner/repo[#N] --target pullRequests --comments --patches --file <path>` | `ghHistoryResearch(type:"prs")` |
| Commit history | `npx octocode search owner/repo[/path] --target commits --since <iso>` | `ghHistoryResearch(type:"commits")` |
| Clone/materialize repo | `npx octocode clone owner/repo[/path][@ref]` or `npx octocode cache fetch owner/repo [path] --depth file|tree|clone` | `ghCloneRepo` / directory fetch |
| Artifacts/binaries | `npx octocode search <file> --target artifacts --inspect|--list|--strings|--extract|--decompress`; `npx octocode unzip <archive>` | `localBinaryInspect` |
| Diff/patch | `npx octocode search <left> <right> --target diff` or PR patch flags | OQL diff / `ghHistoryResearch` patches |
| Dead-code/reachability | `npx octocode search --query '{"target":"research",...}'` then `target:"graph"` with `proof:"lsp"` | `oqlSearch` |

Use raw tools when `search` cannot express a needed field, selector, or pagination lane. Use local shell for repo maintenance around Octocode itself and for git diff/status/log during local reviews.

## Diagnostics

| Signal | Next step |
|--------|-----------|
| Auth/token error | Run `npx octocode auth status --json`; ask for login only when protected data is required. |
| Rate limit | Preserve the query, narrow scope, or retry later; mark provider evidence incomplete. |
| Local or clone disabled | Use remote proof where possible, or ask to enable local/clone when AST, LSP, binary, or materialized proof is required. |
| LSP unavailable/empty | Treat semantic proof as inconclusive; use AST/exact content, materialize project context, or check `npx octocode lsp-server status`. |
| Partial/truncated output | Follow the advertised continuation before widening scope. |
| Sanitizer/redaction warning | Do not reconstruct secrets; cite only non-sensitive evidence. |
| Provider approximation | Materialize and re-run locally, or downgrade confidence. |
| Empty provider result | Verify ref/path/spelling/filters and try structure/read/materialization before calling absence. |
| Cache hit/stale cache | Use `--force-refresh` only when freshness matters. |
