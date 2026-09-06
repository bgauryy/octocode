# `@octocodeai/pi-extension` — Full Harness Reference

Everything the extension registers with Pi on load: tools, system-prompt sections, MCP, subagents, skills, slash commands, flags, lifecycle hooks, and UI surfaces.

---

## System Prompt

The lightweight host facts in `src/prompts/system-prompt.ts` describe capabilities,
untrusted data, permission boundaries, and `/configuration`. They are built into
`dist/system/SYSTEM_PROMPT.md` and supplied through `before_agent_start`. Workflow,
planning depth, skill choice, and response style follow the user's request.
No regex-triggered repo-state, output-recovery, or editor-comment prompts are injected.

On the first main-agent turn, the hook assembles either the eager `<mcp_catalog>` or lazy `<mcp_catalog_index>`, `<dynamic_capabilities>` (dynamic `callTool` and `skill` `type:"call"` registries), the complete available-skills projection, and the initial `<active_plan>`, then freezes those exact system-prompt bytes for the session. Runtime plan/tool results remain in transcript context; compaction adds a bounded `<octocode_compaction_context>` marker with the native summary and active-plan pointer. With `--no-context` set, the hook suppresses project context before freezing the prompt.

---

## Tools

### Native Research Tools — 0 (removed — MCP-only)

All 10 catalogued Octocode research tools (GitHub, local, graph, LSP, npm) are **not registered as native Pi tools**. They are served via the built-in `octocode` MCP server through `MCPTool`, keeping their schemas out of Pi’s direct `tools[]` array.

**Call pattern:**
```js
MCPTool({queries:[{reasoning:"Search remote code.", action:"call", server:"octocode", tool:"ghSearch",
  arguments:{queries:[{reasoning:"Find candidate files.", operation:"code", keywords:["..."]}]}}]})
```

Catalogued tools via `MCPTool server:"octocode"`: `ghSearch` · `ghGetFileContent` · `ghSearchHistory` · `ghGetHistoryItem` · `ghCloneRepo` · `npmSearch` · `localSearch` · `localGetFileContent` · `localAnalyzeGraph` · `lspGetSemantics`. Runtime availability can disable individual tools such as cloning.

`warmMcpCatalog()` runs at `session_start`. The default first-turn prompt consumes the concise deterministic `mcp.md` guide while exact schemas stay private for validation. Set `OCTOCODE_COMPACT_MCP=0` only to inject the exact catalog for debugging; set `OCTOCODE_MCP_AI_GUIDE=1` to opt into model-authored guide generation. Calls always validate against the exact private catalog; use `MCPTool action:"describe"` before calling an unfamiliar tool or whenever the compact guide leaves an operation ambiguous.

**Edit stale-check**: `MCPTool` intercepts `server:"octocode" tool:"localGetFileContent"` calls and runs `recordFileReadState()` so `file` operations with `type:"edit"` can detect stale targets.

### Support Tools — 16

Registered from extension sources and named in `OCTOCODE_SUPPORT_TOOL_NAMES`: `file`, `web`, `chromeDebug`, `agent`, `callTool`, `skill`, `plan`, `localServer`, `MCPTool`, `askUser`, `memory`, `lock`, `message`, `readMedia`, `media`, and `runFfmpeg`. Together with the guarded `bash` override, these form the 17-tool direct palette. Every direct tool exposes only a top-level `queries[]` array; each query requires concise `reasoning`. `/configuration` is the local management page, not a model-callable support-tool alias.

| Tool | Label | Description |
|---|---|---|
| `file` | File | Create, edit, or delete files through one guarded and fully preflighted mutation boundary |
| `web` | Web | Fetch an absolute URL or run a web search |
| `chromeDebug` | Chrome DevTools | Run direct, stateful CDP operations for DOM, network, console, evaluation, navigation, and screenshots |
| `agent` | Agent | Spawn and manage researcher, planner, architect, browser, and custom worker profiles |
| `callTool` | Call Tool | Invoke a registered dynamic-capability tool from the live `<dynamic_capabilities>` registry |
| `skill` | Skill | Load installed skills or manage dynamic skill workflows with `type:"call"` |
| `plan` | Plan | Own session/shared plans, stable task projection, and observed check receipts |
| `localServer` | Local Server | Serve an inspected local static directory over loopback for user review |
| `MCPTool` | MCPTool | Call automatically discovered tools, describe one selected tool, and manage configured MCP servers |
| `askUser` | Ask User | Ask the user through an interactive picker, form, or non-TUI fallback |
| `memory` | Memory | Recall, record, review, suggest, or forget durable Awareness memory |
| `lock` | Lock | Acquire, wait for, or release exceptional exclusive file locks |
| `message` | Message | Send and read small cross-agent coordination messages when needed |
| `readMedia` | Read Media | Perceive images, video frames/contact sheets, and audio metadata/visualizations |
| `media` | Media | Author images/PDFs or transform media into path-guarded output files |
| `runFfmpeg` | Run FFmpeg | Run advanced ffmpeg/ffprobe argv with path guards, timeout, cancellation, and progress |

### Guarded Built-in Overrides — 1

Same-name `registerTool` overrides. Pi keeps the tool name; the extension owns the implementation. Named in `OVERRIDDEN_BUILTIN_TOOL_NAMES`.

| Tool | What the override adds |
|---|---|
| `bash` | Catastrophic pattern block (`rm -rf /`, `mkfs`, `dd of=/dev/`, `shutdown/reboot/halt`) · best-effort write-target extraction for redirects / `tee` / `cp`/`mv`/`install` → path guard · process output streamed to a private ephemeral log · at most approximately 4,000 model-visible head-and-tail characters plus a chunk-read reference · bounded UI reads · timeout support |

### Disabled Built-ins — 6

The branded launcher suppresses every native Pi built-in before session creation
(`noTools:"builtin"` in the SDK path, `--no-builtin-tools` in the subprocess path). For hosts
that load the extension directly, these six names are also removed from `activeTools` on load
and on `session_start`. Named in `DISABLED_BUILTIN_TOOL_NAMES`.

| Removed | Replaced by |
|---|---|
| `read` | `localGetFileContent` (records read state for `file` edit stale-check) |
| `edit` | `file` with `type:"edit"` |
| `write` | `file` with `type:"write"` |
| `grep` | `localSearch` with `operation:"text"` |
| `find` | `localSearch` with `operation:"files"` |
| `ls` | `localSearch` with `operation:"tree"` |

---

## MCP

### Built-in Octocode server

Auto-configured — no user action required.

| Field | Value |
|---|---|
| Server name | `octocode` |
| Command | Pinned local `octocode-mcp` through Node; fallback `npx -y octocode-mcp@latest` |
| NPX cache | `$OCTOCODE_HOME/extension/cache/mcp-npx` for the fallback (no `--prefer-online`) |
| Timeout | 30 s |
| Connection | **Pre-warmed at `session_start`** via `warmMcpCatalog()`; catalog injected into system prompt before turn 1 |

### User-defined servers

The harness merges active files from lowest to highest precedence. A later entry with the same server name wins.

| Precedence | Scope | Path |
|---|---|---|
| 1 | Built-in | pinned-local-first `octocode` server |
| 2 | Global | `$OCTOCODE_HOME/extension/mcp/servers.json` |
| 3 | Workspace | `$OCTOCODE_HOME/extension/workspaces/<workspace-key>/mcp/servers.json` |

Project files load only after workspace trust. `MCPTool` `action:"add"|"remove"` manages the canonical files; direct edits hot-refresh connections and artifacts, while `/new` refreshes the frozen agent prompt.

Format: `{ "mcpServers": { "<name>": { "command": "...", "args": [], "env": {}, "cwd": "...", "disabled": false, "timeoutMs": 30000 } } }`. See [`docs/TOOLS.md`](docs/TOOLS.md#mcp-servers) for the complete setup, precedence, and security contract.

### Settings and MCP slash commands

`/configuration` rebuilds local `settings.html` from Pi's live public command registry and opens the overview. The page contains skills, MCP connections/tools, enablement overrides, display controls, and prompt-state information. Section links navigate within the page. See [docs/SETTINGS.md](docs/SETTINGS.md) for persistence, security, and refresh behavior.

---

## Bundled Skills

Served via the `resources_discover` hook. Installed at `dist/skills/` inside the extension. The `octocode-awareness` package skill is intentionally **excluded** (`EXCLUDED_BUNDLED_SKILLS` in `scripts/build.mjs`) — it is not a loadable skill and bundling its `SKILL.md` causes duplicate skill-load UI noise.

| Skill | Source |
|---|---|
| `octocode-brainstorming` · `octocode-chrome-devtools` · `octocode-code-graph` · `octocode-documentation` · `octocode-eval-benchmark` · `octocode-orchestrator` · `octocode-prompt-optimizer` · `octocode-research` · `octocode-rfc-generator` · `octocode-roast` · `octocode-scraping` · `octocode-skills` · `octocode-subagent` | `octocode` package `skills/` → synced into `dist/skills/` at build time |

| `octocode-architect` | `@octocodeai/octocode-awareness` package `skills/` → synced into `dist/skills/` at build time |

Env var `OCTOCODE_SKILL_ROOT` is set to the skill root so bundled skills can locate their assets.

---

## Subagents

Spawn workers with an `agent` query whose `type` is `spawn` and whose `profile` selects the runtime. The researcher, architect, and planner profiles use standalone prompts in `subagents/<name>/SYSTEM_PROMPT.md` and curated toolsets. Browser and custom profiles are orchestrated by the same public facade.

| Profile | Specialty | Tools |
|---|---|---|
| `researcher` | Evidence gathering and compact claim ledgers | `web` · `MCPTool` (all GitHub, local, LSP, and npm research) |
| `architect` | Root-cause analysis and code archaeology | `bash` · `web` · `MCPTool` |
| `planner` | Dependency-ordered implementation plans and test strategy | `web` · `MCPTool` (read-only; no bash) |
| `browser` | Multi-turn browser analysis and lifecycle management | Browser-specific CDP orchestration |
| `custom` | Explicit model, prompt, toolset, and resource configuration | Caller-selected tools |

---

## Slash Commands

Registered via `pi.registerCommand`:

| Command | Description |
|---|---|
| `/configuration` | Open the local browser configuration page from its overview. |

The footer displays the same entry. Configuration includes MCP, skills, display,
effort, permission controls, and an explicit Review plan action. Browser plan
Start and Request changes use typed HTTP actions; feedback remains plain user text.
Other extension slash commands have been removed.

---

## Flags

Registered via `pi.registerFlag`.

| Flag | Type | Default | Effect |
|---|---|---|---|
| `--no-context` | boolean | `false` | Suppress project context files from the system prompt for this run |

---

## Lifecycle Hooks

Registered via `createHookComposer(pi, …)` (middleware composer that catches and reports errors).

| Event | Middleware ID | What it does |
|---|---|---|
| `resources_discover` | `bundled-skills` | Returns `{ skillPaths: [dist/skills/] }` so Pi discovers bundled skills |
| `session_start` | `octocode-session-start` | Resets metrics state, applies Octocode UI, starts cron scheduler, reasserts the native-tool replacement set for direct hosts, loads `.env` via `propagateOctocodeEnv` (global + project, trust-gated), notifies on env changes |
| `session_shutdown` | `octocode-session-shutdown` | Stops cron scheduler, kills spawned agents, stops MCP servers, clears all status labels and widgets |
| `model_select` | `octocode-model-select` | Logs model selection; updates UI thinking-level label |
| `thinking_level_select` | `octocode-thinking-select` | Logs thinking level; refreshes UI label |
| `input` | `octocode-session-autoname` | Names the session from the first substantive user message |
| `tool_execution_start` | `octocode-tool-error-timing` | Records tool call start time for latency tracking |
| `tool_execution_end` | `octocode-tool-error-log` | On tool error, logs structured error with latency; notifies UI |
| `before_provider_request` | `octocode-provider-error-timing` | Records provider request start time |
| `after_provider_response` | `octocode-provider-error-log` | On non-2xx status, logs provider error with latency + headers |
| `before_agent_start` | `octocode-system-prompt` | Builds the complete prompt/catalog once, then reuses byte-identical system-prompt content for the session |

### Direct `pi.on` handlers

| Event | Effect |
|---|---|
| `turn_start` | Sets `activeTurnStartedAt`, refreshes metrics UI |
| `turn_end` | Records `lastTurnMs`, increments `completedTurns`, clears active-turn marker |
| `session_shutdown` (compaction) | Clears registered context-source snapshots without reading a stale replacement context |
| `session_before_compact` | Deterministic split-turn checkpoint on the overflow path only |
| `session_compact` | Clears read state, persists a best-effort checkpoint and rehydration ledger, then emits one checkpoint card; Pi owns continuation |

### Awareness

The harness imports `@octocodeai/octocode-awareness` from the package root for automatic registry membership, shared plan projection, mutation-time lock checks and presence, plus first-class `lock`, `message`, and `memory` tools—no child process and no duplicated dispatcher. External agents use the package's `octocode-awareness` binary and share the same workspace-scoped SQLite ledger. Only unread direct-message count reaches the model automatically; global counts stay in the user dashboard. `$OCTOCODE_AWARENESS_CLI` remains available for diagnostics, recovery, and host-installed hooks. Pi coordination is prompt-owned, so the external-agent skill is not added to Pi's model-facing catalog.

Each Pi session also writes one version 2 contract across `manifest.json`, `session.json`, `plan/index.json`, `tasks/index.json`, and `backlog/index.json`, plus `memory.md` and `audit.md`, under the safe flat session root. These files expose stable session/plan/task/backlog IDs for inspection and handoff; they are projections, not a second coordination database. With `storage.mode=memory`, filesystem projections remain available and durable Awareness tools return explicit errors.

---

## UI Status Surfaces

Set via `ctx.ui.setStatus(name, value)` and `ctx.ui.setWidget(name, value)`.

| Status key | Content |
|---|---|
| `octocode` | Working message (tool name or thinking indicator) |
| `octocode-thinking` | Current thinking level badge |
| `octocode-agents` | Spawned worker count and states badge |
| `octocode-plan` | Active plan badge |
| `agent-wait` | "waiting for agent \<id\>" label during an `agent` `type:"wait"` query |
| `chrome-debug` | Active CDP action label during `chromeDebug` calls |
| `octocode-mcp` | MCP connection status label |

Metrics (turns · durations · exact current/max context), plan/task progress, Awareness attention, and a bounded live-agent list live only on the consolidated footer (`setFooter`), never in a duplicate status line or below-editor widget. The identity row contains `/configuration` (opens the settings HTML page in the browser); keyboard hints are intentionally omitted. A once-per-session, non-blocking `npx octocode auth status --json` probe adds `github ✓` in green when authenticated, `github ✗ login required` in red when credentials are missing, or `github check failed` in red on probe errors. Log in with `npx octocode auth login`. Full worker details remain available through the `agent` tool.

---

## Environment Variables

Set by the harness at load time.

| Variable | Value |
|---|---|
| `OCTOCODE_AWARENESS_CLI` | Bare path to the `octocode-awareness` bin from `@octocodeai/octocode-awareness` |
| `OCTOCODE_SKILL_ROOT` | Absolute path to `dist/skills/` |

Read from env at runtime (not set by harness):

| Variable | Purpose |
|---|---|
| `OCTOCODE_HOME` | Octocode home directory (default: `~/.octocode`) |
| `ALLOWED_PATHS` | Colon/comma-separated extra roots for path-guard (`file`/`bash`) |
| `OCTOCODE_AGENT_MAX_ACTIVE` | Cap on concurrent spawned workers |
| `ENABLE_CLONE` | Enables `ghCloneRepo` tool |
| `ENABLE_LOCAL` | Set `false` to disable all `local*` tools |
| `OCTOCODE_EDIT_NATIVE_DIFF` | Set `1` to use native Rust diff engine for large files |

---

## Asset Paths

Resolved by `getAssetPaths()` in `src/assets.ts`.

| Asset | Path |
|---|---|
| System prompt | `dist/system/SYSTEM_PROMPT.md` |
| Shared Awareness runtime | Imported from installed `@octocodeai/octocode-awareness`; its CLI subpath is invoked only for external/manual commands |
| Skills dir | `dist/skills/` |
| APPEND_SYSTEM template | `dist/system/APPEND_SYSTEM.md` |

---

## Counts at a Glance

```
 0  native research tools    (removed — served via MCPTool → octocode MCP server)
16  support tools            (see Support Tools table)
 1  guarded built-in override (bash)
 6  disabled built-ins       (read, edit, write, grep, find, ls → replaced)
1  slash commands           (live inventory and guidance via /commands)
 1  flag                     (--no-context)
12  lifecycle hooks          (hookComposer; session_start pre-warms MCP catalog)
    direct pi.on handlers    (metrics, UI, worker inbox, Awareness, and Pi-owned compaction observation)
14  bundled skills           (13 from octocode CLI + octocode-architect from awareness pkg; octocode-awareness coordination skill excluded)
 5  worker profiles          (researcher, architect, planner, browser, custom)
 1  built-in MCP server      (octocode — cache-first npx, pre-warmed at session start)
 1  composed system prompt     (shared policy + Awareness + Pi engineering delta)
```

## Token Savings

| | Per-turn `tools[]` definitions |
|---|---|
| Before | 13 native tool schemas — not prompt-cached, paid every turn |
| After | 1 (`MCPTool`) — catalog lives in `<mcp_cached_catalog>` in system prompt (prompt-cached, paid once) |
