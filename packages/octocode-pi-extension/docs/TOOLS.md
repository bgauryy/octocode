# Tools Reference — Pi Extension

Complete reference for every tool registered by the supported Pi adapter and temporary
parity oracle, `@octocodeai/pi-extension`. This is not the native `octocode-agent` tool
contract. The native product obtains live schemas and composes policy through its own
runtime adapters.

The 10 Octocode research tools are reached through the built-in `octocode` MCP server;
Pi-specific tools are implemented directly in `src/tools/`. The system prompt includes a
bounded routing index, not tool input schemas. Use `MCPTool action:"describe"` to load the
selected exact schema and, when the host admits dynamic names, a namespaced Pi tool; then call
the returned tool directly. The generic gateway call remains available for fixed allowlists and
batching but is blocked until describe. CLI-only
hosts use `npx octocode tools <name> --scheme` instead.

The extension supplies its guarded same-name `bash`. For direct extension installs, it
removes Pi `read`/`edit`/`write`/`grep`/`find`/`ls` on load and session start. `file`
consolidates file mutations. See **[OVERRIDES.md](https://github.com/bgauryy/octocode/blob/main/packages/octocode-pi-extension/docs/OVERRIDES.md)** for the contract and
developer code map.

---

## Tool inventory

The direct palette contains 15 extension-owned tools: 14 support tools and the guarded `bash` override. GitHub, local, LSP, and npm research tools are provided indirectly through the built-in `octocode` MCP server.

| Family                   | Direct tools                             |
| ------------------------ | ---------------------------------------- |
| Core                     | `file`, `bash`                           |
| Browser and workers      | `chromeDebug`, `agent`                   |
| Media and web            | `inspectMedia`, `media`, `runFfmpeg`, `web` |
| MCP                      | `MCPTool`                                |
| Dynamic capabilities     | `callTool`, `skill`                      |
| Planning and coordination | `plan`, `awareness`, `askUser`, `localServer` |

Every direct tool exposes a `queries` batch. The registered schema requires a non-empty `reasoning` string of at most 400 characters for each query, and the Pi adapter fills a scoped default when the model omits it. A call accepts at most 100 queries. Preflight checks declared inputs across the batch before execution; live permissions, remote schemas, and mutable state are checked again when needed at execution. This is not a transaction or a promise that every operation will succeed. Sequential mode executes in source order and stops on the first runtime failure. Tools that expose `queryRunType:"parallel"` overlap independent operations, run at most four queries concurrently by default, and return results in source order. Receipts distinguish successful, failed, and not-run items. Partial failures preserve completed child content and returned error diagnostics through the shared output budget; cancellation does not label unstarted queued work as executed. Successful one-query calls preserve the underlying detail shape.

### Prompt and schema ownership

`DIRECT_TOOL_DESCRIPTIONS` is the single source used by registration and each
tool definition. Descriptions distinguish when to choose the tool and the
consequence of a nearby wrong choice. Field descriptions keep exact constraints;
registration preserves schema descriptions and example data without truncation.
Dynamic tool and skill generation import `BEHAVIORAL_PROMPT_GUIDANCE` from
agent-contracts.

The dynamic capability index is a bounded inventory, not a complete contract.
Its overflow entry provides an executable `callTool` or `skill` list call.
Browser workers use live CDP schemas; role prompts contain only role-specific
boundaries, with ownership and handback policy supplied by the shared contract.

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

Renderer limits are view-only. The shared registration and batch boundaries preserve complete returned text and images. Tools that paginate their own results expose partial state and executable continuations; the extension does not apply a second output cap. Do not automatically repeat a mutating tool.

Registration converts internal error results and partial batch failures to Pi's
thrown-error channel so the host records failure.
That channel carries text only: image blocks are serialized as JSON alongside
complete text and row diagnostics.

Media renderers are path-backed: generated image bytes are stored once in the
session artifact tree instead of being duplicated as base64 inside result details.
MCP call details likewise retain only block counts and status metadata; full text,
structured content, and image bytes remain in model-facing content. Equivalent
JSON text and structured payloads appear once. Table mode adds a summary without
replacing evidence or continuations. See [MCP result conversion](../src/tools/mcp/sanitize.ts).

The `awareness` tool exposes canonical operations across Context, Work, Message, Memory, and History. A query supplies `operation` plus validated `params`; Pi binds database, workspace, actor, and scope. Use `describe:true` without `params` to inspect an operation's exact schema without executing it or opening storage. Read operations can be batched, while a mutation must be the only query. Host lifecycle callbacks are available only through the explicit host API.

`OCTOCODE_SUPPORT_TOOL_NAMES` in `src/constants.ts` is the direct support-tool source of truth.

---

## Routing Guide

`gh*`, `local*`, `astSearch`, `lspSearch`, and `artifactSearch` below are inner tools of the built-in `octocode` MCP server. In Pi, discover/describe/call them through `MCPTool`; they are not direct Pi tools. Use the bundled `npx octocode tools` route only outside the native MCP facade.

| Task                                                    | Tool                                                           |
| ------------------------------------------------------- | -------------------------------------------------------------- |
| Run authorized builds, tests, or bounded debug commands | `bash`                                                         |
| Edit existing file (exact replacement)                  | `file` with `type:"edit"`                                      |
| Create / overwrite a file                               | `file` with `type:"write"`                                     |
| Delete a file or symbolic link                          | `file` with `type:"delete"`                                    |
| Search code across GitHub                               | `ghSearch` with `operation:"code"`                            |
| Read a file from GitHub                                 | `ghGetFileContent`                                             |
| Browse a GitHub repo tree                               | `ghSearch` with `operation:"tree"`                            |
| Discover GitHub repos                                   | `ghSearch` with `operation:"repositories"`                    |
| Discover remote history                                | `ghSearchHistory`; inspect its live operation schema          |
| Read an exact remote history item                      | `ghGetHistoryItem`; use the returned repository and item ID   |
| Clone repo for local reads                              | `ghCloneRepo`                                                  |
| Search local files (text)                              | `localSearch` with `searchText`; filter with `include` or `exclude` globs |
| Search local syntax                                     | `astSearch` with `operation:"match"`                          |
| Browse local directory tree                             | `astSearch` with `operation:"tree"`                           |
| Find files by name/size/time                            | `astSearch` with `operation:"files"`; use `names`, `pathPattern`, or `pathRegex` |
| Read a local file or range                              | `localFetch`; choose one of `fullContent`, `matchString`, or `startLine` plus `endLine` |
| Find dead-code candidates                               | `astSearch` with `operation:"topology", analysis:"deadCode"` |
| Symbol identity, refs, callers, types                   | `lspSearch`                                                    |
| Resolve package identity or capability                           | `artifactSearch`                                                    |
| See a local image / screenshot                          | `inspectMedia` with `type:"image"`                                |
| Inspect video/audio metadata                            | `inspectMedia` with `type:"video"` / `"audio"`, `view:"metadata"` |
| See a video frame/contact sheet or audio visualization  | `inspectMedia` with the matching `view`                           |
| Author an image or PDF                                  | `media` with `type:"image"` / `"pdf"`                          |
| Convert / clip / resize / gif / extract audio           | `media` with `type:"convert"` / `"trim"` / `"gif"` / `"audio"` |
| Single-shot Chrome DevTools call                        | `chromeDebug`                                                  |
| Browser analysis routing                                | `agent` with `profile:"browser"`                               |
| Multi-turn browser session                              | `agent` spawn, then wait/message/steer/abort/kill queries      |
| Spawn background Pi worker                              | `agent` with `type:"spawn"`                                    |
| Coordinate spawned workers                              | `agent` lifecycle queries                                      |
| Read Awareness state                                    | `awareness` with a direct read operation such as `context.orient` or `message.list` |
| Change Awareness state                                  | `awareness` with one direct mutation operation per batch       |
| Handle internal Awareness hook callbacks                | Host lifecycle (`hook run`, `hooks pre-edit`); excluded from model calls |
| Fetch a URL / web search                                | `web`                                                          |
| List / call an external MCP server tool                 | `MCPTool`                                                      |
| Add / remove / restart an MCP server (no agent restart) | `MCPTool` (action: add/remove/restart)                         |

> **Built-in `octocode` server.** The gateway resolves the pinned local `octocode-mcp`
> package first and falls back to the `octocode-mcp` dependency version in the extension's package manifest. You can't remove the
> built-in entry, but you can override it with `action:"add"`. Active config directories
> are watched for external edits; a change drops stale connections and catalogs before the
> next call. `RUN_MCP_LIVE=1` enables the
> [built-in connection test](https://github.com/bgauryy/octocode/blob/main/packages/octocode-pi-extension/tests/mcp-tool.test.ts), and the
> [external Node MCP integration test](https://github.com/bgauryy/octocode/blob/main/packages/octocode-pi-extension/tests/mcp-external.test.ts) runs by default.

| Reuse/create/maintain a verified dynamic capability | `callTool` |
| Load or manage a reusable multi-step workflow | `skill` with `type:"load"|"call"` |
| Compact or reset context | Pi's native auto-compaction or user `/compact` / `/new`; configure Pi's reserve threshold for 80% |
| Recall prior lessons that may change the approach | `awareness` operation `memory.recall` |
| Record a verified reusable root cause or decision | `awareness` operation `memory.record` |
| Read decision-changing shared state | `awareness` operation `context.orient` |
| Send or read needed peer messages | `awareness` operations `message.send` and `message.list` |
| Protect sensitive, non-mergeable files | `awareness` operation `work.protect` |
| Inspect shared work or verification debt | `awareness` operations `work.list`, `work.show`, and `work.verify` |

---

## Core Tools

### `bash`

Execute shell commands in the current working directory. Octocode overrides Pi’s built-in bash with the same shell execution, a path guard on redirect/`tee`/`cp`/`mv` write targets, and a small blocklist of catastrophic commands. Every call requires a non-empty `reasoning` field. Bash streams output to a private ephemeral log and keeps at most about 4,000 model-visible characters: a 1,000-character head and a 3,000-character tail. Renderer metadata contains only the log path and byte/character counts, not a duplicate of stdout or stderr. The in-memory preview source stops at 150,000 characters, but the referenced log continues up to a 64 MiB safety ceiling. Session shutdown deletes the log. Prefer `file` for ordinary mutations; use bash for builds, tests, package commands, and mechanical changes. For more information, see [OVERRIDES.md](https://github.com/bgauryy/octocode/blob/main/packages/octocode-pi-extension/docs/OVERRIDES.md).

### `file`

One guarded mutation boundary with `type:"edit" | "write" | "delete"`:

- `edit`: targeted exact/normalized/lineRange replacements with stale/lost-update checks, preservation of BOM and line endings outside replaced spans, and Myers diff/patch details. Position-only ranges require a recorded read; malformed Unicode and binary input fail before editing.
- `write`: atomic create or full overwrite with parent-directory creation, canonical target/version checks, exclusive temporary creation, and post-write read-state recording. A competing creator cannot be silently overwritten.
- `delete`: files and symbolic links only; directories are rejected, and native identity/content snapshots are rechecked before unlinking. A symbolic link is removed without deleting its target.

The dedicated extension Rust package executes file I/O and edit-preparation diff in native workers. File reads and mutation content have a 64 MiB limit. Successful receipts include `committed:true`, a separate `durable` sync result, and a versioned `mutation` receipt with applied/no-op classification, pre/post fingerprints, touched byte and line counts, and explicit diff/patch truncation flags. Receipts don't copy file contents. A stale commit returns a runnable `MCPTool` → `localFetch` recovery query. Post-commit sync or bookkeeping failures become warnings. See [FILE_MUTATIONS.md](FILE_MUTATIONS.md) for build requirements, platform support and the native boundary.

Every query requires one concise `reasoning`. Mixed batches reject duplicate paths and fully preflight every operation before the first mutation. All paths use the shared cwd/home/temp/`ALLOWED_PATHS` guard. Use `delete` only when removal is explicitly in scope. Details: [OVERRIDES.md](https://github.com/bgauryy/octocode/blob/main/packages/octocode-pi-extension/docs/OVERRIDES.md).

---

## GitHub Tools

All use the live `queries` schema reported by `npx octocode tools <name> --scheme`. Do not reuse remembered fields across catalog versions.

The live catalog owns research tool names, operations, fields, and availability. Use `ghSearch` for discovery and `ghGetFileContent` for source reads; use `ghSearchHistory` and `ghGetHistoryItem` for history discovery and exact items. `ghCloneRepo` is available only when the reported storage and clone policy permits it. Inspect the schema rather than translating retired tool names or copying fields between operations.

---

## Local Tools

All accept absolute paths. Strip leading `@` if copied from a Pi file reference.

| Tool                  | Key params                                                            | Notes                                                                               |
| --------------------- | --------------------------------------------------------------------- | ----------------------------------------------------------------------------------- |
| `localSearch`         | `searchText`, `path`, plus lexical filters                           | Lexical text/regex search                                                            |
| `astSearch`           | `operation`, `path`, plus operation-specific fields                  | `match`, `files`, `tree`, `symbols`, and `topology` share one strict schema          |
| `localFetch` | `path`, `startLine`/`endLine`, `matchString`, `minify`, `fullContent` | `symbols` first for large files; `none` for edits/citations                         |
| `lspSearch`           | `operation`, `uri`, `symbolName`, `lineHint` or `position`            | Resolve symbol identity through a language server                                    |

**`astSearch` operations:**

| Operation    | Use                                                                       |
| ------------ | ------------------------------------------------------------------------- |
| `match`      | AST pattern (`pattern`) or rule (`rule`); captures feed `lspSearch`       |
| `files`      | Path and metadata discovery without reading file contents                 |
| `tree`       | Bounded directory orientation                                             |

---

## LSP Tool

### `lspSearch`

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

### `artifactSearch`

Find packages by capability, resolve dependencies to registry metadata, or locate upstream source. Require `type` (`npm`, `pypi`, `crates`, `maven`, `nuget`, `go`, `packagist`, `rubygems`) and exactly one of `packageName` or `keywords`. Python/pip/uv use `pypi`, which supports exact lookup only. Discovery defaults to 10 results; copy `next.nextPage` unchanged for continuation. Use separate bulk queries to compare ecosystems.

Use local tools for installed behavior and GitHub tools when the repository is already known. Returned `artifacts[]` provides metadata and source links, not implementation evidence. See the [shared tool reference](../../../docs/OCTOCODE_TOOLS.md#artifactsearch) for npm registry configuration and provider limits.

---

## Browser and agent tools

See [`BROWSER_AGENT.md`](https://github.com/bgauryy/octocode/blob/main/packages/octocode-pi-extension/subagents/browser-agent/BROWSER_AGENT.md) for the Chrome DevTools scheme reference.

### `chromeDebug`

Runs one direct Chrome DevTools operation. Use it for bounded observations or interactions. A query can select a named scheme or `raw` with a CDP `Domain.method`.

### `agent`

Spawns typed, browser, or custom workers and controls their lifecycle. The `type` discriminator supports `spawn`, `inspect`, `wait`, `message`, `steer`, `abort`, and `kill`.

Worker assistant output and stderr are retained and returned without a transport cap. Collapsed and expanded TUI renderers create bounded previews only at display time; they never mutate the result delivered to the parent agent.

Spawn profiles:

| Profile      | Use                                                                                      |
| ------------ | ---------------------------------------------------------------------------------------- |
| `researcher` | Evidence gathering across web, GitHub, package registries, local files, binaries, and LSP.              |
| `planner`    | Dependency-ordered implementation plans, risks, verification strategy, and RFC handoffs. |
| `architect`  | Root-cause and architecture analysis with local tools and targeted shell checks.         |
| `implementer`| One bounded code change under exclusive ownership with an observed acceptance check.     |
| `reviewer`   | Read-only acceptance review with a typed PASS, WARN, or FAIL verdict.                     |
| `browser`    | Routed multi-turn Chrome DevTools work.                                                  |
| `custom`     | An explicit uncovered role with caller-selected tools and a required system prompt.      |

Typed workers use Octocode `MCPTool` and matching skills for repository research; the implementer additionally receives `file` for its explicit ownership. The reviewer receives only `MCPTool`, `skill`, and `awareness`; it is advisory and can't mutate files or mark a plan complete. Role policy keeps researcher/planner/architect/browser product work read-only except for assigned artifacts, and limits shell to Awareness or role-bounded checks. Each worker has a distinct Awareness identity in the parent's database/workspace. Browser workers receive `chromeDebug`, `MCPTool`, `skill`, `awareness`, and `bash`. Custom workers must declare a non-empty role `systemPrompt` and an explicit least-capability `tools` list; `tools:[]` maps to Pi's `--no-tools`, and lean mode disables extension and skill loading.

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

Spawn policy is warning-first: task packets should name goal, context, scope, ownership, acceptance, and return shape. Optional `cohortId` groups related workers in bounded, attention-first inspect summaries. The completion policy requests wrap-up at 80% of the step budget and an honest partial handback at the hard limit. Capacity limits block before process creation. Workers never receive the `agent` facade, so recursive spawning is unavailable. Spawn first and use the returned ID in a later call; generated IDs can't be referenced by another item in the same preflighted batch.

### `/octocode-inbox`

Open the worker picker, select a worker, and choose **View output**, **Steer**, or **Stop**. The footer summarizes normal workers and identifies those needing attention. The model uses the `agent` tool for lifecycle operations. See [Agent Orchestrator](AGENT_ORCHESTRATOR.md) for the full flow.

## Media and web tools

### `inspectMedia`

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

To compact at approximately 80%, set Pi's `compaction.reserveTokens` to `ceil(activeContextWindow * 0.20)`. The following illustrative configuration reserves 1,639 tokens for an 8,192-token model:

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
`$OCTOCODE_HOME/extension/sessions/<session-key>/` and registered in a session manifest
(`manifest.json`). The `session-key` is derived from `sessionManager.getSessionId()` (falls
back to a normalized session-file identity, then `process-<pid>`). Fallback document IDs are
opaque hashes, so private file paths are not copied into the manifest or indexes.

| Producer slot    | Path inside session tree                                                    | Notes                                                                        |
| ---------------- | --------------------------------------------------------------------------- | ---------------------------------------------------------------------------- |
| `plan`           | `plan/plan.html`, `plan/plan.md`, `plan/state.json`, `plan/branches/*.json` | Primary plan artifacts and branch snapshots                                  |
| `browser`        | `browser/port-<N>/session.json`, `browser/screenshots/*.png`                | chromeDebug session metadata and screenshots                                 |
| `compaction`     | `compaction/<timestamp>-<label>.md`, `compaction/latest.md`                 | Compaction checkpoint markdown                                               |
| `log`            | `logs/error.txt`                                                            | Extension error/warning log                                                  |
| `image`          | `images/<name>-<ts>.png`                                                    | `media` fallback PNGs                                                        |
| `export`         | `export/latest-ref.json`                                                    | Pointer to the branded session HTML export                                   |

All write operations are atomic (`O_EXCL` temp + rename) and use private permissions
(`0o700` dirs, `0o600` files). A fallback path is used when the session artifact dir
cannot be created (e.g., workspace does not yet exist).

Large generic tool results and bash logs are intentionally not durable session artifacts. They use private files under `$OCTOCODE_HOME/extension/tmp/tool-results/`, include an exact path in the bounded result, support chunked reads through `localFetch`, and are removed during `session_shutdown`. A later write prunes crash leftovers older than 24 hours.

## Local file history

Successful native `file` mutations are captured before and after through the shared Awareness history store. Awareness owns the private bundled Git objects and metadata; Pi does not invoke system Git or write a second history database. Use `/octocode-rewind` to select a bounded timeline entry, inspect its file-level preview, and approve the same preview for apply. In headless sessions, use `history.timeline`, `history.read`, and `history.restore` through the native tool; restore selects `action: preview` or `action: apply`. Pi never snapshots the whole workspace on input and never rewinds conversation state.

---

## Internal error log

The extension appends extension-visible errors to `logs/error.txt` inside the session
artifact tree (`$OCTOCODE_HOME/extension/sessions/<session-key>/logs/error.txt`). When session
context is not available, the fallback remains under the same extension-owned workspace root.

- user-visible extension `error` notifications;
- hook middleware exceptions;
- tool executions that end with `isError: true`;
- provider responses with HTTP status `>= 400`.

Each entry includes timestamp, process uptime, source, cwd, Pi mode, model id/reasoning, context usage when available, duration for tool/provider failures, redacted details, stack, and cause. Secret-like fields (`authorization`, `cookie`, `token`, `secret`, `password`, API keys, credentials) are redacted before writing.

Pi-core/runtime banners that do not pass through extension hooks, such as a model-runtime `maximum output token limit` stop, may still require Pi-side logging.

---

## Memory and Awareness

`plan` is for complex dependencies, coordinated ownership, consequential risk, substantial work spanning sessions, or an explicit planning request. Skip it for routine fixes. Pi projects shared plan state through canonical Work operations when needed and reuses native IDs and observed receipts.

Pi uses `createAwarenessClient` for routine operations and `createAwarenessHost` for lifecycle-owned history capture. It claims native lifecycle ownership at session start, so shell hooks do not duplicate Pi events. External hosts use the CLI with the same physical database and distinct stable identities. Linked Git worktrees can share coordination state; separate clones or databases cannot.

The model starts with a host briefing or `context.orient`. It uses Context for attributed observations and advisory feedback, Work for ownership, dependencies, path protection, and verification, Message for decision-changing communication, Memory for verified reusable learning, and History for inspection or authorized restore. For the exact twenty-one-operation catalog, see [AWARENESS_AGENT_FLOW.md](AWARENESS_AGENT_FLOW.md) and the [API reference](../../octocode-awareness/docs/API.md).

## MCP Servers

`MCPTool` is the extension's MCP 2026-07-28 gateway for stdio and Streamable HTTP.
It lists and calls tools, validates exact schemas internally, reads resources, gets prompts,
and supports completion without registering each remote tool in Pi. `/mcp` opens the
local, shared-theme connection and enablement manager.

Set `queryRunType:"parallel"` only for independent operations. Tool `call`
entries may share a server: the MCP client correlates requests. This does not
make arbitrary remote tools read-only or their effects independent. Mutating
management actions fail parallel preflight. Batches use the shared four-query
concurrency cap. Omit the
field for the default sequential, stop-on-first-error behavior.

MCP results retain complete text and every image block in model content. Unsupported
block types remain JSON text. The gateway does not apply a second truncation budget;
server-owned partial state and continuations remain intact. When an MCP server emits only
the compact `structuredContent available` stub, the gateway surfaces the complete
`structuredContent` payload instead.

The built-in `octocode` research server uses the installed `octocode-mcp` binary,
with an npx fallback constrained to the extension manifest's dependency version. Add a trusted stdio command or Streamable
HTTP URL with `action:"add"` or a canonical `servers.json`. Changes hot-refresh the catalog.

Once initialization discovery finishes, `.octocode/discovery.json` records discovered
skills, active MCP server and tool metadata, and MCP config files found in common host
locations. Claude, Cursor, Codex, and `.agents` configs are inventory only and never
auto-spawn. See [Discovery](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md#mcp-config-discoverability-mcpdiscoveredconfigs)
for the complete cross-host location matrix.

Startup reads a versioned private snapshot from
`$OCTOCODE_HOME/extension/mcp/workspaces/<workspace-digest>/`. `catalog.json` retains exact
schemas for enabled tools from enabled servers. The system prompt receives one deterministic,
bounded `<mcp_catalog_index>` containing server instructions plus tool names and descriptions;
it does not contain input schemas. `MCPTool action:"describe"` loads one selected exact schema,
registers a namespaced Pi proxy without prompt snippets or guidelines, and activates it for the
next provider request. Call that proxy directly with the target arguments. If a host-level tool
allowlist rejects dynamic names, describe reports that fallback and the generic gateway remains
the callable path. Generic gateway calls require a prior describe receipt and revalidate its schema
digest. After compaction, receipts without an active provider-visible proxy are cleared, forcing a
fresh describe. Calls also validate against the same exact enabled catalog. There is no prepare
action or caller-supplied schema lease.

Local cached-catalog readiness is independent from live schema refresh and does not
prove a provider cache hit: matching `catalog.json` supplies the routing projection.
Cold or changed startup waits through two bounded discovery attempts per enabled server
(35 seconds total), then freezes stable prompt bytes for that session and persists any
late result for the next one. The shared runtime renderer shows checking, discovery,
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

The local validator accepts unstamped schemas with modern keyword semantics and
explicit JSON Schema 2019-09 or 2020-12 declarations. It rejects draft-03 through
draft-07 declarations: their required fields, exclusive bounds, or `$ref` sibling
rules need dialect-aware handling. Removing a declaration is not a schema migration;
the server must publish a semantically equivalent supported schema.

MCP sampling requires interactive consent and uses the active Pi model. The request's
token ceiling and optional temperature reach Pi's model registry; a length-limited
completion reports `maxTokens`, and failed or cancelled completions do not report
success. Sampling preserves complete message payloads as serialized JSON in a user
message; it does not map MCP roles and media into native Pi conversation blocks.
Form elicitation displays the requested schema and validates edited JSON before
acceptance. URL elicitation displays its destination before consent.

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
| `ENABLE_CLONE`                                 | Enables `ghCloneRepo`                        |
| `OCTOCODE_CDP_DEBUG`                           | Set `1` to write CDP events to `~/.octocode/chrome-debug/port-<N>/cdp-events.jsonl` |

Loaded via `@octocodeai/config`. Run `npx @octocodeai/config --keys` to inspect active values.

---

## Schema Lookup

```bash
# Exact active schema for any tool
node $OCTOCODE_CLI tools <toolName> --scheme

# List the current Octocode tool catalog
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
ordered steps) plus optional helper files, written to `$OCTOCODE_HOME/skills/<name>/`
(default `~/.octocode/skills`) for next-turn discovery. **Skills orchestrate; `callTool` executes** — any executable helper a skill
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
  `MCPTool` → `localFetch`.

Implementation: `src/tools/dynamic-skills.ts` (deterministic core), `src/tools/call-skill.ts` (private orchestration), and `src/tools/skill-tool.ts` (public facade).
