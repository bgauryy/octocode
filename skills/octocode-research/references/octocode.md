# Octocode Tools & Interfaces

Load when a task needs Octocode setup, transport choice, tool selection, authentication, or CLI command syntax.
For routing and evidence rules, see `references/algorithm.md`; this file covers tool and interface choice.

## Interfaces

Octocode research tools are reachable through two interfaces; host-exposed subsets can vary:

| Interface | Use when | Notes |
|---|---|---|
| **MCP tools** | the host exposes them directly (`localSearchCode`, `ghSearchCode`, `lspGetSemantics`, `npmSearch`, `localBinaryInspect`, ...) | preferred — no shell hop, typed params. Read the tool's input schema before calling. |
| **CLI** (`npx octocode`) | MCP tools are not exposed, or you need `--scheme`/`--explain`/`--dry-run` introspection | read `npx octocode tools <name> --scheme` before raw calls; read `npx octocode search --scheme --compact` before hand-writing OQL JSON |

If neither interface is available: continue only with clearly degraded confidence, or ask the user to install/authenticate. Ask for `npx octocode auth login` only when the task actually requires GitHub/private data.
When another skill says "use `octocode-research` if installed", check the current skill/tool list or host discovery first, then fall back to CLI probes, and ask only if neither path works.

Materialize or run locally for predicates remote providers cannot prove exactly: AST, PCRE2-only regex, negative file queries, file metadata, LSP semantics, binary/archive inspection, and many-file repeated reads.

## Tool Matrix

| Tool (MCP) | CLI form | Surface | Role |
|---|---|---|---|
| `localSearchCode` | `search <term> <path> --view discovery` | local | text/regex/AST search, count modes, ranked — the workhorse |
| `localGetFileContent` | `search <file> --content-view exact\|symbols --match-string <s>` | local | read file / matchString slices / line ranges |
| `localViewStructure` | `search <path> --tree --depth N` | local | directory tree — orientation |
| `localFindFiles` | `search <query> <path> --search path --name <glob> --ext <list>` | local | find by name/size/time/permissions — the constraint is *about* the file |
| `localBinaryInspect` | `search <file> --target artifacts --inspect\|--list\|--strings\|--extract\|--decompress` | local | archives, compressed streams, native binaries |
| `lspGetSemantics` | `search <file> --op documentSymbols\|references\|definition\|callers\|callees\|hover --symbol S --line N` | local | definitions, references, callers/callees, hover, symbols, types — proving identity/impact |
| `ghSearchCode` | `search <term> <owner/repo> --view discovery` | external | GitHub code/path search |
| `ghGetFileContent` | `search <owner/repo/path> --content-view exact\|symbols` | external | read GitHub file (slices/ranges/symbols); `type:"directory"` materializes a subtree |
| `ghViewRepoStructure` | `search <owner/repo> --tree` | external | GitHub tree browse; `resolvedBranch` in the result governs every follow-up |
| `ghSearchRepos` | `search <keywords> --target repositories --lang <lang> --stars ">N"` | external | repo discovery |
| `ghHistoryResearch` | `search <owner/repo[#N]> --target pullRequests\|commits` | external | PR search + deep-read, commit history — archaeology |
| `npmSearch` | `search <package> --target packages` | external | package → source repo (+ `repositoryDirectory`) |
| `oqlSearch` | `search --query '<json>'` | both | typed federated query — multi-predicate, remote+local in one plan |
| `ghCloneRepo` | `clone <owner/repo[/path][@ref]>` | bridge | full/sparse clone (**gated: `ENABLE_CLONE=true`**) |

Bulk: every tool takes up to 5 parallel queries per call with a per-query `id` — batch independent probes into ONE call, it is the cheapest parallelism available.

Other flows composed from the tools above, not 1:1 with a single tool:

| Need | CLI | Notes |
|---|---|---|
| Diff/patch | `search <left> <right> --target diff` or PR patch flags | OQL diff / `ghHistoryResearch` patches |
| Dead-code/reachability/drift | `search --query '{"target":"research",...}'` → `target:"graph"` with `proof:"lsp"` | `oqlSearch`; candidates until upgraded — see `references/code-research.md` |

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

Run commands as `npx octocode <command>`. Useful probes:

```bash
npx octocode --help
npx octocode auth status --json
npx octocode context
npx octocode tools
npx octocode tools <name> --scheme
npx octocode lsp-server status <file>
```

Current CLI surface: research/materialization (`search`, `unzip`, `clone`, `cache fetch`), raw tools/context (`tools`, `context`), management (`skill`, `install`, `auth`, `status`, `lsp-server`).

Removed quick-command aliases such as `grep`, `cat`, `ls`, `find`, `lsp`, `pr`, `pkg`, `repo`, `binary`, and `diff` should be expressed as `search` lanes.
Use `--json` for automation and `--compact` for low-token exploration. Follow returned continuations, refs, and `localPath` values exactly.

## Diagnostics

| Signal | Next step |
|--------|-----------|
| Auth/token error | Run `npx octocode auth status --json`; ask for login only when protected data is required. |
| Rate limit | Preserve the query, narrow scope, or retry later; mark provider evidence incomplete. |
| Local or clone disabled | `ENABLE_LOCAL` defaults `true`; if local/LSP tools fail, check `ENABLE_LOCAL` isn't `false` and `local.enabled` isn't `false` in `.octocoderc`. `ghCloneRepo` needs `ENABLE_CLONE=true` (defaults `false` on MCP, `true` on CLI). Use remote proof where possible while blocked. |
| LSP unavailable/empty | Treat semantic proof as inconclusive; use AST/exact content, materialize project context, or check `npx octocode lsp-server status`. |
| Partial/truncated output | Follow the advertised continuation before widening scope. |
| Sanitizer/redaction warning | Do not reconstruct secrets; cite only non-sensitive evidence. |
| Provider approximation | Materialize and re-run locally, or downgrade confidence. |
| Empty provider result | Verify ref/path/spelling/filters and try structure/read/materialization before calling absence. |
| Cache hit/stale cache | Use `--force-refresh` only when freshness matters. |
