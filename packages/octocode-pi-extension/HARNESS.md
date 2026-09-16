# `@octocodeai/pi-extension` — Full harness reference

Everything the extension registers with Pi on load: tools, system-prompt sections, MCP, subagents, skills, slash commands, flags, lifecycle hooks, and UI surfaces.

---

## System Prompt

The prompt is assembled from a small Pi host adapter, the canonical coder kernel from `src/contracts/prompts`, and the canonical `EXTERNAL_AGENT_AWARENESS_PROMPT`. The kernel owns intent classification, execution and delegation, verification, continuation, repository/tool routing, lifecycle, and output rules. Full Awareness recipes remain on demand through `guide` and the skill. The built prompt is supplied through `before_agent_start`; no regex-triggered repo-state, output-recovery, or editor-comment prompts are injected.

Before every turn, the hook refreshes a versioned effective capability snapshot
and replaces its owned prompt projection. The seven core segments contain product
policy, MCP catalog, runtime capabilities, dynamic tool contracts, available skills,
session artifacts, and `awareness-cli-runtime`. An additional `agents-protocol`
segment carries user-authority instructions from native `AGENTS.md` sources.
Unchanged content remains byte-stable; changed MCP/skill catalogs appear on the
next turn without `/new`. The active plan remains separate attributed turn context.
Runtime plan/tool results remain in the transcript; compaction adds a bounded
recovery marker. `--no-context` suppresses project context and native instruction
projection. See [capability snapshots](docs/CAPABILITIES.md#versioned-prompt-and-catalogs).

---

## Tools

### Native research tools — 0 (removed — MCP-only)

All 10 catalogued Octocode research tools (GitHub, local, graph, LSP, npm) are **not registered as native Pi tools**. They are served via the built-in `octocode` MCP server through `MCPTool`, keeping their schemas out of Pi’s direct `tools[]` array.

**Call pattern:**
```js
MCPTool({queries:[{reasoning:"Search remote code.", action:"call", server:"octocode", tool:"ghSearch",
  arguments:{queries:[{reasoning:"Find candidate files.", operation:"code", keywords:["..."]}]}}]})
```

Catalogued tools via `MCPTool server:"octocode"`: `ghSearch` · `ghGetFileContent` · `ghSearchHistory` · `ghGetHistoryItem` · `ghCloneRepo` · `artifactSearch` · `localSearch` · `astSearch` · `localFetch` · `lspSearch`. Runtime availability can disable individual tools such as cloning.

`warmMcpCatalog()` runs at `session_start`. The prompt receives one deterministic `<mcp_catalog_index>` with enabled server instructions plus tool names and descriptions; input schemas are not injected. The initial index is bounded and exposes an executable `MCPTool action:"list"` continuation when needed. `action:"describe"` returns the exact JSON schema and, when the host admits dynamic names, activates a namespaced Pi proxy whose provider-visible parameters are that schema. Call the returned proxy directly; a fixed host allowlist is reported and uses the generic gateway fallback. Generic gateway calls are blocked until describe and are bound to the described schema digest. Calls also validate against the current enabled `catalog.json` snapshot. The gateway's own `queries[]` schema uses strict action-discriminated branches, so unrelated fields are rejected before execution.

**Edit stale-check**: `MCPTool` intercepts `server:"octocode" tool:"localFetch"` calls and runs `recordFileReadState()` so `file` operations with `type:"edit"` can detect stale targets.

### Support Tools — 14

Registered from extension sources and named in `OCTOCODE_SUPPORT_TOOL_NAMES`: `file`, `web`, `chromeDebug`, `agent`, `callTool`, `skill`, `plan`, `localServer`, `awareness`, `MCPTool`, `askUser`, `inspectMedia`, `media`, and `runFfmpeg`. Together with the guarded `bash` override, these form the 15-tool direct palette. Every direct tool exposes only a top-level `queries[]` array; each query requires concise `reasoning`. `/config` and its `/configuration` alias open the OS browser management page.

| Tool | Label | Description |
|---|---|---|
| `file` | File | Create, edit, or delete files through one guarded and fully preflighted mutation boundary |
| `web` | Web | Fetch an absolute URL or run a web search |
| `chromeDebug` | Chrome DevTools | Run direct, stateful CDP operations for DOM, network, console, evaluation, navigation, and screenshots |
| `agent` | Agent | Spawn and manage researcher, planner, architect, implementer, read-only reviewer, browser, and explicit custom worker profiles |
| `callTool` | Call Tool | Invoke a registered dynamic-capability tool from the live `<dynamic_capabilities>` registry |
| `skill` | Skill | Load installed skills or manage dynamic skill workflows with `type:"call"` |
| `plan` | Plan | Own session/shared plans, stable task projection, and observed check receipts |
| `localServer` | Local Server | Serve an inspected local static directory over loopback for user review |
| `awareness` | Awareness | List, describe, and execute canonical Awareness commands with host-injected identity and storage bindings |
| `MCPTool` | MCPTool | Call automatically discovered tools, describe one selected tool, and manage configured MCP servers |
| `askUser` | Ask User | Request input through an interactive picker, form, or non-TUI fallback |
| `inspectMedia` | Inspect Media | Perceive images, video frames/contact sheets, and audio metadata/visualizations |
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
| `read` | `localFetch` (records read state for `file` edit stale-check) |
| `edit` | `file` with `type:"edit"` |
| `write` | `file` with `type:"write"` |
| `grep` | `localSearch` with `searchText` |
| `find` | `astSearch` with `operation:"files"` |
| `ls` | `astSearch` with `operation:"tree"` |

---

## MCP

### Built-in Octocode server

Auto-configured — no user action required.

| Field | Value |
|---|---|
| Server name | `octocode` |
| Command | Package-local `octocode-mcp` through Node; fallback version range owned by `mcp/config.ts` |
| NPX cache | `$OCTOCODE_HOME/extension/cache/mcp-npx` for the fallback (no `--prefer-online`) |
| Timeout | 30 s |
| Connection | **Pre-warmed at `session_start`** via `warmMcpCatalog()`; catalog injected into system prompt before turn 1 |

### User-defined servers

The harness merges native active files from lowest to highest precedence. A later entry with the same server name wins.

| Precedence | Scope | Path |
|---|---|---|
| 1 | Built-in | pinned-local-first `octocode` server |
| 2 | Global | Retained private files, then `$OCTOCODE_HOME/mcp.json` |
| 3 | Workspace | Repository ancestors toward the current directory: private files, then `.agents/mcp.json` |

Project files load only after workspace trust. `MCPTool` `action:"add"|"remove"` manages `.agents/mcp.json` or `$OCTOCODE_HOME/mcp.json`. Direct edits refresh connections and artifacts; the next turn projects the effective catalog. Pi defaults and `PI_CODING_AGENT_DIR` apply only to models. Foreign Claude, Codex, Cursor, and other recognized sources remain disabled until their source ID and exact definition revision are reviewed and linked. Changed links require review again; removed sources remain unavailable. See [source paths and review](docs/CAPABILITIES.md#source-paths).

The native JSON container is `mcpServers`. Definitions support stdio or HTTP, environment/header references, authentication, optional instructions, timeouts, and tool filters. [MCP configuration](src/tools/mcp/config.ts) owns validation and serialization; unsupported foreign fields remain explicit diagnostics.

### Settings and MCP slash commands

`/config` and `/configuration` rebuild local `settings.html` from Pi's live public command registry and open the OS browser. The page contains source review, skills, MCP connections/tools, models, hooks, worker grants, enablement, display controls, and prompt state. See [docs/SETTINGS.md](docs/SETTINGS.md) for persistence, security, and refresh behavior.

---

## Bundled Skills

Served via the `resources_discover` hook and installed at `dist/skills/`. Canonical
discovery in `src/tools/skill-discovery.ts` resolves one effective skill per name
from valid Pi metadata, native roots, reviewed foreign links, and the bundle.
Bundled names win unless an exact source is explicitly selected. Ordinary workspace
definitions outrank global ones; the nearest repository ancestor wins. Recursive
and linked directories retain source identity and change diagnostics. The bundled
`octocode-awareness` skill is loadable and owns detailed operating guidance. See the
[15-skill inventory](README.md#bundled-skills-15) for the complete enabled bundle.

| Skill | Source |
|---|---|
| `octocode-awareness` | Canonical `@octocodeai/octocode-awareness` package skill |
| Other bundled skills | Build-managed sources; see the README inventory |

Env var `OCTOCODE_SKILL_ROOT` is set to the skill root so bundled skills can locate their assets.

---

## Subagents

Spawn workers with an `agent` query whose `type` is `spawn` and whose `profile` selects the runtime. Typed profiles use standalone prompts in `subagents/<name>/SYSTEM_PROMPT.md` and curated least-capability toolsets. Every spawn requires goal, context, scope, ownership, acceptance, and returnShape; incomplete packets fail before process creation.

| Profile | Specialty | Tools |
|---|---|---|
| `researcher` | Evidence gathering and compact claim ledgers | `web` · `MCPTool` · `file` · `skill` · `awareness` · `bash` (role-scoped) |
| `architect` | Root-cause analysis and code archaeology | `web` · `MCPTool` · `file` · `skill` · `awareness` · `bash` (bounded test/build/debug checks) |
| `planner` | Dependency-ordered implementation plans and test strategy | `web` · `MCPTool` · `file` · `skill` · `awareness` · `bash` (role-scoped) |
| `implementer` | One bounded implementation unit with exclusive ownership and an observed check | `MCPTool` · `file` · `skill` · `awareness` · `bash` |
| `reviewer` | Independent, read-only acceptance review with a typed PASS/WARN/FAIL verdict | `MCPTool` · `skill` · `awareness` |
| `browser` | Multi-turn browser analysis and lifecycle management | `chromeDebug` · `MCPTool` · `skill` · `awareness` · `bash` |
| `custom` | Caller-defined bounded role | Explicit caller-provided `tools` allowlist and non-empty `systemPrompt` |

Researcher, architect, and planner workers keep product-code investigation read-only. Their `file` capability
is limited by role policy to parent-assigned RFC or durable handback artifacts;
it does not authorize product edits. `skill` loads operating guidance. Every typed
worker uses native `awareness` for scoped communication and bookkeeping;
`bash` does not replace MCP research. Architect additionally permits bounded
non-destructive test, build and debug checks. Typed profiles use the current
`file` and `skill` names rather than removed standalone memory/write wrappers.
Browser workers load the Octocode extension and Awareness bindings. Every worker
receives a versioned grant limited to the parent's enabled native tools, exact
skill source IDs, and MCP server/tool pairs. Parent `agent type:"configure"`
replaces selected arrays using the current `snapshotRevision` and optional
`grantRevision`: removals apply immediately, additions before the next worker turn.
MCP calls use the parent broker, and worker skill/prompt inventories use the same
grant. Workers request missing access from the parent. Custom workers receive the
shared bounded-worker contract before their caller role prompt. Explicit `tools:[]`
maps to `--no-tools`; lean mode disables extension and skill loading.

---

## Slash Commands

Registered via `pi.registerCommand`:

| Command | Description |
|---|---|
| `/config` | Open configuration in the OS browser. |
| `/octocode-rewind` | Select bounded local history, preview file changes, and explicitly apply the reviewed preview. |
| `/octocode-inbox` | Pick a spawned worker, then view its transcript, steer it, stop it, or dismiss the overlay. |
| `/octocode-status` | Inspect usage, tools, skills, plan, workers, and pending decisions in a scrollable terminal view. |
| `/octocode-status events` | Inspect the selected branch’s semantic JSONL journal; `export` writes a session artifact. |
| `/configuration` | Alias for `/config`. |

The footer displays `/config`. Configuration includes MCP, skills, models, hooks,
worker grants, display, effort, permission controls, and Review plan. Browser plan
Start and Request changes use typed HTTP actions; feedback remains plain user text.
Other workflow actions remain in the native tools, including `awareness`.

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
| `session_start` | `octocode-session-start` | Resets metrics state, applies Octocode UI, claims Pi's native Awareness lifecycle ownership, reasserts the native-tool replacement set for direct hosts, loads `.env` via `propagateOctocodeEnv` (global + project, trust-gated), notifies on env changes |
| `session_shutdown` | `octocode-session-shutdown` | Kills spawned agents, stops MCP servers, and clears all status labels and widgets |
| `model_select` | `octocode-model-select` | Logs model selection; updates UI thinking-level label |
| `thinking_level_select` | `octocode-thinking-select` | Logs thinking level; refreshes UI label |
| `input` | `octocode-session-autoname` | Names the session from the first substantive user message |
| `tool_execution_start` | `octocode-tool-error-timing` | Records tool call start time for latency tracking |
| `tool_execution_end` | `octocode-tool-error-log` | On tool error, logs structured error with latency; notifies UI |
| `before_provider_request` | `octocode-provider-error-timing` | Records provider request start time |
| `after_provider_response` | `octocode-provider-error-log` | On non-2xx status, logs provider error with latency + headers |
| `before_agent_start` | `octocode-system-prompt` | Refreshes effective capabilities and projects the current version into the prompt; unchanged content remains byte-identical |

The [model and declarative hook adapters](docs/CAPABILITIES.md) initialize after
environment propagation and refresh before turns. Models merge into Pi's registry
while preserving authentication and unrelated providers. Hook discovery remains
inert; reviewed command definitions execute through bounded lifecycle handlers
with cancellation, timeout, and output limits. Native Pi extension handlers retain
their host ownership.

### Direct `pi.on` handlers

| Event | Effect |
|---|---|
| `turn_start` | Sets `activeTurnStartedAt`, refreshes metrics UI |
| `turn_end` | Records `lastTurnMs`, increments `completedTurns`, clears active-turn marker |
| `session_shutdown` (compaction) | Clears registered context-source snapshots without reading a stale replacement context |
| `session_before_compact` | Deterministic split-turn checkpoint on the overflow path only |
| `session_compact` | Clears read state, persists a best-effort checkpoint and rehydration ledger, then emits one checkpoint card; Pi owns continuation |

### Awareness

The harness imports `@octocodeai/octocode-awareness` for native registry membership,
shared plan projection, mutation guards/presence and peer-event delivery/policy.
Model-facing signals, explicit locks, memory, verification, history, and bookkeeping
use the single `awareness` list/describe/call facade, importing the package API directly. Operator retention and whole-store retirement remain outside routine Pi discovery. Internal hook callbacks stay with the host lifecycle; setup and instruction export use the native API. The stable Awareness runtime segment supplies the physical SQLite path, normalized workspace, and identity; native execution needs no CLI runner.
External CLI agents communicate through that same database/workspace with their own
distinct IDs. Workers keep their physical worktree as workspace, while the inherited
`OCTOCODE_AWARENESS_DB` selects the parent ledger for native calls, CLI fallback,
guards, registry and delivery; no database copies are made. Native run/task IDs and
receipts are reused. Pi does not install shell
hooks; its native events remain the lifecycle owner. See [agent flow](docs/AWARENESS_AGENT_FLOW.md).

Each Pi session also writes one version 2 contract across `manifest.json`, `session.json`, `plan/index.json`, `tasks/index.json`, and `backlog/index.json`, plus `memory.md` and `audit.md`, under the safe flat session root. These files expose stable session/plan/task/backlog IDs for inspection and handoff; they are projections, not a second coordination database. With `storage.mode=memory`, filesystem projections remain available, durable CLI bindings are omitted and the prompt directs agents to session state.

---

## UI status surfaces

Set via `ctx.ui.setStatus(name, value)` and `ctx.ui.setWidget(name, value)`.

| Status key | Content |
|---|---|
| `octocode` | Static Octocode identity |
| `octocode-thinking` | Current thinking level badge |
| `agent-wait` | "waiting for agent \<id\>" label during an `agent` `type:"wait"` query |
| `chrome-debug` | Active CDP action label during `chromeDebug` calls |
| `octocode-mcp` | MCP connection status label |

The register-once footer owns activity, exact measured context, plan progress, worker attention, and session metadata. Pending decisions stop the motion indicator. The event reducer owns turn timing and tool counts; initialization and provider context remain separate runtime facts. Git line totals describe the whole working tree, including changes that preceded the session. GitHub authentication problems appear as attention; successful checks stay quiet.

`lifecycle-ui.ts` records structured host observations through `execution-runtime.ts` into Pi custom state entries. `execution-events.ts` owns their typed payloads and replay reducer. These entries never enter model context. Pi retains user/assistant text and full tool results; the event journal references those native records instead of copying private reasoning or large output. `/octocode-status` and `/octocode-status events` inspect this state without sending an assistant message. See [UI contract](docs/UI.md).

---

## Environment Variables

CLI fallback bindings supplied by the harness. Native Awareness calls use structured context.

| Variable | Value |
|---|---|
| `OCTOCODE_AWARENESS_CLI` | Installed CLI path for external/foreign-tool fallback; native Awareness calls do not use it |
| `OCTOCODE_NODE` | Node executable for the installed CLI |
| `OCTOCODE_AWARENESS_DB` | Canonical inherited Awareness database for native/worker calls and external/foreign-tool CLI adapters |
| `OCTOCODE_AWARENESS_WORKSPACE` | Normalized workspace available to external/foreign-tool CLI adapters |
| `OCTOCODE_AGENT_ID` | Current participant identity available to external/foreign-tool CLI adapters |
| `OCTOCODE_SKILL_ROOT` | Absolute path to `dist/skills/` |

Read from env at runtime (not set by harness):

| Variable | Purpose |
|---|---|
| `OCTOCODE_HOME` | Octocode home directory (default: `~/.octocode`) |
| `PI_CODING_AGENT_DIR` | Pi model source directory (default: `~/.pi/agent`); Octocode uses Pi defaults only for models |
| `CODEX_HOME` | Codex sources for disabled foreign discovery (default: `~/.codex`) |
| `ALLOWED_PATHS` | Colon/comma-separated extra roots for path-guard (`file`/`bash`) |
| `OCTOCODE_AGENT_MAX_ACTIVE` | Cap on concurrent spawned workers |
| `ENABLE_CLONE` | Enables `ghCloneRepo` tool |
| `ENABLE_LOCAL` | Set `false` to disable all `local*` tools |

---

## Conformance testing entrypoint

Production Pi SDK probes are test-only and are not published with the package. The
probe corpus lives in the test corpus, not in `src/`, so it never reaches `dist`:

```ts
import {
  createProductionPiScenarioSuite,
  captureProductionPiLifecycle,
} from './helpers/production-pi.js';
```

The `@octocodeai/pi-extension` package exposes only `.` (runtime composition) and
`./package.json`; there is no published conformance subpath. `tests/factory.test.ts`
pins this by asserting the probe files are absent from `src/`.

---

## Asset Paths

Resolved by `getAssetPaths()` in `src/assets.ts`.

| Asset | Path |
|---|---|
| System prompt | `dist/system/SYSTEM_PROMPT.md` |
| Shared Awareness runtime | Installed `@octocodeai/octocode-awareness`; native lifecycle imports and the `awareness` facade share its ledger; the CLI remains the external-host fallback |
| Skills dir | `dist/skills/` |
| APPEND_SYSTEM template | `dist/system/APPEND_SYSTEM.md` |

---

## Counts at a Glance

```
 0  native research tools    (removed — served via MCPTool → octocode MCP server)
14  support tools            (see Support Tools table)
 1  guarded built-in override (bash)
 6  disabled built-ins       (read, edit, write, grep, find, ls → replaced)
5  slash commands           (config, configuration alias, recovery, worker inbox, and status)
 1  flag                     (--no-context)
    lifecycle hooks          (hookComposer plus reviewed declarative event adapters)
    direct pi.on handlers    (metrics, UI, worker inbox, Awareness, and Pi-owned compaction observation)
15  bundled skills           (including the canonical octocode-awareness skill)
 7  worker profiles          (researcher, architect, planner, implementer, reviewer, browser, custom)
 1  built-in MCP server      (octocode — cache-first npx, pre-warmed at session start)
 1  composed system prompt     (host facts + canonical coder kernel + Awareness guide + runtime bindings)
```

## Catalog delivery

| Surface | Delivery |
|---|---|
| Direct Pi palette | 14 support schemas plus guarded `bash` |
| MCP routing | Bounded index, revision-bound list pages, exact describe on demand |
| Skills | Bounded effective inventory, list pages, selected instruction load |
| Updates | A changed effective revision is projected before the next turn |
