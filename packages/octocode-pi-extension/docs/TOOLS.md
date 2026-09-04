# Tools Reference — Pi Extension

Complete reference for every tool registered by the supported Pi adapter and temporary
parity oracle, `@octocodeai/pi-extension`. This is not the native `octocode-agent` tool
contract. The native product obtains live schemas and composes policy through its own
runtime adapters.

The 13 Octocode research tools are reached through the built-in `octocode` MCP server;
Pi-specific tools are implemented directly in `src/tools/`. Run
`npx octocode tools <name> --scheme` before calling a research tool—the live schema is
authoritative.

The extension supplies its guarded same-name `bash`. For direct extension installs, it
removes Pi `read`/`edit`/`write`/`grep`/`find`/`ls` on load and session start. `file`
consolidates file mutations. See **[OVERRIDES.md](https://github.com/bgauryy/octocode/blob/main/packages/octocode-pi-extension/docs/OVERRIDES.md)** for the contract and
developer code map.

---

## Tool inventory

The direct palette contains 17 extension-owned tools: 16 support tools and the guarded `bash` override. GitHub, local, LSP, and npm research tools are provided indirectly through the built-in `octocode` MCP server.

| Family                   | Direct tools                             |
| ------------------------ | ---------------------------------------- |
| Core                     | `file`, `bash`                           |
| Browser and workers      | `chromeDebug`, `agent`                   |
| Media and web            | `readMedia`, `media`, `runFfmpeg`, `web` |
| MCP                      | `MCPTool`                                |
| Dynamic capabilities     | `callTool`, `skill`                      |
| Planning and interaction | `plan`, `askUser`, `localServer`         |
| Awareness                | `memory`, `lock`, `message`              |

Every direct tool exposes a `queries` batch. Each query requires a non-empty `reasoning` string of at most 240 characters. A call accepts at most 100 queries and validates the full batch before side effects. Sequential mode executes in source order and stops on the first runtime failure. Tools that explicitly expose `queryRunType:"parallel"` overlap only their documented independent operations, run at most four queries concurrently by default, and still return results in source order. Successful batches return a compact receipt index followed by every child content block—including images—in source order; the receipt never replaces model-visible results. One-query calls preserve the underlying result details and rendering contract.

### Tool transcript UI contract

Tool request and result rows use one compositional view shape: **state glyph → tool identity → tool-specific semantic segments → optional evidence body → optional disclosure hint**. Each tool still chooses the useful segments for its domain—for example paths and byte counts for file operations, URLs and pagination for web, exit codes and line counts for Bash, or action/target for Awareness—but the reading order and state language stay stable.

Colors convey meaning rather than decoration:

- brand/accent: a request or operation currently running;
- green: completed successfully;
- red: failed and needs correction;
- gold: warning, blocked state, or user action needed;
- sky/path: filesystem target;
- lavender/link: URL or parallel-policy signal;
- normal/count: totals and numeric evidence;
- bright/title: tool identity, action, or current focal value;
- muted/dim: metadata, previews, reasoning, and disclosure hints.

Renderer limits are view-only. A separate provider boundary makes results above approximately 12,000 characters reference-first: it keeps at most 4,000 diagnostic characters (one-quarter head and three-quarters tail) and two images. The full text goes to a private ephemeral file, and the result includes a `localGetFileContent` chunk-read hint. Session shutdown removes ephemeral tool-output files. Excess images remain recoverable through a private image manifest.

Media renderers are path-backed: generated image bytes are stored once in the
session artifact tree instead of being duplicated as base64 inside result details.
When a batched result exceeds the two-image boundary, the spill manifest reuses
already-safe workspace/session image paths rather than writing duplicate copies.
MCP call details likewise retain only block counts and status metadata; full text,
structured content, and image bytes live solely in the bounded model content or
its lossless spill artifact.

The Awareness CLI remains the canonical backend diagnostics and recovery surface; it does not expand the Pi palette.

Session-scoped maintenance jobs are controlled by `/octocode-cron`; see [CRON.md](https://github.com/bgauryy/octocode/blob/main/packages/octocode-pi-extension/docs/CRON.md). `OCTOCODE_SUPPORT_TOOL_NAMES` in `src/constants.ts` is the direct support-tool source of truth.

---

## Routing Guide

| Task                                                    | Tool                                                           |
| ------------------------------------------------------- | -------------------------------------------------------------- |
| Run shell commands, git, builds                         | `bash`                                                         |
| Edit existing file (exact replacement)                  | `file` with `type:"edit"`                                      |
| Create / overwrite a file                               | `file` with `type:"write"`                                     |
| Delete a file or symbolic link                          | `file` with `type:"delete"`                                    |
| Search code across GitHub                               | `ghSearch` with `operation:"code"`                            |
| Read a file from GitHub                                 | `ghGetFileContent`                                             |
| Browse a GitHub repo tree                               | `ghSearch` with `operation:"tree"`                            |
| Discover GitHub repos                                   | `ghSearch` with `operation:"repositories"`                    |
| Search pull requests                                    | `ghSearchPullRequests`                                         |
| Search issues                                           | `ghSearchIssues`                                               |
| Search commits                                          | `ghSearchCommits`                                              |
| List releases                                           | `ghListReleases`                                               |
| Search discussions                                      | `ghSearchDiscussions`                                          |
| Clone repo for local reads                              | `ghCloneRepo`                                                  |
| Search local files (text / AST)                         | `localSearch` with `operation:"text"` or `"structural"`      |
| Browse local directory tree                             | `localSearch` with `operation:"tree"`                         |
| Find files by name/size/time                            | `localSearch` with `operation:"files"`                        |
| Read a local file or range                              | `localGetFileContent`                                          |
| Find dead-code candidates                               | `localAnalyzeGraph` with `operation:"deadCode"`               |
| Symbol identity, refs, callers, types                   | `lspGetSemantics`                                              |
| Resolve npm package to source                           | `npmSearch`                                                    |
| See a local image / screenshot                          | `readMedia` with `type:"image"`                                |
| Inspect video/audio metadata                            | `readMedia` with `type:"video"` / `"audio"`, `view:"metadata"` |
| See a video frame/contact sheet or audio visualization  | `readMedia` with the matching `view`                           |
| Author an image or PDF                                  | `media` with `type:"image"` / `"pdf"`                          |
| Convert / clip / resize / gif / extract audio           | `media` with `type:"convert"` / `"trim"` / `"gif"` / `"audio"` |
| Single-shot Chrome DevTools call                        | `chromeDebug`                                                  |
| Browser analysis routing                                | `agent` with `profile:"browser"`                               |
| Multi-turn browser session                              | `agent` spawn, then wait/message/steer/abort/kill queries      |
| Spawn background Pi worker                              | `agent` with `type:"spawn"`                                    |
| Coordinate spawned workers                              | `agent` lifecycle queries                                      |
| Fetch a URL / web search                                | `web`                                                          |
| List / call an external MCP server tool                 | `MCPTool`                                                      |
| Add / remove / restart an MCP server (no agent restart) | `MCPTool` (action: add/remove/restart)                         |

> **Built-in `octocode` server.** The gateway resolves the pinned local `octocode-mcp`
> package first and falls back to `npx -y octocode-mcp@latest`. You can't remove the
> built-in entry, but you can override it with `action:"add"`. Active config directories
> are watched for external edits; a change drops stale connections and catalogs before the
> next call. `RUN_MCP_LIVE=1` enables the
> [built-in connection test](https://github.com/bgauryy/octocode/blob/main/packages/octocode-pi-extension/tests/mcp-tool.test.ts), and the
> [external Node MCP integration test](https://github.com/bgauryy/octocode/blob/main/packages/octocode-pi-extension/tests/mcp-external.test.ts) runs by default.

| Reuse/create/maintain a verified dynamic capability | `callTool` |
| Load or manage a reusable multi-step workflow | `skill` with `type:"load"|"call"` |
| Compact or reset context | Pi's native auto-compaction or user `/compact` / `/new`; configure Pi's reserve threshold for 80% |
| Recall prior lessons that may change the approach | `memory` |
| Record a verified reusable root cause / decision | `memory` |
| Inspect deeper shared-state diagnostics | Awareness skill / `$OCTOCODE_AWARENESS_CLI` |
| Send / read needed peer messages | `message` |
| Protect sensitive/non-mergeable files exceptionally | `lock` |
| Diagnose task, handoff, verification, or presence state | `$OCTOCODE_AWARENESS_CLI` (recovery only) |

---

## Core Tools

### `bash`

Execute shell commands in the current working directory. Octocode overrides Pi’s built-in bash with the same shell execution, a path guard on redirect/`tee`/`cp`/`mv` write targets, and a small blocklist of catastrophic commands. Every call requires a non-empty `reasoning` field. Bash streams output to a private ephemeral log and keeps at most about 4,000 model-visible characters: a 1,000-character head and a 3,000-character tail. Renderer metadata contains only the log path and byte/character counts, not a duplicate of stdout or stderr. The in-memory preview source stops at 150,000 characters, but the referenced log continues up to a 64 MiB safety ceiling. Session shutdown deletes the log. Prefer `file` for ordinary mutations; use bash for builds, tests, package commands, and mechanical changes. For more information, see [OVERRIDES.md](https://github.com/bgauryy/octocode/blob/main/packages/octocode-pi-extension/docs/OVERRIDES.md).

### `file`

One guarded mutation boundary with `type:"edit" | "write" | "delete"`:

- `edit`: targeted exact/normalized/lineRange replacements with stale/lost-update checks, BOM/CRLF preservation, and Myers diff/patch details.
- `write`: atomic create or full overwrite with parent-directory creation and post-write read-state recording.
- `delete`: files and symbolic links only; directories are rejected, and metadata is rechecked under the mutation queue before unlinking.

Every query requires one concise `reasoning`. Mixed batches reject duplicate paths and fully preflight every operation before the first mutation. All paths use the shared cwd/home/temp/`ALLOWED_PATHS` guard. Use `delete` only when removal is explicitly in scope. Details: [OVERRIDES.md](https://github.com/bgauryy/octocode/blob/main/packages/octocode-pi-extension/docs/OVERRIDES.md).

---

## GitHub Tools

All use the live `queries` schema reported by `npx octocode tools <name> --scheme`. Do not reuse remembered fields across catalog versions.

| Tool                   | Key params                                                                        | Notes                                                     |
| ---------------------- | --------------------------------------------------------------------------------- | --------------------------------------------------------- |
| `ghSearch` (`code`)    | `operation`, `keywords`, `owner`, `repo`, `match`, `extension`, `path`, `page`    | `match:"path"` for filenames; `match:"file"` for snippets |
| `ghSearch` (`repositories`) | `operation`, `keywords`, `language`, `stars`, `sort`, `concise`             | Start `concise:true`; follow with `operation:"tree"`     |
| `ghSearchPullRequests` | `keywordsToSearch`, `prNumber`, `owner`, `repo`, `state`                          | Search PRs or fetch one PR by number                      |
| `ghSearchIssues`       | `keywordsToSearch`, `issueNumber`, `owner`, `repo`, `state`                       | Search issues or fetch one issue by number                |
| `ghSearchCommits`      | `owner`, `repo`, `path`, `since`, `until`, `base`, `head`, `includeDiff`          | Search commit history or inspect a bounded comparison     |
| `ghGetFileContent`     | `owner`, `repo`, `path`, `startLine`/`endLine`, `matchString`, `minify`, `branch` | `symbols` → anchor → `none` for edits                     |
| `ghSearch` (`tree`)    | `operation`, `owner`, `repo`, `path`, `maxDepth`, `branch`                        | Orient before fetching files                              |
| `ghCloneRepo`          | `owner`, `repo`, `branch`, `sparsePath`                                           | Needs `ENABLE_CLONE`; use `sparsePath` to bound checkout  |

---

## Local Tools

All accept absolute paths. Strip leading `@` if copied from a Pi file reference.

| Tool                  | Key params                                                            | Notes                                                                               |
| --------------------- | --------------------------------------------------------------------- | ----------------------------------------------------------------------------------- |
| `localSearch`         | `operation`, `path`, plus operation-specific fields                  | `text`, `structural`, `files`, and `tree` share one strict schema                    |
| `localGetFileContent` | `path`, `startLine`/`endLine`, `matchString`, `minify`, `fullContent` | `symbols` first for large files; `none` for edits/citations                         |
| `localAnalyzeGraph`   | `operation`, `path`, `entrypoints`, `includeTests`, `excludeDir`      | `deadCode` returns file candidates; prove symbol usage with `lspGetSemantics`        |

**`localSearch` operations:**

| Operation    | Use                                                                       |
| ------------ | ------------------------------------------------------------------------- |
| `text`       | Lexical or regex search with selectable result views                      |
| `structural` | AST pattern (`pattern`) or rule (`rule`); captures feed `lspGetSemantics` |
| `files`      | Path and metadata discovery without reading file contents                 |
| `tree`       | Bounded directory orientation                                             |

---

## LSP Tool

### `lspGetSemantics`

Symbol-level code intelligence. `lineHint` **must** come from a prior search result, `matchRanges`, or `documentSymbols` — never guessed.

| Operation                 | When to use                                       |
| ------------------------- | ------------------------------------------------- |
| `definition`              | Jump to declaration                               |
| `references`              | All usages of a symbol                            |
| `callers` / `callees`     | Call hierarchy one level                          |
| `callHierarchy`           | Full call graph (use `depth`)                     |
| `hover`                   | Type info + docs at a location                    |
| `documentSymbols`         | All symbols in a file (no `lineHint` needed)      |
| `workspaceSymbol`         | Fuzzy project-wide symbol search                  |
| `typeDefinition`          | Follow to type declaration                        |
| `implementation`          | Find interface implementations                    |
| `supertypes` / `subtypes` | Type hierarchy                                    |
| `diagnostic`              | File-level errors/warnings (no `lineHint` needed) |

---

## Package Tool

### `npmSearch`

Resolve npm package names → GitHub repo. Exact package name returns rich single result with `repository`. Keyword query returns paginated candidates. Follow `repository` into GitHub tools.

---

## Browser and agent tools

See [`BROWSER_AGENT.md`](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-pi-extension/subagents/browser-agent/BROWSER_AGENT.md) for the Chrome DevTools scheme reference.

### `chromeDebug`

Runs one direct Chrome DevTools operation. Use it for bounded observations or interactions. A query can select a named scheme or `raw` with a CDP `Domain.method`.

### `agent`

Spawns typed, browser, or custom workers and controls their lifecycle. The `type` discriminator supports `spawn`, `inspect`, `wait`, `message`, `steer`, `abort`, and `kill`.

Worker assistant output and stderr are retained and returned without a transport cap. Collapsed and expanded TUI renderers create bounded previews only at display time; they never mutate the result delivered to the parent agent.

Spawn profiles:

| Profile      | Use                                                                                      |
| ------------ | ---------------------------------------------------------------------------------------- |
| `researcher` | Evidence gathering across web, GitHub, npm, local files, binaries, and LSP.              |
| `planner`    | Dependency-ordered implementation plans, risks, verification strategy, and RFC handoffs. |
| `architect`  | Root-cause and architecture analysis with local tools and targeted shell checks.         |
| `browser`    | Routed multi-turn Chrome DevTools work.                                                  |
| `custom`     | A clean worker with explicit tools, system prompt, and resource mode.                    |

```text
agent({queries:[{
  reasoning:"Delegate an independent browser audit.",
  type:"spawn",
  profile:"browser",
  task:"Audit cookie security on https://example.com",
  url:"https://example.com",
  launch:true
}]})
→ agentId: "abc123"

agent({queries:[{reasoning:"Collect the browser turn.",type:"wait",agentId:"abc123",timeoutMs:60000}]})
agent({queries:[{reasoning:"Free the completed worker.",type:"kill",agentId:"abc123",remove:true}]})
```

Spawn policy is warning-first: task packets should name goal, context, scope, ownership, acceptance, and return shape. Capacity limits block before process creation. Workers never receive the `agent` facade, so recursive spawning is unavailable. Spawn first and use the returned ID in a later call; generated IDs can't be referenced by another item in the same preflighted batch.

### `/octocode-agents`

This user command manages the in-session worker ledger shown in the unified footer. It lists, inspects, kills, prunes, or hides worker records. Killed workers are omitted from the footer; `/octocode-agents` retains complete inspection detail. The model uses `agent` lifecycle queries; users can use `/octocode-agents` directly. See [`AGENT_ORCHESTRATOR.md`](https://github.com/bgauryy/octocode/blob/main/packages/octocode-pi-extension/docs/AGENT_ORCHESTRATOR.md) and [`SUBAGENTS.md`](https://github.com/bgauryy/octocode/blob/main/packages/octocode-pi-extension/docs/SUBAGENTS.md).

## Media and web tools

### `readMedia`

The read-only perception boundary for local media. Each query chooses `type:"image"`, `"video"`, or `"audio"` and provides `path`. Images are returned directly. Video supports `view:"metadata"`, `"frame"`, or `"contactSheet"`; audio supports `view:"metadata"`, `"waveform"`, or `"spectrogram"`. Visual results are sent to the model as image content as well as rendered in capable terminals. Defaults favor useful perception: `contactSheet` for video and `waveform` for audio.

### `media`

The only public creation/transformation boundary. Each query chooses `type:"image"`, `"pdf"`, `"gif"`, `"trim"`, `"audio"`, or `"convert"`.

- `image`: exactly one of `svg` or `html`; an optional `dest` saves the PNG.
- `pdf`: exactly one of `html`, `markdown`, or `images`; `dest` is required.
- `gif`, `trim`, `audio`, `convert`: require `source` and `dest` and use hardened argv-only ffmpeg execution.

Writes are workspace path-guarded and refuse to clobber unless `overwrite:true`. Timestamps accept `"12"`, `"1:05"`, or `"00:01:05.5"`; ffmpeg jobs honor `timeoutSec` (default 120). Chrome is required for HTML/PDF authoring and ffmpeg for transformations. See [MEDIA_TOOL.md](https://github.com/bgauryy/octocode/blob/main/packages/octocode-pi-extension/docs/MEDIA_TOOL.md) for the decision record and exact split.

### `web`

Fetches a public URL as clean text or runs a web search. Query fields select `url` or search `query`, result/page limits, engine, recency, and domain filters.

## Context controls

The extension does not invoke `ctx.compact()` automatically: Pi defines that API as manual compaction, which aborts an active run and does not continue it. Pi's native auto-compaction instead runs after tool results and before the next assistant response, preserving the active run, overflow recovery, and continuation.

To compact at 80%, set Pi's `compaction.reserveTokens` to 20% of the active model context window. For an 8,192-token model this is 1,639 tokens:

```json
{
  "compaction": {
    "enabled": true,
    "reserveTokens": 1639
  }
}
```

Put this in `<project>/.pi/settings.json` or `~/.pi/agent/settings.json`. Recalculate the value when changing to a model with a different context window. Users can invoke Pi's `/compact` and `/new` commands directly.

A model-runtime `maximum output token limit` stop is different from context pressure: shorten or chunk the response, or write long output to a file and return a concise summary and path.

---

## Session artifact routing

Every durable tool output that lands on disk is routed into the **session artifact tree** under
`$OCTOCODE_HOME/extension/workspaces/<workspace-key>/sessions/<session-key>/` and registered in a session manifest
(`manifest.json`). The `session-key` is derived from `sessionManager.getSessionId()` (falls
back to the session-file basename, then `process-<pid>`).

| Producer slot    | Path inside session tree                                                    | Notes                                                                        |
| ---------------- | --------------------------------------------------------------------------- | ---------------------------------------------------------------------------- |
| `plan`           | `plan/plan.html`, `plan/plan.md`, `plan/state.json`, `plan/branches/*.json` | Primary plan artifacts and branch snapshots                                  |
| `browser`        | `browser/port-<N>/session.json`, `browser/screenshots/*.png`                | chromeDebug session metadata and screenshots                                 |
| `compaction`     | `compaction/<timestamp>-<label>.md`, `compaction/latest.md`                 | Compaction checkpoint markdown                                               |
| `checkpoint-ref` | `checkpoint-ref.json`                                                       | Pointer to shadow-git store (stays at `$OCTOCODE_HOME/extension/checkpoints/<cwd-hash>/`) |
| `log`            | `logs/error.txt`                                                            | Extension error/warning log                                                  |
| `image`          | `images/<name>-<ts>.png`                                                    | `media` fallback PNGs                                                        |
| `export`         | `export/latest-ref.json`                                                    | Pointer to the branded session HTML export                                   |

All write operations are atomic (`O_EXCL` temp + rename) and use private permissions
(`0o700` dirs, `0o600` files). A fallback path is used when the session artifact dir
cannot be created (e.g., workspace does not yet exist).

Large generic tool results and bash logs are intentionally not durable session artifacts. They use private files under `$OCTOCODE_HOME/extension/tmp/tool-results/`, include an exact path in the bounded result, support chunked reads through `localGetFileContent`, and are removed during `session_shutdown`. A later write prunes crash leftovers older than 24 hours.

---

## Internal error log

The extension appends extension-visible errors to `logs/error.txt` inside the session
artifact tree (`$OCTOCODE_HOME/extension/workspaces/<workspace-key>/sessions/<session-key>/logs/error.txt`). When session
context is not available, the fallback remains under the same extension-owned workspace root.

- user-visible extension `error` notifications;
- hook middleware exceptions;
- tool executions that end with `isError: true`;
- provider responses with HTTP status `>= 400`.

Each entry includes timestamp, process uptime, source, cwd, Pi mode, model id/reasoning, context usage when available, duration for tool/provider failures, redacted details, stack, and cause. Secret-like fields (`authorization`, `cookie`, `token`, `secret`, `password`, API keys, credentials) are redacted before writing.

Pi-core/runtime banners that do not pass through extension hooks, such as a model-runtime `maximum output token limit` stop, may still require Pi-side logging.

---

## Memory and Awareness

The unified default facades are `plan`, `memory`, `lock`, and `message`. `plan` owns session/shared projection and receipt-gated completion. There is no separate public `task` tool: plan steps become shared Awareness tasks when projection is needed, while `agent.task` is only the assignment text given to a spawned worker. Only unread direct-message counts enter agent context automatically; global state remains in the user dashboard. Advisory presence, peer registry lifecycle, and mutation-time lock checks are automatic. `lock` is only for exceptional non-mergeable exclusivity. Each facade calls the same in-process Awareness library as the CLI.

Use `node "$OCTOCODE_AWARENESS_CLI" <noun> <verb>` for diagnostics or recovery commands that aren't exposed directly and for contract inspection via `schema`. Agents on other hosts may continue using the published Awareness CLI.

See [`AWARENESS_AGENT_FLOW.md`](https://github.com/bgauryy/octocode/blob/main/packages/octocode-pi-extension/docs/AWARENESS_AGENT_FLOW.md) for live coordination, [`REFLECT.md`](https://github.com/bgauryy/octocode/blob/main/packages/octocode-pi-extension/docs/REFLECT.md) for Lite memory guidance, and [`CRON.md`](https://github.com/bgauryy/octocode/blob/main/packages/octocode-pi-extension/docs/CRON.md) for session job controls.

### Signal-driven pattern

```
[plan]     scope auto/session/shared → Start → execute declared check
[complete] plan.complete + observed receipt → shared verification → next ready dependency
[mutation] explicit targets → automatic peer-lock preflight → automatic advisory presence
[signal]   unread direct-message count only → message inbox when relevant
[rare]     lock for non-mergeable state · message for needed peer coordination
[learn]    memory only for verified reusable outcomes that can change future work
```

### CLI quick-reference

| Command                                  | Purpose                                                                                                        |
| ---------------------------------------- | -------------------------------------------------------------------------------------------------------------- |
| `memory recall`                          | Retrieve durable lessons before risky/unfamiliar work; flags `judgment_required` when recall confidence is low |
| `memory store`                           | Store verified root cause, decision, workaround, or gotcha                                                     |
| `memory list\|forget\|delete\|prune`     | Inspect or explicitly remove stale memories                                                                    |
| `status`                                 | Show plans, tasks, locks, work presence, agents, messages, handoffs, checks, and memory counts                 |
| `message send\|inbox\|list\|read\|prune` | Tiny coordination inbox                                                                                        |
| `handoff add\|list\|clear`               | Manual continuation notes for later agents                                                                     |
| `lock acquire\|release\|list`            | Optional exclusive protection for sensitive paths                                                              |
| `check audit`                            | List done tasks still needing check receipts                                                                   |
| `check mark`                             | Mark one exact owned task verified after its declared check; never batch another agent's work.                 |

## MCP Servers

`MCPTool` is the extension's MCP 2026-07-28 gateway for stdio and Streamable HTTP.
It lists and calls tools, validates exact schemas internally, reads resources, gets prompts,
and supports completion without registering each remote tool in Pi. `/mcp` opens the
local, shared-theme connection and enablement manager.

Set `queryRunType:"parallel"` to overlap status/read operations. Tool `call`
entries may run in parallel only when they target distinct servers; same-server
calls and mutating management actions fail preflight instead of racing. Parallel
batches use the shared four-query concurrency cap. Omit the
field for the default sequential, stop-on-first-error behavior.

MCP payloads pass through the shared provider-result budget. Omitted full text is preserved
in a private ephemeral file and referenced by path; up to two image blocks remain model
content. Unsupported block types are preserved as JSON text. When an MCP server emits only
the compact `structuredContent available` stub, the gateway surfaces the complete
`structuredContent` payload instead.

The built-in `octocode` research server is always defined (pinned local `octocode-mcp`,
with `npx -y octocode-mcp@latest` as fallback). Add a trusted stdio command or Streamable
HTTP URL with `action:"add"` or a canonical `servers.json`. Changes hot-refresh the catalog.

Once initialization discovery finishes, `.octocode/discovery.json` records discovered
skills, active MCP server and tool metadata, and MCP config files found in common host
locations. Claude, Cursor, Codex, and `.agents` configs are inventory only and never
auto-spawn. See [Discovery](https://github.com/bgauryy/octocode/blob/main/docs/DISCOVERY.md#mcp-config-discoverability-mcpdiscoveredconfigs)
for the complete cross-host location matrix.

Startup reads a versioned private snapshot from
`$OCTOCODE_HOME/extension/mcp/workspaces/<workspace-digest>/`. `catalog.json` retains exact
schemas for enabled tools from enabled servers. By default the first-turn system prompt
receives a compact `<mcp_catalog_index>` from `mcp.md`; calls still validate against the
exact private schema. Set `OCTOCODE_COMPACT_MCP=0` only to inject the exact catalog for
debugging. `OCTOCODE_MCP_AI_GUIDE=1` opts into model-authored guide generation; otherwise
the guide is deterministic and adds no model request.
There is no prepare action or schema lease.

Cached prompt readiness is independent from live schema refresh: matching `catalog.json`
+ `mcp.md` releases the default compact prompt immediately; exact mode needs only
`catalog.json`. Cold/changed startup waits through two bounded discovery attempts per
enabled server and, only with AI guide generation enabled, bounded generation (35 seconds total),
then freezes stable prompt bytes for that session and persists any late result for the
next one. The shared runtime renderer shows checking, discovery, optional generation,
counts, and degraded state. See [RUNTIME_STATE.md](https://github.com/bgauryy/octocode/blob/main/packages/octocode-pi-extension/docs/RUNTIME_STATE.md).

### 1. Active config locations

The gateway merges one global definition file and one trusted project definition file.
A project entry with the same server name wins.

| Precedence | Scope     | Path                                                                    | Loaded when             |
| ---------- | --------- | ----------------------------------------------------------------------- | ----------------------- |
| 1          | Built-in  | pinned local `octocode-mcp`, with `npx -y octocode-mcp@latest` fallback | Always as `octocode`    |
| 2          | Global    | `$OCTOCODE_HOME/extension/mcp/servers.json`                                 | If the file exists      |
| 3          | Workspace | `$OCTOCODE_HOME/extension/workspaces/<workspace-key>/mcp/servers.json`      | Trusted workspaces only |

For an untrusted project config, the gateway records a skipped source and warning but never
spawns a process. Run `MCPTool({queries:[{reasoning:"Inspect resolved MCP configuration.",action:"config"}]})`
to see the resolved servers, sources, and warnings.

### 2. Add or remove a server

Use `MCPTool` for the managed path:

```js
MCPTool({
  queries: [
    {
      reasoning: "Add the trusted documentation server.",
      action: "add",
      server: "docs",
      scope: "project",
      config: { command: "npx", args: ["-y", "@acme/docs-mcp@latest"] },
    },
  ],
});
```

`action:"add"|"remove"` manages these canonical targets:

| Scope     | Managed path                                                       | Gate                                                            |
| --------- | ------------------------------------------------------------------ | --------------------------------------------------------------- |
| Workspace | `$OCTOCODE_HOME/extension/workspaces/<workspace-key>/mcp/servers.json` | Workspace trust; removal also requires interactive approval     |
| Global    | `$OCTOCODE_HOME/extension/mcp/servers.json`                            | Adding an arbitrary local process requires interactive approval |

You can also edit any active path in the preceding table. The gateway watches existing
active directories and re-reads config on calls, so changes apply without a restart.
Foreign host files are not import sources: copy a trusted **stdio** entry explicitly with
`action:"add"` or into an active file. Once configured there, an external server does not
need to be Octocode-specific: a standards-conforming stdio MCP implementation can expose
instructions, schemas, and tool results through `MCPTool`. The
[external Node MCP integration test](https://github.com/bgauryy/octocode/blob/main/packages/octocode-pi-extension/tests/mcp-external.test.ts) starts a Node MCP SDK
server and verifies list and call operations, `env`, and workspace-relative `cwd` through
all three active project aliases. The [MCP config tests](https://github.com/bgauryy/octocode/blob/main/packages/octocode-pi-extension/tests/mcp-config.test.ts) cover
the equivalent global aliases.

The current gateway does not load URL-only HTTP, SSE, WebSocket, or OAuth entries from
Claude, Cursor, or Codex configs.

### 3. Config file format

JSON with a `mcpServers` object (a bare `servers` object or a top-level name→config map
also work). Each server entry:

```json
{
  "mcpServers": {
    "my-server": {
      "command": "npx",
      "args": ["-y", "@acme/mcp-server@latest"],
      "env": { "ACME_TOKEN": "..." },
      "cwd": "./sub/dir",
      "timeoutMs": 30000,
      "disabled": false,
      "description": "Acme knowledge base"
    }
  }
}
```

| Field         | Required | Notes                                                                                                       |
| ------------- | -------- | ----------------------------------------------------------------------------------------------------------- |
| `command`     | yes      | Executable to spawn (stdio transport).                                                                      |
| `args`        | no       | Array of string arguments.                                                                                  |
| `env`         | no       | Extra environment variables merged over the SDK's safe defaults. Ambient process secrets are not inherited. |
| `cwd`         | no       | Working dir; relative paths resolve from the workspace and are path-guarded.                                |
| `timeoutMs`   | no       | Per-request timeout, clamped `1000..120000` (default `30000`).                                              |
| `disabled`    | no       | `true` skips the server entirely.                                                                           |
| `description` | no       | Human label shown in `list`/`config`.                                                                       |

Server names must match `^[A-Za-z0-9_.-]{1,64}$`. A user entry named `octocode` overrides
the built-in one (its `env` still gets the full-text + npm-cache defaults merged in).

### 4. Use MCP tools

`MCPTool` is a tool bridge, not a worker; it does no planning, memory, or synthesis.

| Action                                | Purpose                                                                                         |
| ------------------------------------- | ----------------------------------------------------------------------------------------------- |
| `list`                                | List servers or one server's tools and schemas; refresh the private cache.                      |
| `describe`                            | Return one exact current schema for explicit inspection.                                        |
| `call`                                | Invoke `server` + `tool` with `arguments`; load and validate the exact schema internally first. |
| `resources` / `read-resource`         | List resources/templates or read one URI.                                                       |
| `prompts` / `get-prompt` / `complete` | List/get prompts or request argument completion.                                                |
| `enable` / `disable`                  | Store a global or workspace server/tool override in the shared database.                        |
| `status`                              | Show configured and running servers; return schema mode and counters in structured details.     |
| `config`                              | Show resolved config sources and warnings.                                                      |
| `restart`                             | Stop and relaunch one `server`; invalidate discovery freshness.                                 |
| `stop`                                | Stop one server or all servers; invalidate affected caches.                                     |
| `add` / `remove`                      | Manage trusted project or approved global config without restarting the agent.                  |

```js
MCPTool({
  queries: [
    {
      reasoning: "Search the documentation.",
      action: "call",
      server: "my-server",
      tool: "searchDocs",
      arguments: { query: "retry policy" },
    },
  ],
});
```

Calls fail closed before `client.callTool`:

| Code                 | Meaning                                                                                             |
| -------------------- | --------------------------------------------------------------------------------------------------- |
| `MCP_SCHEMA_INVALID` | Arguments failed local validation. Errors are bounded and include instance paths.                   |
| `SCHEMA_UNSUPPORTED` | The schema is too large, unserializable, uses an unsupported dialect, or cannot be compiled safely. |
| `SCHEMA_UNAVAILABLE` | The server, tool, or current schema could not be discovered.                                        |

Servers are spawned during best-effort initialization discovery and reused for the
session. `stop` and `restart` recycle them; a later action reconnects on demand. Config
drift and list-change signals invalidate discovery freshness. Compiled
validators are reused only by exact schema digest. Treat every MCP server as arbitrary
code and add only config you trust.

---

## Configuration

| Variable                                       | Effect                                                                              |
| ---------------------------------------------- | ----------------------------------------------------------------------------------- |
| `OCTOCODE_TOKEN` / `GH_TOKEN` / `GITHUB_TOKEN` | GitHub authentication (priority order)                                              |
| `GITHUB_API_URL`                               | GitHub Enterprise API base URL                                                      |
| `ENABLE_LOCAL`                                 | Set `false` to disable all local tools                                              |
| `ENABLE_CLONE`                                 | Enables `ghCloneRepo` + `ghGetFileContent(type:"directory")`                        |
| `OCTOCODE_CDP_DEBUG`                           | Set `1` to write CDP events to `~/.octocode/chrome-debug/port-<N>/cdp-events.jsonl` |

Loaded via `@octocodeai/config`. Run `npx @octocodeai/config --keys` to inspect active values.

---

## Schema Lookup

```bash
# Exact active schema for any tool
node $OCTOCODE_CLI tools <toolName> --scheme

# List all 13 Octocode tools
node $OCTOCODE_CLI tools
```

---

## callTool — self-extending dynamic tools

`callTool` is a meta-tool: request a capability by name and it reuses, creates (with
approval), or maintains a verified **dynamic tool**. Dynamic tools are self-contained
scripts persisted under `getOctocodeHome()/dynamic-tools/`, executed in an isolated Node
subprocess — never registered as first-class Pi tools at runtime.

### Schema

- `toolType` — logical capability name; the O(1) registry key (e.g. `parseCronExpression`).
- `metadata` — runtime args **plus** reserved keys:
  - `intent` — what a new tool should do (used to generate a miss).
  - `reason` — **required to create**: why a persisted reusable tool is justified.
  - `_allow` — approve capabilities, e.g. `["net"]`.
  - `_force` — override the triviality decline.
  - `_approveCreate` — approve creation in `auto` mode without switching to `create`.
  - `_sandboxed` — set `false` to approve creating a NON-sandboxed trusted tool (rare).
- `mode` — `auto` (default: reuse, else propose) · `run` (reuse only) · `create` (generate
  after approval) · `enhance`/`fix` (regenerate existing) · `list` · `delete`.

### Lifecycle

1. **Resolve** — exact name (O(1)) → keyword/description fallback.
2. **Reuse** — run the resolved tool in a sandboxed subprocess.
3. **Propose** — on an `auto` miss, callTool does **not** silently generate. It returns a
   proposal: research (built-in? library? existing tool? one-line command?), brainstorm the
   smallest design, then **ask the user** and re-call with `mode:"create"` + `reason`.
4. **Create** — a tool-smith subagent generates `tool.mjs` + `tool.test.mjs`; registered
   **only if the test passes** (verification gate).
5. **Maintain** — every call prunes unambiguous junk (missing / always-failing tools).

### Guardrails

- **Triviality guard** — a tool must optimize the agent, not bloat it. If a one-line shell
  command already covers it (`date`, `uuidgen`, `base64`, `wc`, `shasum`, `jq`, …), creation
  is declined with the suggested command (override via `metadata._force:true`).
- **Verification gate** — no green test → no registry entry. No stubs.
- **Enforced sandbox (default)** — sandboxed tools run under the **Node permission model**
  (`--permission`): filesystem, network, and child processes are **denied by default** and
  `process.env` is **scrubbed** to a minimal `PATH`. Declared capabilities are _enforced_,
  not advisory — a tool that didn't declare `net`/`fs`/`exec` physically cannot use them
  (`net`→`--allow-net`, `exec`→`--allow-child-process`, `fs`→broad fs read/write). Native
  addons, workers, FFI, and the inspector are never granted. Plus a hard timeout and sha256
  checksum tamper-check on every run.
  Runtime code generation (`eval`/`new Function`) is disabled
  (`--disallow-code-generation-from-strings`), and `metadata` is delivered on **stdin**
  (never argv) so large inputs never hit OS argument limits.
- **`sandboxed` flag (not all tools need it)** — recorded in the manifest (default `true`).
  A trusted tool that needs broad host access can be created with `sandboxed:false`, but only
  when the caller approves via `metadata._sandboxed:false`; it then runs as an ordinary Node
  process with inherited env.
- **Capability approval** — `net`/`fs`/`exec` also require `metadata._allow` at call time, so
  both declaration (manifest) and approval (caller) must agree before a capability is granted.
- **Mandatory reason** — every created tool records why it should exist.
- **Deterministic result cache** — a tool created with `deterministic:true` and no capabilities memoizes results per (name, version, metadata); repeat calls skip the subprocess (`[REUSED …, cached]`). Re-registering a new version busts the cache.
- **Awareness projection** — a live `<dynamic_capabilities>` block is injected into the system prompt each turn (empty when no dynamic tools/skills exist), so the agent knows its self-created tools/skills without an explicit `list`. Rebuilt from disk per turn — no watcher.
- **Concurrency + rollback** — registry writes take a cross-process lock (shared under
  `getOctocodeHome()` across parallel agents); a failed `enhance`/`fix` rolls back to the
  previous good tool, so there is never a soft-broken (stale-checksum) state.

### CRUD

- Read: `mode:"list"`. Delete: `mode:"delete"` (with `toolType`). Update: `enhance`/`fix`.
- Auto-maintenance prunes junk on every call; the `[MAINTAINED]` line reports pruned tools.

Implementation: `src/tools/dynamic-tools.ts` (deterministic core) + `src/tools/call-tool.ts`
(orchestration + codegen). Dynamic **skill** creation is a planned sibling — see the
brainstorm in `.octocode/plans/*/SKILLS-BRAINSTORM.md`.

---

## `skill` dynamic lifecycle

A `skill` query with `type:"call"` is the workflow sibling of `callTool`. A **dynamic skill** is an approved,
reusable multi-step workflow the agent follows: a `SKILL.md` (Agent Skills frontmatter +
ordered steps) plus optional helper files, written to `~/.pi/agent/skills/<name>/` so Pi
discovers it. **Skills orchestrate; `callTool` executes** — any executable helper a skill
ships should run through the callTool sandbox.

### Schema

- `skillType` — skill/workflow name (lowercase `a-z`, `0-9`, hyphens); O(1) registry key.
- `metadata` — reserved keys: `intent` (what the workflow does), `reason` (**required to
  create**), `_approveCreate` (approve in `auto` mode), `_force` (override triviality decline).
- `mode` — `auto` (reuse, else propose) · `use` (reuse only) · `create` (author after
  approval) · `enhance`/`fix` · `list` · `delete`.

### Lifecycle & guardrails (mirrors callTool)

1. **Resolve** exact name (O(1)) → keyword fallback.
2. **Reuse** — returns the `SKILL.md` path + `/skill:<name>` to follow.
3. **Propose** — on an `auto` miss it does **not** silently author; it asks you to research
   (existing skill/tool/command?), brainstorm the smallest workflow, and get user approval.
4. **Create** — a skill-smith subagent authors `SKILL.md`; registered **only if it passes
   frontmatter + structure validation** (the skill verification gate; softer than a tool's
   test gate, so lean on approval + rubric).
5. **Maintain** — every call prunes broken skills (missing/invalid `SKILL.md`).

- **Triviality guard** — a skill must be a _recurring multi-step workflow_, not a one-off a
  single tool/bash/`callTool` covers (override via `metadata._force:true`).
- **Mandatory reason** — every created skill records why it should exist.
- **Discovery** — spawned subagents see a new skill immediately (their skill dirs re-scan per
  spawn); the main process surfaces it after a reload or by reading the returned path through
  `MCPTool` → `localGetFileContent`.

Implementation: `src/tools/dynamic-skills.ts` (deterministic core), `src/tools/call-skill.ts` (private orchestration), and `src/tools/skill-tool.ts` (public facade).
