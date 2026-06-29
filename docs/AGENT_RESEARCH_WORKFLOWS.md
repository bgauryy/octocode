# Agent Research Workflows

Product guidance for agent-facing Octocode workflows. Lead with the current unified `search` command, use raw `tools` only after reading the schema, and use MCP tools directly only when the host has registered them.

## Core Rule

Use layered retrieval, not one magic search path:

```text
orient -> search -> fetch exact evidence -> prove -> act
```

Carry anchors forward at every step: package name, `owner/repo`, branch/ref, path, line, match text, PR number, materialized `localPath`, symbol name, and LSP `lineHint`.

## Transport And Setup

Preferred transport order:

1. **MCP registered**: use `localSearchCode`, `ghSearchCode`, `npmSearch`, `lspGetSemantics`, `oqlSearch`, and related tools directly.
2. **CLI available**: use `npx octocode`.
3. **No Octocode transport**: ask the user to run `npx octocode` and authenticate with `npx octocode auth login` when GitHub access is required.

Setup and health commands:

```bash
npx octocode --help
npx octocode auth status --json
npx octocode context
npx octocode tools
npx octocode lsp-server status <file>
npx octocode auth login
```

The built CLI surface is:

- Research/materialization: `search`, `unzip`, `clone`, `cache fetch`
- Raw tools/context: `tools`, `context`
- Management: `skill`, `install`, `auth`, `status`, `lsp-server`

Removed quick-command aliases such as `grep`, `cat`, `ls`, `find`, `lsp`, `pr`, `pkg`, `repo`, `binary`, and `diff` should be expressed as `search` lanes.

## Hard Rules For Agents

1. Prefer `search` for read-only workflows: local files, GitHub, npm packages, LSP semantics, artifacts, PRs, commits, diffs, research packets, and graph proof.
2. Read `search --scheme` before writing OQL JSON; use `search --explain --dry-run --json` when routing or completeness is uncertain.
3. Read `npx octocode tools <name> --scheme` before any raw tool call. Raw fields differ from CLI flags.
4. Use `--json` for automation and `--compact` for low-token exploration.
5. Treat snippets as candidate evidence. Prove claims with fetched file content, exact PR/patch content, materialized local paths, AST, LSP, binary metadata, or tests.
6. Follow returned `next.*`, `pagination`, `charOffset`, `matchPage`, `filePage`, `commentPage`, and `commitPage`. Do not invent offsets, pages, paths, refs, or local paths.
7. Empty results are not absence until spelling, branch/ref, path, language, filters, provider limitations, pagination, auth, and rate limits are checked.
8. Use local/materialized proof for predicates GitHub providers cannot evaluate exactly: AST, PCRE2-only regex, negative file queries, file metadata, LSP semantics, binary/archive inspection, and many-file repeated reads.
9. Batch independent raw-tool queries up to the active schema limit; serialize dependent steps that need returned anchors.
10. Report fallback surfaces explicitly when OQL or `search` cannot express the proof path.

## Surface Selection

| Surface | Use when | Required behavior |
|---------|----------|-------------------|
| `search` shorthand | The workflow is common: text, file read, tree, path search, LSP, PR, history, package, repo, artifact, or diff | Prefer `--json`; preserve paths, refs, line numbers, and continuations. |
| OQL `search --query` | One typed query should route across code/content/files/structure/semantics/research/graph/materialize | Run `search --scheme`; use `--explain` for uncertainty; follow `next.*`. |
| Raw `tools` | `search` cannot express a needed field, selector, or pagination lane | Run `tools <name> --scheme`; pass schema-exact JSON only. |
| MCP direct | MCP tools are registered in the host | Use the same evidence rules and schema discipline. |
| Local shell | Repo maintenance around Octocode itself, or git diff/status/log for local reviews | Prefer Octocode for research so behavior is dogfooded. |

## Tool And Command Map

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

## OQL Coverage And Fallbacks

| Need | Prefer OQL? | Fallback when partial |
|------|-------------|-----------------------|
| Local/GitHub text, regex, structural code search | Yes | Raw `localSearchCode` / `ghSearchCode` for tool-specific fields. |
| Exact local/GitHub content reads | Yes | Raw content tools for unusual pagination or match options. |
| File discovery and tree structure | Yes | `search --search path`, `search --tree`, or raw local/GitHub tools. |
| Remote-as-local proof | Yes via `--repo`/materialize | `cache fetch`, `clone`, or `ghCloneRepo`; continue on returned `localPath`. |
| LSP semantics | Yes | Raw `lspGetSemantics` for fields not exposed by shorthand. |
| Packages and repositories | Yes | Raw `npmSearch` / `ghSearchRepos` when typed rows or continuations matter. |
| PRs, commits, and history | Partly | `search --target pullRequests|commits`; raw `ghHistoryResearch` for selected content and paging. |
| Artifacts, archives, binaries | Partly | `search --target artifacts`, `unzip`, or raw `localBinaryInspect`; continue from `localPath`. |
| Diffs | Partly | `search --target diff` or selected PR patch lanes. |
| Dead code, reachability, package drift | Partly | Start with OQL `target:"research"`; upgrade with `target:"graph"` + LSP proof; confirm destructive cleanup with exact reads/AST/tests. |

## Best Workflows

### 1. Package To Source To Evidence

```text
npx octocode search <package> --target packages --json
-> take owner/repo and directory from package metadata
-> npx octocode search <owner/repo[/dir]> --tree --json
-> npx octocode search <distinctive-symbol> <owner/repo> --view discovery --json
-> npx octocode search <owner/repo/path> --match-string <symbol> --content-view exact --json
-> npx octocode cache fetch ... or npx octocode clone ... only if AST/LSP/local proof is needed
```

### 2. Repo Discovery To Pattern Examples

```text
npx octocode search <keywords> --target repositories --lang <language> --stars ">100" --concise --json
-> shortlist owner/repo candidates
-> npx octocode search <term> <owner/repo> --view discovery --json
-> npx octocode search <owner/repo/path> --content-view symbols --json
-> npx octocode search <owner/repo/path> --match-string <anchor> --content-view exact --json
```

### 3. GitHub Code Search To Fetch

```text
npx octocode search <symbol-or-string> <owner/repo> --view discovery --json
-> read diagnostics and pagination
-> npx octocode search <owner/repo/path> --match-string <anchor> --content-view exact --json
-> cite the fetched file slice, not the search snippet
```

### 4. Remote To Local Bridge

```text
npx octocode search <owner/repo> --tree --depth 1 --json
-> npx octocode cache fetch <owner/repo> <path> --depth tree --json
   or npx octocode clone <owner/repo[/path][@ref]>
   or npx octocode search <repo-relative-path> --repo <owner/repo[@ref]> ...
-> npx octocode search <localPath> --tree --json
-> npx octocode search <term> <localPath> --view discovery --json
-> npx octocode search <localPath> --pattern '<shape>' --lang <lang> --json
-> npx octocode search <file> --op references --symbol <name> --line <lineHint> --json
```

Clone or cache when clone cost buys project context, repeated local searches, AST, LSP, binary inspection, or many-file reads.

### 5. Local Repo Investigation

```text
npx octocode search <path> --tree --depth 1 --json
-> npx octocode search <query> <path> --search path --json
-> npx octocode search <term> <path> --view discovery --json --compact
-> npx octocode search <file> --content-view symbols --json
-> npx octocode search <file> --match-string <anchor> --content-view exact --json
-> npx octocode search <path> --pattern/--rule ... --lang <lang> --json
-> npx octocode search <file> --op references|callers|callees --symbol <name> --line <lineHint> --json
```

Use AST for code shape. Use LSP after a real line anchor exists.

### 6. Change History And Intent

```text
npx octocode search <owner/repo[/path]> --target commits --since <iso> --json
-> inspect commit headlines for PR numbers
-> npx octocode search <owner/repo#N> --target pullRequests --json
-> npx octocode search <owner/repo#N> --target pullRequests --patches --file <path> --json
-> npx octocode search <owner/repo/path> --content-view exact --json for current code
```

Prefer selected patches over full PR dumps.

### 7. Unified OQL Router

```bash
npx octocode search --scheme
npx octocode search --query '{"target":"code","from":{"kind":"local","path":"src"},"where":{"kind":"text","value":"registerTool"},"view":"discovery","limit":10}' --json
npx octocode search --query '{"target":"content","from":{"kind":"local","path":"src/index.ts"},"fetch":{"content":{"match":{"text":"registerTool"},"contentView":"exact"}}}' --json
npx octocode search --query '{"target":"research","from":{"kind":"local","path":"."},"params":{"goal":"find unused exports, transitive dead code, unused files, and package drift","mode":"analyze"}}' --json
```

Use raw tools when `search` cannot express a tool-specific field or exact pagination lane.

### 8. Cache Fetch To Local Proof

```bash
npx octocode cache fetch owner/repo path/to/dir --depth tree --json
npx octocode search /absolute/localPath --tree --json
npx octocode search "symbolName" /absolute/localPath --json --compact
npx octocode search /absolute/localPath/file.ts --match-string "symbolName" --content-view exact --json
```

Use `--force-refresh` only when freshness matters.

### 9. Artifacts, Archives, And Binaries

```text
npx octocode search <file> --target artifacts --inspect
-> npx octocode search <file> --target artifacts --list
-> npx octocode search <file> --target artifacts --strings --json
-> npx octocode search <file> --target artifacts --extract <entry>
-> npx octocode unzip <archive> --json
-> npx octocode search <localPath> --tree
-> npx octocode search <term> <localPath>
-> npx octocode search <file> --content-view exact
```

List before extract. Use strings to find anchors. Use `unzip` when many files matter. Run artifact inspection again on nested `.node`, `.so`, `.dll`, `.wasm`, `.zip`, or compressed files.

### 10. Diff And Patch Review

```text
npx octocode search <left> <right> --target diff --json
-> npx octocode search <owner/repo#N> --target pullRequests --json
-> npx octocode search <owner/repo#N> --target pullRequests --patches --file <path> --json
-> npx octocode search <current-file> --match-string <changedSymbol> --content-view exact --json
```

Use current file content to separate what changed from what exists now.

### 11. Smart Reachability, Unused Symbols, And Package Drift

```bash
npx octocode search --query '{"target":"research","from":{"kind":"local","path":"."},"params":{"goal":"find unused exports, transitive dead code, unused files, and package drift","mode":"analyze"}}' --json
npx octocode search --query '{"target":"graph","from":{"kind":"local","path":"."},"params":{"intent":"symbols","mode":"prove","proof":"lsp","proofLimit":20},"page":1,"itemsPerPage":25}' --json
```

`target:"research"` and `target:"graph"` provide candidate and proof packets, but destructive edits still need exact source inspection, AST/LSP confirmation, and project-specific entrypoint knowledge.

## Diagnostic And Failure Handling

| Signal | Meaning | Next step |
|--------|---------|-----------|
| `auth` / token error | GitHub/npm/private data may be inaccessible | Run `npx octocode auth status --json`; ask for login only when protected data is required. |
| `rate limited` | Provider result is incomplete for now | Preserve query, narrow scope, or retry later. |
| `ENABLE_LOCAL` / local disabled | Local filesystem, clone, directory fetch, LSP, or binary work may be blocked | Use remote-only proof where possible; otherwise enable local tools. |
| `ENABLE_CLONE` / clone disabled | Materialization is unavailable | Use remote content slices or ask to enable clone. |
| `serverUnavailable` / LSP unavailable | Semantic proof is inconclusive | Use AST/exact content; materialize project context; check `npx octocode lsp-server status`. |
| `partialResult`, truncation, `hasMore`, char pagination | Response is incomplete | Follow the advertised continuation. |
| Sanitizer/redaction warning | Secret-like content was masked | Do not reconstruct secrets; cite only non-sensitive evidence. |
| Provider approximation | Provider did not prove every predicate | Materialize and re-run locally, or downgrade confidence. |
| Empty provider result | Could be true absence or bad scope | Verify ref/path/spelling/filters and try structure/read/materialization. |
| Cache hit/stale cache | Local evidence may reflect cached remote content | Use `--force-refresh` only when freshness matters. |

## Evidence Gates

- Search snippets are discovery, not proof.
- Empty status is not absence until scope, spelling, branch, filters, and pagination are checked.
- AST proves syntax shape, not runtime behavior or types.
- LSP proves semantic identity when available; empty/unavailable LSP is inconclusive.
- History and PR patches explain intent and change, not necessarily current behavior.
- Binary strings are hints; prove behavior with source, exports, docs, or runtime evidence.
- Use pagination and match windows before expanding scope.
- Batch independent queries; serialize dependent steps that rely on returned anchors.
- Name the fallback surface when OQL or shorthand search was not expressive enough.

## Completeness Checklist

Before answering from Octocode research, confirm:

1. The corpus is explicit: local path, package, owner/repo, branch/ref, PR number, artifact path, or materialized `localPath`.
2. The surface is justified: MCP, `search`, OQL, or raw tool.
3. Raw-tool fields came from the active `--scheme`.
4. Candidate results were converted into exact evidence.
5. Pagination and continuations were followed or declared unnecessary.
6. Diagnostics and provider limitations were handled.
7. Claims distinguish syntax proof, semantic proof, history proof, binary proof, and runtime/test proof.
8. Fallbacks are named when used.

## References

Internal Octocode references:

- [GitHub Tools Reference](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/GITHUB_TOOLS.md)
- [Local Tools Reference](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/LOCAL_TOOLS.md)
- [LSP Tools Reference](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/LSP_TOOLS.md)
- [Clone & Local Tools Workflow](https://github.com/bgauryy/octocode/blob/main/docs/mcp/CLONE_WORKFLOW.md)
- [Tool Behavior Guide](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/TOOL_BEHAVIOR.md)
- [Octocode CLI Guide](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_CLI.md)
- [Binary Tools Reference](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/BINARY_TOOLS.md)
- [Octocode Query Language](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_QUERY_LANGUAGE.md)

External references:

- [GitHub Code Search syntax](https://docs.github.com/en/search-github/github-code-search/understanding-github-code-search-syntax)
- [GitHub repository search](https://docs.github.com/en/search-github/searching-on-github/searching-for-repositories)
- [npm package search guidance](https://docs.npmjs.com/searching-for-and-choosing-packages-to-download/)
- [ripgrep](https://github.com/BurntSushi/ripgrep)
- [ast-grep AI prompting](https://ast-grep.github.io/advanced/prompting.html)
- [Tree-sitter introduction](https://tree-sitter.github.io/tree-sitter/)
- [Language Server Protocol](https://microsoft.github.io/language-server-protocol/)
