# `@octocodeai/pi-extension` — Full Harness Reference

Everything the extension registers with Pi on load: tools, system-prompt sections, MCP, subagents, skills, slash commands, flags, lifecycle hooks, and UI surfaces.

---

## System Prompt

Authored as a stable eight-section decision kernel in `src/prompts/prompt.ts`, built into `dist/system/SYSTEM_PROMPT.md`, and injected via the `before_agent_start` hook: `<authority>` · `<operating_model>` · `<judgment>` · `<repository>` · `<awareness>` · `<code_quality>` · `<capability_routing>` · `<output>`. The kernel owns cross-task decisions; live tool/MCP/skill catalogs, plan mode, and typed-role prompts own operational detail. The concept-level contract lives in `tests/prompt-contract.test.ts`.

On the first main-agent turn, the hook assembles either the eager `<mcp_catalog>` or lazy `<mcp_catalog_index>`, `<dynamic_capabilities>` (dynamic `callTool` and `skill` `type:"call"` registries), the complete available-skills projection, and the initial `<active_plan>`, then freezes those exact system-prompt bytes for the session. Runtime plan/tool results remain in transcript context; compaction adds a bounded `<octocode_compaction_context>` marker with the native summary and active-plan pointer. With `--no-context` set, the hook suppresses project context before freezing the prompt.

---

## Tools

### Native Research Tools — 0 (removed — MCP-only)

All 13 Octocode research tools (GitHub, local, graph, LSP, npm) are **not registered as native Pi tools**. They are served via the built-in `octocode` MCP server through `MCPTool`, keeping their schemas out of Pi’s direct `tools[]` array.

**Call pattern:**
```js
MCPTool({queries:[{reasoning:"Search remote code.", action:"call", server:"octocode", tool:"ghSearch",
  arguments:{queries:[{reasoning:"Find candidate files.", operation:"code", keywords:["..."]}]}}]})
```

Available tools via `MCPTool server:"octocode"`: `ghSearch` · `ghGetFileContent` · `ghSearchPullRequests` · `ghSearchIssues` · `ghSearchCommits` · `ghListReleases` · `ghSearchDiscussions` · `ghCloneRepo` · `npmSearch` · `localSearch` · `localGetFileContent` · `localAnalyzeGraph` · `lspGetSemantics`

`warmMcpCatalog()` runs at `session_start`. The default first-turn prompt consumes the concise deterministic `mcp.md` guide while exact schemas stay private for validation. Set `OCTOCODE_COMPACT_MCP=0` only to inject the exact catalog for debugging; set `OCTOCODE_MCP_AI_GUIDE=1` to opt into model-authored guide generation. Calls always validate against the exact private catalog, so no prepare round trip is required.

**Edit stale-check**: `MCPTool` intercepts `server:"octocode" tool:"localGetFileContent"` calls and runs `recordFileReadState()` so `file` operations with `type:"edit"` can detect stale targets.

### Support Tools — 16

Registered from extension sources and named in `OCTOCODE_SUPPORT_TOOL_NAMES`: `file`, `web`, `chromeDebug`, `agent`, `callTool`, `skill`, `plan`, `localServer`, `MCPTool`, `askUser`, `memory`, `lock`, `message`, `readMedia`, `media`, and `runFfmpeg`. Together with the guarded `bash` override, these form the 17-tool direct palette. Every direct tool exposes only a top-level `queries[]` array; each query requires concise `reasoning`. `/mcp` is the local management page, not a model-callable support-tool alias.

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
| `bash` | Catastrophic pattern block (`rm -rf /`, `mkfs`, `dd of=/dev/`, `shutdown/reboot/halt`) · best-effort write-target extraction for redirects / `tee` / `cp`/`mv`/`install` → path guard · lossless output paging into ≤20,000-character model content blocks · bounded head/tail UI previews · timeout support |

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

`/settings` rebuilds local `settings.html` from Pi's live public command registry and opens `#skills` by default. The same page contains every command, discovered skill, MCP connection/tool, redacted configuration, enablement override, and prompt-state artifact; section completions jump directly to any panel. `/mcp` opens the focused connections panel. See [docs/SETTINGS.md](docs/SETTINGS.md) for the complete feature, persistence, security, and refresh contract.

---

## Bundled Skills

Served via the `resources_discover` hook. Installed at `dist/skills/` inside the extension. The `octocode-awareness` package skill is intentionally **excluded** (`EXCLUDED_BUNDLED_SKILLS` in `scripts/build.mjs`) — it is not a loadable skill and bundling its `SKILL.md` causes duplicate skill-load UI noise.

| Skill | Source |
|---|---|
| `octocode-brainstorming` · `octocode-chrome-devtools` · `octocode-documentation` · `octocode-graph-eval` · `octocode-orchestrator` · `octocode-prompt-optimizer` · `octocode-research` · `octocode-rfc-generator` · `octocode-roast` · `octocode-scraping` · `octocode-skills` · `octocode-subagent` | `@octocodeai/octocode` package `skills/` → synced into `dist/skills/` at build time |

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

Registered via `pi.registerCommand`. All commands support tab-completion where noted.

| Command | Alias | Description |
|---|---|---|
| `/commands` | — | Live, grouped inventory of all public Pi, extension, prompt, and skill commands with when-to-use guidance and GitHub login help |
| `/octocode` | — | Dashboard: status, agents, setup, skills, health warnings, next actions |
| `/octocode-harness` | — | Full harness surface listing (native tools, support, overrides, commands, skills) |
| `/octocode-now` | — | Current working state snapshot |
| `/octocode-tasks` | — | Awareness task list |
| `/octocode-skills` | — | Skill catalog and readiness |
| `/octocode-agents [help\|list\|status\|inspect\|kill\|kill-all\|prune\|hide]` | — | Show, inspect, prune, hide, or kill spawned worker agents |
| `/octocode-cron [list\|check\|cancel\|help]` | — | List, check, or cancel session jobs |
| `/settings [commands\|skills\|connections\|add-server\|sources\|agent-context\|overrides]` | — | Rebuild the complete control center from the live command registry; defaults to `settings.html#skills` |
| `/mcp` | — | Open the focused MCP connections panel |
| `/octocode-setup [project\|global]` | — | Install the `APPEND_SYSTEM.md` block into `.pi/` or `~/.pi/agent/` |
| `/octocode-skills-update` | — | Update the Pi package then reload Pi resources (interactive only) |
| `/octocode-plan` | — | Show/manage the active plan or enter plan mode |
| `/octocode-theme` | — | Switch/apply the Octocode theme |
| `/octocode-chrome` | — | Chrome/CDP connection status |
| `/octocode-footer` | — | Change footer density or show the segment legend |
| `/octocode-permissions` | — | Inspect/change session approval controls |
| `/octocode-profile` | — | Apply a named profile to the live session |
| `/octocode-inbox` | — | Inspect, steer, or kill spawned workers from the inbox |
| `/octocode-palette` | — | Interactive command/action picker |
| `/octocode-rewind` | — | Restore automatic pre-prompt checkpoints |
| `/octocode-dial` | — | Adjust thinking level and worker parallelism together |
| `/octocode-watch` | — | Turn editor comments ending in `AI!` into prompts |
| `/octocode-export` | — | Apply Octocode branding to a Pi HTML export |

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
| `input` | `octocode-repo-state-hint` | Appends a one-line repo-state hint (branch, dirty files) to any user message matching repo/git keywords |
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

Metrics (turns · durations · context %) live ONLY on the consolidated footer (`setFooter`), not a status line. The identity row contains `/settings` (opens the settings HTML page in the browser); keyboard hints are intentionally omitted. A once-per-session, non-blocking `npx octocode auth status --json` probe adds `github ✓` in green when authenticated, `github ✗ login required` in red when credentials are missing, or `github check failed` in red on probe errors. `/commands` shows `npx octocode auth login` and `gh auth login` guidance without retaining or displaying token values. The unified below-editor widget is `octocode-status-panel` (complete Plan checklist → Awareness); model/context remain in Pi/footer chrome and agents remain in uncapped footer rows.
| `octocode-agents` (widget) | Rich agent panel with per-worker state, timestamps, and preview |

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
24  slash commands           (live inventory and guidance via /commands)
 1  flag                     (--no-context)
12  lifecycle hooks          (hookComposer; session_start pre-warms MCP catalog)
    direct pi.on handlers    (metrics, UI, worker inbox, Awareness, and Pi-owned compaction observation)
11  bundled skills           (octocode CLI skill set; awareness excluded)
 5  worker profiles          (researcher, architect, planner, browser, custom)
 1  built-in MCP server      (octocode — cache-first npx, pre-warmed at session start)
 8  stable system-prompt sections
```

## Token Savings

| | Per-turn `tools[]` definitions |
|---|---|
| Before | 13 native tool schemas — not prompt-cached, paid every turn |
| After | 1 (`MCPTool`) — catalog lives in `<mcp_cached_catalog>` in system prompt (prompt-cached, paid once) |
