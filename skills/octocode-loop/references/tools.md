# Octocode Tools — Transport, Status, Anchors

Load before the first tool/schema call in any loop. Covers MCP↔CLI mapping, reading `status`, schema-first calling, anchor hand-off, and the remote-as-local bridge. Loop logic lives in `loop-protocol.md`.

## Choose transport once

- **MCP server present** → call the tools directly. Host-dependent set; common names: `oqlSearch`, `ghSearchCode`, `ghSearchRepos`, `ghViewRepoStructure`, `ghGetFileContent`, `npmSearch`, `ghHistoryResearch`, plus local-search/local-read and LSP tools when the host exposes them.
- **No MCP** → CLI: `npx octocode <cmd>`. If MCP must be configured, use `{"octocode":{"command":"npx","type":"stdio","args":["@octocodeai/mcp@latest"]}}`.

Don't mix transports mid-loop unless one is unavailable — keep observations comparable across iterations.

## Verb → CLI command → MCP tool

| Loop need | CLI | MCP |
|-----------|-----|-----|
| Search code / OQL | `npx octocode search "<kw>" [owner/repo\|path]` | `ghSearchCode` / `localSearchCode` via `oqlSearch` |
| Find files by path/name | `npx octocode search "<q>" [scope] --search path` | OQL `target:"files"` |
| List tree / structure | `npx octocode search <scope> --tree` | `ghViewRepoStructure` / local structure |
| Read exact slice | `npx octocode search <path> --content-view exact` | `ghGetFileContent` / local read |
| Structural (AST) | `npx octocode search --pattern/--rule --lang <l> <scope>` | structural mode on search |
| LSP semantics | `npx octocode search --op <op> <scope>` | LSP op tools |
| Repos / packages / history | `npx octocode search … --target repositories\|packages\|commits` | `ghSearchRepos` / `npmSearch` / `ghHistoryResearch` |
| Clone / fetch subtree | `npx octocode clone owner/repo/path[@branch]` | `ghCloneRepo` / `ghGetFileContent type:"directory"` |

Names and flags drift — confirm live, don't trust this table blindly.

## Schema-first

Before a raw-tool or bulk call, read its schema: `npx octocode tools <name> --scheme` (CLI) or the tool's field descriptions (MCP). For OQL, read `npx octocode search --scheme` / `--explain` before writing query JSON. Field descriptions define modes, defaults, pagination, limits, and mutual exclusions — the activation context does not, so don't guess them.

## Reading status (the observation)

Every result carries a `status`. Read it first, every iteration:

- `empty` — ran, matched nothing. Verify scope, spelling, branch, extraction mode; broaden or switch surface. In bulk, per-query misses appear under `emptyQueries`.
- `error` — carries a message (and `errorCode`): fix auth / scope / rate limit / validation and retry. In bulk, failures appear under `errors`.
- results — extract anchors now.

## Carrying anchors

The data that threads iterations together: paths, line numbers, match ranges, repo/PR/package ids, branches, and `next.*` / pagination cursors / char offsets. Carry them forward verbatim and use returned cursors for the next page — fabricated offsets or paths break the loop. `localPath` returned in `next.*` / `location.*` is absolute; pass it as-is.

## Remote-as-local bridge

For AST/structural/LSP/test checks on an external repo, bring it local first: `ghCloneRepo` (or `ghGetFileContent type:"directory"` for a subtree), then feed the returned absolute `localPath` to the local search/read/LSP tools. This lets the code-check loop run ground-truth checks on remote code.
