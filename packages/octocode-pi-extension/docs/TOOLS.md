# Tools Reference — Pi Extension

Complete reference for every tool registered by `@octocodeai/pi-extension`. The 13 Octocode research tools delegate execution to `@octocodeai/octocode-tools-core`; the Pi-specific tools are implemented directly in `src/tools/`.

---

## Tool Inventory

| Family | Tools |
|--------|-------|
| **Core** | `bash`, `edit`, `write` |
| **GitHub** | `ghSearchCode` · `ghSearchRepos` · `ghHistoryResearch` · `ghGetFileContent` · `ghViewRepoStructure` · `ghCloneRepo` |
| **Local** | `localSearchCode` · `localViewStructure` · `localFindFiles` · `localGetFileContent` · `localBinaryInspect` |
| **LSP** | `lspGetSemantics` |
| **Package** | `npmSearch` |
| **Browser** | `chromeDebug` · `browserAgent` · `spawnSubagent` |
| **Agents** | `spawnAgent` · `AgentMessage` |
| **Web** | `web` |
| **Context** | `manage_context` |
| **Memory** | `memory_recall` · `memory_record` · `memory_reflect` · `workspace_status` · `agent_signal` · `file_lock` · `memory_refine_get` · `memory_audit_unverified` · `memory_verify` · `memory_export_harness` |
| **Aliases** | `memory_workspace_status` · `memory_file_lock` · `memory_notify` |

Source of truth for names: `OCTOCODE_DIRECT_TOOL_NAMES` + `OCTOCODE_SUPPORT_TOOL_NAMES` in `src/constants.ts`.

---

## Routing Guide

| Task | Tool |
|------|------|
| Run shell commands, git, builds | `bash` |
| Edit existing file (exact replacement) | `edit` |
| Create / overwrite a file | `write` |
| Search code across GitHub | `ghSearchCode` |
| Read a file from GitHub | `ghGetFileContent` |
| Browse a GitHub repo tree | `ghViewRepoStructure` |
| Discover GitHub repos | `ghSearchRepos` |
| Search PR / commit history | `ghHistoryResearch` |
| Clone repo for local reads | `ghCloneRepo` |
| Search local files (text / AST) | `localSearchCode` |
| Browse local directory tree | `localViewStructure` |
| Find files by name/size/time | `localFindFiles` |
| Read a local file or range | `localGetFileContent` |
| Inspect archives / binaries | `localBinaryInspect` |
| Symbol identity, refs, callers, types | `lspGetSemantics` |
| Resolve npm package to source | `npmSearch` |
| Single-shot Chrome DevTools call | `chromeDebug` |
| Multi-turn browser session | `spawnSubagent` (agent: "browser-agent") |
| Browser analysis routing | `browserAgent` |
| Spawn background Pi worker | `spawnAgent` |
| Coordinate spawned workers | `AgentMessage` |
| Fetch a URL / web search | `web` |
| Compact / reset context | `manage_context` |
| Recall prior lessons | `memory_recall` |
| Record a root cause / decision | `memory_record` |
| Capture a post-task lesson | `memory_reflect` |
| Check locks + active agents | `workspace_status` |
| Publish / reply to signals | `agent_signal` |
| Lock files for parallel edits | `file_lock` |

---

## Core Tools

### `bash`
Execute shell commands in the current working directory. Returns stdout + stderr (truncated to last 2 000 lines / 50 KB). Use for git, builds, `sed`/bulk edits, and anything local tools cannot cover.

### `edit`
Targeted file replacement using exact current-file text. Detects stale reads before writing. Every edit requires a non-empty `reasoning`. Use `matchMode:"normalized"` for whitespace drift; `matchMode:"lineRange"` with freshly read line numbers as a last resort. **Not** for new files — use `write`.

### `write`
Create or overwrite a file. Automatically creates parent directories. No match guard — overwrites without confirmation. Use only for new files or intentional full rewrites.

---

## GitHub Tools

All accept `{ queries: [...] }` (up to 5 parallel). Support `page`, `responseCharOffset`, `responseCharLength`.

| Tool | Key params | Notes |
|------|-----------|-------|
| `ghSearchCode` | `keywords`, `owner`, `repo`, `match`, `extension`, `path`, `page` | `match:"path"` for filenames; `match:"file"` for snippets |
| `ghSearchRepos` | `keywords`, `language`, `stars`, `sort`, `concise` | Start `concise:true`; follow into `ghViewRepoStructure` |
| `ghHistoryResearch` | `type`, `owner`, `repo`, `prNumber`, `content`, `state` | `type:"prs"` or `type:"commits"`; detail mode needs `prNumber` |
| `ghGetFileContent` | `owner`, `repo`, `path`, `startLine`/`endLine`, `matchString`, `minify`, `branch` | `symbols` → anchor → `none` for edits |
| `ghViewRepoStructure` | `owner`, `repo`, `path`, `maxDepth`, `branch` | Orient before fetching files |
| `ghCloneRepo` | `owner`, `repo`, `branch`, `sparsePath` | Needs `ENABLE_CLONE`; use `sparsePath` to bound checkout |

---

## Local Tools

All accept absolute paths. Strip leading `@` if copied from a Pi file reference.

| Tool | Key params | Notes |
|------|-----------|-------|
| `localViewStructure` | `path`, `recursive`, `maxDepth`, `pattern`, `extensions` | Cheapest orientation step; use before any file read |
| `localSearchCode` | `path`, `keywords`, `mode`, `perlRegex`, `fixedString`, `include`, `contextLines` | Modes: `discovery` · `paginated` · `detailed` · `structural` (AST) |
| `localFindFiles` | `path`, `names`, `regex`, `entryType`, `maxDepth`, `modifiedWithin` | Name/size/time filters; use when content doesn't matter |
| `localGetFileContent` | `path`, `startLine`/`endLine`, `matchString`, `minify`, `fullContent` | `symbols` first for large files; `none` for edits/citations |
| `localBinaryInspect` | `path`, `mode` | Modes: `inspect` · `list` · `extract` · `decompress` · `strings` · `unpack` |

**`localSearchCode` modes:**

| Mode | Use |
|------|-----|
| `discovery` | Paths only — cheapest; find candidates before reading |
| `paginated` | Snippets with surrounding context |
| `detailed` | Full context window |
| `structural` | AST pattern (`pattern`) or rule (`rule`); captures feed `lspGetSemantics` |

---

## LSP Tool

### `lspGetSemantics`

Symbol-level code intelligence. `lineHint` **must** come from a prior search result, `matchRanges`, or `documentSymbols` — never guessed.

| Operation | When to use |
|-----------|------------|
| `definition` | Jump to declaration |
| `references` | All usages of a symbol |
| `callers` / `callees` | Call hierarchy one level |
| `callHierarchy` | Full call graph (use `depth`) |
| `hover` | Type info + docs at a location |
| `documentSymbols` | All symbols in a file (no `lineHint` needed) |
| `workspaceSymbol` | Fuzzy project-wide symbol search |
| `typeDefinition` | Follow to type declaration |
| `implementation` | Find interface implementations |
| `supertypes` / `subtypes` | Type hierarchy |
| `diagnostic` | File-level errors/warnings (no `lineHint` needed) |

---

## Package Tool

### `npmSearch`
Resolve npm package names → GitHub repo. Exact package name returns rich single result with `repository`. Keyword query returns paginated candidates. Follow `repository` into GitHub tools.

---

## Browser Tools

See [`BROWSER_AGENT.md`](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-pi-extension/subagents/browser-agent/BROWSER_AGENT.md) for the full 28-scheme reference, stealth mode, multi-turn protocol, and CDP event log.

### `chromeDebug`
Direct Chrome DevTools Protocol calls. One scheme per call. Use for single-shot tasks.

```
chromeDebug scheme:"debug" url:"https://example.com" port:9222 launch:true
chromeDebug scheme:"screenshot" port:9222 format:"png" fullPage:true
chromeDebug scheme:"raw" method:"Network.getCookies" params:{"urls":["https://example.com"]}
```

Key params: `scheme` (required), `url`, `port` (default 9222), `launch`, `headless`, `stealth`, `durationMs`.

### `browserAgent`
Routes a natural-language browser task to the right CDP scheme(s), runs initial analysis, and returns a spawn config for a dedicated `browser-agent` subagent. Use when the task type is unclear; the tool selects the optimal schemes.

### `spawnSubagent`
Spawn a typed, pre-configured Pi subagent. Currently supports `agent: "browser-agent"`.

```
spawnSubagent({
  agent: "browser-agent",
  task: "audit cookies on https://example.com",
  url: "https://example.com",
  port: 9222,
  launch: true
})
→ agentId: "abc123"
AgentMessage({action:"wait", agentId:"abc123", timeoutMs:60000})
AgentMessage({action:"kill", agentId:"abc123", remove:true})
```

Params: `agent`, `task`, `context`, `url`, `port`, `launch`, `headless`, `model`, `thinking`, `name`, `cwd`.

---

## Agent Tools

### `spawnAgent`
Spawn a background Pi worker process. Returns `agentId` immediately. Prompt must be **self-contained** — worker has zero parent context.

Key params: `task`, `prompt`, `context`, `name`, `cwd`, `model`, `provider`, `thinking`, `tools`, `systemPrompt`, `resourceMode` (`lean` / `octocode` / `default`), `noSession`.

`FORBIDDEN_WORKER_TOOLS`: `spawnAgent`, `AgentMessage` — workers cannot spawn sub-workers.

### `AgentMessage`
Coordinate spawned workers. Always set explicit `timeoutMs` on `wait`.

| Action | Use |
|--------|-----|
| `list` | Show all registered agents + status |
| `status` | Poll one agent without blocking |
| `wait` | Block until agent reaches `idle`/`exited`/`failed` |
| `send` | Queue a message; worker finishes current turn first |
| `steer` | Interrupt mid-turn immediately |
| `followUp` | Queue after current completion |
| `abort` | Graceful stop; process stays alive |
| `kill` | Hard terminate + optional `remove:true` |

Agent lifecycle: `starting` → `running` → `idle` → `exited` / `failed` / `killed`.

---

## Web Tool

### `web`
Fetch a URL as clean text or run a web search. Use `url` to fetch, `query` to search.

Key params: `url`, `query`, `maxResults` (default 5), `maxChars`, `page`, `timeRange`, `includeDomains`, `excludeDomains`, `engine`.

Use for: live docs, error messages, changelogs, current info beyond the codebase.

---

## Context Tool

### `manage_context`
Compact or reset the conversation context.

| Type | When |
|------|------|
| `compact` | ≥ 60% full, at research→execution boundary, before large task, after writing handoff doc |
| `new` | Next task is fully unrelated to current conversation |

Param: `instructions` — focus hint for compaction summary (used with `compact` only).

---

## Memory / Awareness Tools

See [`MEMORY_AGENT_FLOW.md`](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-pi-extension/docs/MEMORY_AGENT_FLOW.md) for live coordination and [`REFLECT.md`](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-pi-extension/docs/REFLECT.md) for the Awareness learning loop.

### Lifecycle pattern

```
[Awareness/start] memory_recall → memory_refine_get → workspace_status
[Awareness/work]  agent_signal  (coordination inbox: questions, handoffs, blockers)
                 file_lock     (parallel-safe edit coordination)
[Awareness/after] memory_audit_unverified → memory_verify(allPending:true)
[Awareness/learn] memory_record (verified root causes, decisions, gotchas)
                  memory_reflect (lesson, fix_repo, fix_harness)
```

### Tool quick-reference

| Tool | Purpose |
|------|---------|
| `memory_recall` | Awareness: retrieve durable lessons before risky/unfamiliar work; flags `judgment_required` when recall confidence is low |
| `memory_record` | Awareness reflection: store verified root cause, decision, workaround, gotcha; reports novelty + similar-memory candidates for supersede decisions |
| `memory_reflect` | Awareness reflection: capture post-task lesson; creates repo-fix refinements, clusters failure patterns; supports `judgment_note`, `duo`, `eval_failures` |
| `workspace_status` | Show active locks, working agents, open signals/refinements, store stats |
| `agent_signal` | Coordination inbox — actions: `publish` · `list` · `reply` · `resolve` · `ack` |
| `file_lock` | Explicit file locks for parallel agents — types: `lock` · `release` · `status` · `renew` |
| `memory_refine_get` | List open repo-fix refinements |
| `memory_audit_unverified` | List pending edit tasks needing verification |
| `memory_verify` | Mark tasks verified/failed — prefer `allPending:true` for batch |
| `memory_export_harness` | Awareness reflection: export human-reviewed skill/harness proposals; never writes files |

### Aliases (compatibility)

| Alias | Canonical |
|-------|-----------|
| `memory_workspace_status` | `workspace_status` |
| `memory_file_lock` | `file_lock` |
| `memory_notify` | `agent_signal` (publish only) |

---

## Configuration

| Variable | Effect |
|----------|--------|
| `OCTOCODE_TOKEN` / `GH_TOKEN` / `GITHUB_TOKEN` | GitHub authentication (priority order) |
| `GITHUB_API_URL` | GitHub Enterprise API base URL |
| `ENABLE_LOCAL` | Set `false` to disable all local tools |
| `ENABLE_CLONE` | Enables `ghCloneRepo` + `ghGetFileContent(type:"directory")` |
| `OCTOCODE_CDP_DEBUG` | Set `1` to write CDP events to `~/.octocode/chrome-debug/port-<N>/cdp-events.jsonl` |

Loaded via `@octocodeai/config`. Run `npx @octocodeai/config --keys` to inspect active values.

---

## Schema Lookup

```bash
# Exact active schema for any tool
node $OCTOCODE_CLI tools <toolName> --scheme

# List all 13 Octocode tools
node $OCTOCODE_CLI tools
```
