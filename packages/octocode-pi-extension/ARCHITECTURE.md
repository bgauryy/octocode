# @octocodeai/pi-extension — Architecture

This document describes the Octocode Pi Extension (`packages/octocode-pi-extension`): system prompt assembly, tool registration, skill discovery, session data layout, plan lifecycle, and the HTML/Markdown plan surface. Source contracts remain authoritative. Capability, prompt, worker, and discovery sections checked 2026-09-10.

---

## 1. Ownership

| Area | Source contract |
|---|---|
| Main-agent policy | `@octocodeai/agent-contracts/prompts` owns the canonical coder kernel; [`src/prompts/system-prompt.ts`](src/prompts/system-prompt.ts) adds Pi host facts and canonical Awareness guidance |
| Awareness host bindings | [`src/tools/awareness-cli-context.ts`](src/tools/awareness-cli-context.ts): native-facade prompt context; foreign tool sets can receive a bound CLI fallback. [`src/tools/awareness-context.ts`](src/tools/awareness-context.ts) owns native database/workspace/identity bindings |
| Runtime physiology | [`src/adapters/pi-physiology.ts`](src/adapters/pi-physiology.ts): headless native measurements and session fences; [`src/adapters/pi-physiology-regulation.ts`](src/adapters/pi-physiology-regulation.ts): bounded projection of canonical Awareness advice |
| Context assembly and lifecycle | [`src/index.ts`](src/index.ts), [`src/tools/session-prompt-context.ts`](src/tools/session-prompt-context.ts), and [`src/tools/context-segments.ts`](src/tools/context-segments.ts) |
| Direct tool names | [`src/constants.ts`](src/constants.ts); registration in `registerSupportToolPhase` |
| Query execution and partial receipts | [`src/tools/query-envelope.ts`](src/tools/query-envelope.ts) owns ordering, preflight, concurrency, and cancellation. [`src/tools/query-batch-error.ts`](src/tools/query-batch-error.ts) preserves completed evidence in bounded thrown errors, with artifact references for oversized text and images. Registration preserves Pi's thrown-error failure contract. Operation-specific validation stays with each tool. |
| Host tool failure adaptation | [`src/tools/tool-result-error.ts`](src/tools/tool-result-error.ts) converts internal error results into bounded thrown errors at registration. Success result shapes remain unchanged; original error diagnostics stay available on the exception. |
| File mutations | [`docs/FILE_MUTATIONS.md`](docs/FILE_MUTATIONS.md) defines the dedicated `@octocodeai/octocode-extension-rust` boundary: native snapshots, mutations, sync and diff; TypeScript owns text semantics, path policy, batches and receipts. |
| MCP discovery and execution | [`src/tools/mcp-tool.ts`](src/tools/mcp-tool.ts) and [`src/tools/mcp/config.ts`](src/tools/mcp/config.ts) |
| Skill discovery | [`src/tools/skill-discovery.ts`](src/tools/skill-discovery.ts); the `skill` tool consumes that inventory from [`src/tools/skill-tool.ts`](src/tools/skill-tool.ts) |
| Worker spawning and waits | [`src/tools/agents/tool.ts`](src/tools/agents/tool.ts), [`src/tools/agents/lifecycle.ts`](src/tools/agents/lifecycle.ts), and [`src/tools/agents/wait.ts`](src/tools/agents/wait.ts) |
| Capability sources and review | `@octocodeai/agent-contracts/capability-sources`, `agent-skills`, and `capability-state`; Pi applies host trust and enablement through the skill/MCP adapters |
| Model and command-hook adapters | [`src/adapters/pi-capability-adapters.ts`](src/adapters/pi-capability-adapters.ts) owns initialization, refresh, and disposal; [capability reference](docs/CAPABILITIES.md) owns source and execution contracts |
| Effective snapshots and worker grants | [`src/tools/capability-session.ts`](src/tools/capability-session.ts), [`src/tools/worker-capabilities.ts`](src/tools/worker-capabilities.ts), and [`src/tools/mcp/broker.ts`](src/tools/mcp/broker.ts); shared Zod schemas live in `@octocodeai/agent-contracts/capabilities` |
| Pi retained-context evidence | [`src/adapters/pi-retained-context.ts`](src/adapters/pi-retained-context.ts) |
| Execution journal and replay | [`src/tools/execution-events.ts`](src/tools/execution-events.ts) owns typed events and the reducer; [`src/tools/execution-runtime.ts`](src/tools/execution-runtime.ts) binds Pi branch persistence; [`src/tools/lifecycle-ui.ts`](src/tools/lifecycle-ui.ts) observes host events |
| Plan projection | [`src/tools/plan-read-model.ts`](src/tools/plan-read-model.ts) |

This reference does not assign quality grades or claim token savings without a measured baseline.

---

## 2. System prompt assembly

### 2.1 Composition

The extension adds a short `<octocode_host>` capability and trust-boundary adapter, then composes the canonical coder kernel from `@octocodeai/agent-contracts/prompts` with `EXTERNAL_AGENT_AWARENESS_PROMPT`. The root process receives intent classification, execution/delegation, verification, continuation, tool routing, and output rules. Workers receive host/interaction safety plus their bounded shared and role contracts, never root user-facing authority. Live catalogs and selected skills retain operational detail; no regex-triggered repository instruction is injected.

Source: `src/prompts/system-prompt.ts` → `SYSTEM_PROMPT`.
Bundled artifact: `dist/system/SYSTEM_PROMPT.md`.

### 2.2 Versioned capability projection and live turn context

Before every main-agent or worker turn, `preparePromptCapabilities` resolves the
capabilities allowed by that process and publishes a validated snapshot with
`schemaVersion: 1` and a content revision. `assembleSessionPromptContext` attributes
and bounds the segments. The hook replaces only its prior owned projection,
preserving the host's current prompt. Unchanged content remains byte-stable;
changed capabilities appear on the next turn without a new session.

The reported context status is `ready` when this projection is assembled; it is
not a provider cache-hit receipt. Estimates count the active direct tools and
prompt/turn overhead, separately from Pi's retained conversation measurements.
MCP discovery revalidates configuration before publishing, and coalesces changes
behind in-flight discovery. Skill execution receives raw Pi metadata, never a
previously resolved inventory that could introduce a second source identity.

| Segment key | Content | Budget |
|---|---|---|
| `octocode-product-policy` | Bundled `SYSTEM_PROMPT.md` | 20k tokens |
| `awareness-cli-runtime` | Native Awareness routing and current host bindings; CLI fallback only for tool sets without the native facade | 2k tokens |
| `mcp-tool-contracts` | `<mcp_catalog_index>` (schema-aware, default) or `<mcp_catalog>` (unoptimized, `OCTOCODE_COMPACT_MCP=0`) | 30k tokens |
| `runtime-tool-contracts` | `<runtime_capabilities>` and current `capability_revision` | 10k tokens |
| `dynamic-tool-contracts` | Dynamic skill addendum (excludes installed skill names already in catalog) | 20k tokens |
| `available-skills` | `<available_skills>` — discovered skill list | 20k tokens |
| `session-artifact-contract` | Session memory and audit paths | 1k tokens |
| `agents-protocol` | Exact native `AGENTS.md` content, attributed as user-authority project instructions | 20k tokens |

Workers receive only segments supported by their grant of native tools, exact
skill IDs, and MCP server/tool pairs. Their role prompt remains caller-owned;
MCP/skill catalogs and host bindings use the shared attribution and token budgets.
Native project instruction discovery is suppressed for workers, whose task context
comes from the explicit worker packet.

The active plan is a separate attributed turn-context segment, budgeted at 15k
tokens. After compaction or resume, its current full content is delivered when
absent from retained context, even if unchanged. This allowance is separate from
the 8k recovery budget for other segments; retained or freshly delivered plans
are validated without consuming that recovery budget. Runtime physiology is another turn segment, limited to 128 estimated
tokens and never rehydrated as current state. It carries changed advisory actions
from fresh host receipts; unavailable sensors do not establish recovery. The
shared Zod observation contract belongs to `agent-contracts`, thresholds belong to
Awareness, and actual compaction/retry control remains with Pi. The observer uses
the same hook composer as output-budget and prompt middleware, retains only a
bounded numeric tool-outcome window, and exposes `readPiPhysiology(ctx)` to trusted
integrations. It adds no model-facing tool or shared SQLite state.

The plan hook recomputes its projection every turn and delivers it when first available,
changed, or cleared. Session memory is delivered initially and registered as a
current recovery source. Compaction recovery validates current sources; it does
not copy the owned policy segments into the recovery ledger. During recovery,
`collectPiRetainedContentDigests` delegates branch and compaction interpretation to
Pi's public `buildSessionContext` adapter. Reprojection is skipped only for exact
content that Pi retained, including validated Octocode segment entries; full session
history is never treated as retained model context.

The turn hook checks `hasPendingRehydration` before preparing recovery sources.
Main and worker sessions consume pending recovery on the first resumed turn as
well as later turns. Workers recover their own session sources without receiving
main-agent plan state; their role prompt and tool allowlist remain unchanged.
Ordinary turns refresh capability projections but do not scan retained context
or merge recovery sources without pending recovery. The recovery flag remains session-scoped and is consumed once;
pending approvals, current plan revision, source identity, and digests still validate
before any recovered content reaches the model.

Recovery also carries a bounded user-request history from the current branch,
including the original request and recent amendments. This history explicitly
preserves completion and supersession; it does not create a new work queue.
Recovery is committed only after prompt assembly succeeds, so a budget failure
can retry it. Compaction leaves Pi's current tool selection intact.

Execution also validates current enablement at use time. A changed MCP or skill
source updates the next prompt projection. Bounded MCP list → describe and skill
list → load flows expose exact metadata with revision-bound continuations; a
catalog change restarts paging instead of mixing revisions.

### 2.3 Sub-prompts

| File | Content |
|---|---|
| `src/prompts/system-prompt.ts` | Pi host adapter plus root/worker selection over the shared canonical prompt builders |
| `src/prompts/plan-prompt.ts` | Thin Pi call-syntax adapter over the shared atomic-Start plan prompt |
| `@octocodeai/agent-contracts/prompts` | Local owner: `packages/octocode-agent-contracts/src/prompts/`. Pi build selects `coordination:"worker-only"`; runtime injects the short canonical Awareness standing prompt once; the full guide stays on demand, preserving shared worker restrictions without parallel ledger instructions |

---

## 3. Tool registration

### 3.1 Pi builtin disposition

```ts
// src/constants.ts
DISABLED_BUILTIN_TOOL_NAMES = ['read', 'edit', 'write', 'grep', 'find', 'ls']
// Replaced by MCPTool → octocode-mcp (localFetch, localSearch)

OVERRIDDEN_BUILTIN_TOOL_NAMES = ['bash']
// Octocode owns the implementation (path guard, write-target guard)
```

### 3.2 Direct Pi tools (15 including the `bash` override)

Registered in `registerSupportToolPhase` in [`src/index.ts`](src/index.ts):

| Tool | Source file | Purpose |
|---|---|---|
| `file` | `file-tool.ts` | Guarded file mutations (edit/write/delete) |
| `bash` | `bash-tool.ts` | Shell tasks (overrides Pi weak builtin) |
| `inspectMedia` | `read-media-tool.ts` | Inspect image/video/audio |
| `media` | `create-media-tool.ts` | Create/transform media |
| `runFfmpeg` | `run-ffmpeg-tool.ts` | Raw ffmpeg argv |
| `web` | `web-tool.ts` | Web search and fetch |
| `chromeDebug` | `chrome-debug-tool.ts` | CDP browser automation |
| `agent` | `agents/tool.ts` | Spawn/manage bounded subagents |
| `callTool` | `call-tool.ts` | Dynamic reusable tool registry |
| `skill` | `skill-tool.ts` | Load installed skills + manage dynamic skills |
| `plan` | `planning/plan-registration.ts` | Compaction-safe task checklist and reviewed Start lifecycle |
| `localServer` | `local-server-tool.ts` | Local static server |
| `askUser` | `ask-user-tool.ts` | Interactive user input |
| `awareness` | `awareness-tool.ts` | Canonical Awareness catalog and direct structured package API |
| `MCPTool` | `mcp-tool.ts` | MCP 2026-07-28 client → all research tools |

The 14 support tools and guarded `bash` override form the direct palette. The native
`awareness` facade handles catalog discovery and host-bound calls through imported `executeAwarenessCommand`. Checkpoints, history hooks and optional scheduled checks use the same API. The CLI is an external-host adapter. Native Pi registry, event delivery/policy,
mutation guards and plan UI remain active.
External CLI agents can participate through the same physical SQLite file and
normalized workspace, using distinct stable IDs. Workers retain their physical
worktree for file/lock ownership while inheriting the parent `OCTOCODE_AWARENESS_DB`,
so native calls, CLI fallback, guards, registry and delivery share one ledger without
database copies. Awareness list continuations use executable `queries[]` envelopes;
completed commands with oversized output do not auto-replay mutations. See [the agent flow](docs/AWARENESS_AGENT_FLOW.md).

### 3.3 MCP research tools (10 via MCPTool → octocode-mcp server)

These are served through `MCPTool` with `server:"octocode"`. Their schemas are
discovered through the gateway instead of registered individually in Pi's direct
tool palette. Measure the live contracts before estimating context savings.

| Tool | Field gotcha |
|---|---|
| `localSearch` | `searchText` for text search (not `query`) |
| `localFetch` | Standard |
| `astSearch` | `operation` selects match/files/tree/symbols/topology |
| `lspSearch` | `operation` selects semantic query |
| `ghSearch` | Standard |
| `ghGetFileContent` | Standard |
| `ghSearchHistory` | Standard |
| `ghGetHistoryItem` | Standard |
| `ghCloneRepo` | Standard |
| `artifactSearch` | Standard |

**Protocol**: The prompt publishes every enabled tool's complete input contract. Call it as `MCPTool({queries:[{reasoning, action:"call", server:"octocode", tool:"<name>", arguments:<schema-shaped input>} ]})`; target-tool fields never sit beside `action`/`server`/`tool`. Use `action:"describe"` only to return one selected exact JSON schema.

### 3.4 MCP binary resolution (`mcp/config.ts`)

```
resolveLocalOctocodeMcpBin():
  1. import.meta.resolve('octocode-mcp')  → fileURLToPath → local binary
     (requires ESM context; package.json: "type":"module" ✓)
  2. fallback: npx -y octocode-mcp@<fallback-version-range>

buildDefaultOctocodeMcpServer():
  { command: process.execPath, args: [localBin] }  ← preferred
  { command: 'npx', args: ['-y', `octocode-mcp@${OCTOCODE_MCP_FALLBACK_VERSION}`] }
```

The fallback range is owned by `OCTOCODE_MCP_FALLBACK_VERSION` in
[`src/tools/mcp/config.ts`](src/tools/mcp/config.ts); it is not `latest` or an exact version pin.

### 3.5 Discovery ownership

`@octocodeai/agent-contracts` owns skill/MCP source discovery, JSON and full TOML
normalization, stable source IDs, definition revisions, and admission. Native
workspace sources live in `.agents/`; global sources use `getOctocodeHome()`.
Pi defaults apply only to models (`~/.pi/agent/models.json`, honoring
`PI_CODING_AGENT_DIR`), with older private
Octocode paths below public native paths in precedence. Runtime artifacts remain
under the extension namespace.

Foreign Claude, Codex, Cursor, and other recognized sources are disabled until an
exact source revision is reviewed. `capability-state` persists scoped linked
reviews and selections. Changed definitions become pending review; removed sources
remain unavailable. Plain name enablement cannot bypass this boundary. Pi's
`config-sources.ts` projects those states and `mcp/config.ts` applies source filters,
SQLite enablement, and current project trust. MCP files use bounded regular-file
admission; skill directories support links with cycle and change protection.
See [capability sources](docs/CAPABILITIES.md) for the complete path contract.

### 3.6 Discovery timing

```
session_start
  └── warmMcpCatalog(ctx, signal)  ← fire-and-forget
  └── initializationTasks.push(mcpCatalogReady(ctx))  ← awaited in Promise.allSettled

every main-agent before_agent_start
  └── refresh model/hook adapters and effective MCP/skill sources
  └── preparePromptCapabilities(...)  ← publish the current effective revision
  └── assembleSessionPromptContext(...)  ← replace owned projection if changed

every worker before_agent_start
  └── refresh grant; apply queued additions and current removals
  └── use granted skills and the parent-broker MCP catalog
  └── assembleSessionPromptContext(...)  ← shared ownership and budgets
```

### 3.7 Worker capability and cancellation boundary

Typed profiles select a native tool palette within the parent's enabled snapshot;
browser workers can retain `chromeDebug`. Custom workers require explicit tools
and a role prompt. Every worker receives a versioned grant of native tools, exact
skill source IDs, and MCP server/tool pairs. Prompt projection, skill loading,
native execution, and MCP routing enforce the same grant. Workers use the parent
MCP broker rather than opening independently discovered servers.

`agent type:"configure"` replaces selected capability arrays against the current
`snapshotRevision`, optionally checking `grantRevision`. Removals apply immediately;
additions apply before the next worker turn. Disabled parent entries cannot be
granted. The worker asks the parent for missing access. Explicit `tools: []`
serializes as `--no-tools`; lean mode disables extension and skill loading.
Recursive `agent` access and worker smith surfaces remain unavailable.

`waitForAgent` owns abort listeners, silence timers, absolute timers, liveness
probes, and cleanup. `waitForAgentTurn` carries the caller's `AbortSignal` through every
progress-aware wait iteration. Tool and skill generation pass their execution signal
through this boundary and terminate the spawned smith in `finally`.

### 3.8 Session, Awareness, and recovery flow

```mermaid
flowchart TD
  Init[Extension initialization] --> Snapshot[Effective capability snapshot]
  Turn[Each agent turn] --> Snapshot
  Snapshot --> Assemble[session-prompt-context.ts<br/>project current main/worker segments]
  Assemble --> Provider[Prompt provider]
  Turn --> Live[Live turn context]
  Live --> Provider
  Native[Native awareness tool] --> API[executeAwarenessCommand<br/>trusted host context]
  Events[Native Pi events] --> Consumer[awareness-event-consumer.ts<br/>single registered consumer]
  ToolCall[Tool call] --> Mutation[Mutation gate]
  API --> Store[(Awareness store)]
  Consumer --> Store
  Mutation --> Store
  Compact[Compaction] --> PiAdapter[pi-retained-context.ts<br/>interpret Pi retained context]
  PiAdapter --> Rehydrate[rehydration-orchestrator.ts<br/>validate current sources]
  Rehydrate --> Live
```

Native command execution and event delivery resolve the same Awareness identity
and durable store through [`awareness-context.ts`](src/tools/awareness-context.ts),
[`awareness-event-consumer.ts`](src/tools/awareness-event-consumer.ts), and
[`storage-policy.ts`](src/tools/storage-policy.ts). The adapter in
[`pi-retained-context.ts`](src/adapters/pi-retained-context.ts) is the sole boundary
that interprets Pi's retained branch context before
[`rehydration-orchestrator.ts`](src/tools/rehydration-orchestrator.ts) validates and
reprojects current sources.

The peer consumer watches the exact database selected by the opened Awareness
store while a durable session is active. Coalesced filesystem hints schedule
bounded drains while idle; backlog pages continue with an event-loop yield and
delivery failures get three delayed retries. Watches and pending retries close
at session transitions and shutdown. Persisted Pi receipts still precede each
acknowledgement, project trust gates automatic turns, and only external input
rearms the single automatic peer-wake budget.

---

## 4. Skill System

### 4.1 Bundled skills

The build places bundled skills in `dist/skills/`. See the
[README inventory](README.md#bundled-skills-15) for names. The inventory is checked
by `tests/docs-consistency.test.ts`; `tests/package.test.ts` checks bundled artifacts.

### 4.2 Discovery sources

`discoverSkillCandidates` retains every native, Pi, bundled, and foreign source
with source ID, revision, validity, trust, review state, selection, and shadowing.
`discoverSkills` returns one valid enabled entry per normalized name. Bundled names
win unless a source is explicitly selected. Ordinary workspace sources outrank
global sources, and the nearest repository ancestor wins. Explicit Pi runtime
paths and package metadata remain candidates, but require a valid file.

`reviewSkillSource` rechecks identity and revision before recording a scoped
selection. A changed or removed selected source retains the selection and prevents
implicit fallback; review it again or select another source. Name enablement is a separate gate. Recursive and linked directories
use realpath cycle/change checks. `skill-discovery.ts` is the Pi adapter over the
shared owner; prompts, autocomplete, artifacts, and loading consume its effective
list. Workers receive the parent-selected subset through their capability grant.

### 4.3 Skill tool schema

```ts
skill({ queries: [{
  reasoning: "...",
  type: 'load' | 'call',          // default: 'load'
  // type:load fields:
  action: 'load' | 'list',        // default: 'load'
  offset: number,                 // list continuation; copy next.params
  textOffset: number,             // continuation within a long description
  limit: number,                  // list page size, 1–50
  catalogRevision: string,        // pins continuation to its source inventory
  name: string,                   // skill name (from <available_skills>)
  reason: string,                 // why this skill matches (required for load)
  // type:call fields:
  skillType: string,              // skill workflow id
  mode: 'auto' | 'use' | 'create' | 'enhance' | 'fix' | 'list' | 'delete',
  intent: string,                 // what the workflow does
  approveCreate: boolean,
  force: boolean,
}] })
```

---

## 5. Session data layout

### 5.1 Current structure

```
$OCTOCODE_HOME/extension/
  workspaces/
    {workspaceKey}/              ← workspace-only config/state
      discovery.json
      mcp/
      lsp/
  sessions/
    {sessionKey}/                ← safe slug + workspace-bound hash
      manifest.json              ← shared version + identity/producer registry
      session.json               ← sessionId/backlogId + artifact links
      memory.md                  ← bounded agent-maintained session notes
      audit.md                   ← system-written lifecycle history
      plan/
        index.json               ← current planId + task IDs
        plan.html                ← live plan page when a plan exists
        plan.md                  ← shareable plan when a plan exists
        state.json               ← canonical session plan projection
        branches/                ← immutable plan branch snapshots
      tasks/
        index.json               ← task projections using existing step IDs
      backlog/
        index.json               ← session backlogId + unfinished task IDs
      compaction/
      tool-results/              ← session-owned heavy text and image references
      logs/
      workers/
  tmp/
    plan/{scope-hash}/           ← fallback when session is not initialized
    tool-results/                ← ephemeral output without usable session storage
```

Local file history belongs to the shared Awareness store, outside the session artifact tree. With the full Awareness workspace profile, the native `file` boundary captures explicit targets before and after mutation. The default coordination profile provides registry presence and peer delivery; automatic work records and worker audits require guard/full. Active peer-lock checks remain enforced. Awareness packages its private Git object implementation and exposes bounded timeline, read, preview, and apply operations through the imported API also used by the CLI adapter. Pi's `/octocode-rewind` command previews file changes and applies only the reviewed preview; session input and conversation navigation do not create or restore file history.

### 5.2 Identity and authority

`sessionKey` remains the filesystem-safe directory name. A real Pi `sessionId` is stored
inside `manifest.json` and `session.json`; session-file/process fallbacks use an opaque,
deterministic `pi-session-*` ID so private paths are never copied. IDs are not path segments.
`planId` and `taskId` reuse the active plan's stable IDs. `backlogId` is deterministic for
the session and identifies the local backlog projection; it is not a new Awareness table.

The manifest and four JSON index files share `SESSION_ARTIFACT_VERSION = 2`; earlier
versions fail fast and are never upgraded or mixed. These files are inspectable snapshots.
Active plan state and Awareness SQLite remain authoritative for plan/task coordination,
locks, messages, and durable memory.

### 5.3 Path builder API

| Function | File | Returns |
|---|---|---|
| `extensionHome(octocodeHome?)` | `extension-paths.ts` | `$OCTOCODE_HOME/extension` |
| `extensionWorkspaceRoot(cwd, home?)` | `extension-paths.ts` | `...extension/workspaces/{workspaceKey}` |
| `sessionArtifactRoot(input)` | `session-artifacts.ts` | `...extension/sessions/{sessionKey}` |
| `initializeSessionIndexes(ctx)` | `session-index.ts` | Required session/plan/task/backlog index projections |
| `projectSessionPlan(ctx, model)` | `session-index.ts` | Coherent plan/task/backlog ID snapshots |
| `planArtifactsDir(scope)` | `plan-html.ts` | `...sessions/{sessionKey}/plan/` |
| `createSessionArtifactContext(input)` | `session-artifacts.ts` | Contained artifact context with atomic writes and producer registration |
---

## 6. Plan lifecycle

### 6.1 Plan identity

Every plan has a stable **plan ID** (`planId`) derived from `coordination.sourcePlanKey`.
Format: `pi-plan-{uuid4}`. Generated once in `freshCoordination()` and preserved across
all mutations, compactions, and reloads. Available in `PlanReadModelV1.planId` (added 2026-09-03).

### 6.2 Plan phases

```
researching → needs_answers → draft → in_review ── Start ─→ executing → verifying → complete
                                        │                        ↓
                                        └→ abandoned          abandoned
```

`Start` is the single user decision: it binds the displayed RFC revision and begins the first runnable step in one transaction. `accepted` remains an internal/recovery phase if projection cannot finish after revision acceptance; it is not a second normal UI gate. `Request changes` returns review to `draft`.

The native `plan` schema publishes two separate `action:"start"` variants because they are different transitions: an executing plan may start one runnable step with optional `index`, while a reviewed proposal supplies the exact `revision` and answered `authorizationInteractionId` and must omit `index` (`accepted` recovery may reuse its persisted receipt). Keep these fields in separate schema branches; aggregating them advertises a call the preflight contract must reject.

### 6.3 Storage

Plan steps are held in memory (an in-process Map keyed by scope), snapshotted as branch-aware Pi CustomEntries, and projected to `plan/state.json` in the session artifact tree. The manifest records that projection; it is not the plan-state authority.
The scope key = `{cwd}\0id:{sessionId}` when a session ID is available.

### 6.4 Stored version

Active plan snapshots use version 4, including stable step IDs, review state,
coordination, RFC path, `cleared`, and `outcomeReason`. Both branch CustomEntries
and session plan projections reject other versions; version 3 is not restored or
migrated. See [the planning modules](src/tools/planning/).

### 6.5 HTML page data flow

```
 plan(set/propose/start/complete/add/remove)
       ↓
 planning/plan-store.ts (in-memory state mutation)
       ↓
 syncCurrentPlanHtmlIfEnabled(ctx, scope)     ← called after every mutation
       ↓
 writeCurrentPlanArtifacts(ctx, scope, opts)
       ↓
 getCurrentPlanReadModel(ctx, scope)          ← loads PlanReadModelV1 (now has planId)
       ↓
 writeProjectedPlanArtifacts(scope, model)   ← builds HTML + Markdown + writes to session dir
       ↓
 renderOctocodePage(title, bodyHtml)          ← title: "Octocode plan · {shortId}"
       ↓
 artifactCtx.writeText('plan/plan.html')     ← session artifact dir
 artifactCtx.writeText('plan/plan.md')
       ↓
 meta-refresh (every 3s)                     ← browser picks up changes
```

### 6.6 HTML page structure

[`src/tools/plan-html.ts`](src/tools/plan-html.ts) renders the canonical read
model as a flat, left-aligned document. During review, the decision controls
precede the task list. During execution, tasks precede feedback. Planning
workflow, dependency diagram, and raw Markdown remain collapsible so the next
action stays visible. Each task retains its ID and expandable verification,
acceptance, and path details.

### 6.7 Presentation ownership

The plan renderer and [`src/tui/html-page.ts`](src/tui/html-page.ts) own
markup and styling. Keep presentation changes there instead of copying HTML or
color values into this reference. See [docs/UI.md](docs/UI.md) for the interaction
flow and `tests/plan-html.test.ts` for rendering checks.

---

## 7. Discovery phase detail

### 7.1 Session initialization sequence

```
pi: extension loaded
  ↓
  → register support tools and lifecycle hooks

session_start
  → initializeOctocodeSession(ctx, reason)
  → reset prompt, skill inventory, and plan-delivery state
  → dispose the previous runtime
  → await environment propagation before starting config/process consumers
  → initialize session artifacts and recovery sources
  → warmMcpCatalog(ctx, runtime.signal)
  → Promise.allSettled(initializationTasks)

every main-agent before_agent_start
  → refresh model/hook adapters and effective capabilities
  → publish the capability snapshot and parent grant boundary
  → assembleSessionPromptContext(...) with the current bounded catalogs
  → recompute live plan; validate pending recovery
  → replace the owned prompt projection and return changed turn context

every worker before_agent_start
  → refresh the parent-owned grant; apply queued additions
  → project only granted native, MCP, and skill capabilities
  → assembleSessionPromptContext(...) with the same ownership rules
```

### 7.2 Config loading

All config/env flows through `@octocodeai/config`:

| Function | Source |
|---|---|
| `getOctocodeHome()` | `OCTOCODE_HOME` env → platform default |
| `propagateOctocodeEnv({ cwd, trusted, env })` | global + project `.env` → `process.env` |
| `loadOctocoderc(home?)` | `.octocoderc` config file |
| `PROTECTED_KEYS` | Keys never propagated |

Never reimplement — import from `@octocodeai/config`.

---

## 8. Known gaps

| Gap | Severity | Workaround / Fix |
|---|---|---|
| A source can change after a page or prompt was generated | Revision boundary | Execution revalidates access; stale browser mutations are rejected and the next turn projects current capabilities. Restart list paging with its returned continuation. |
| Skill loading returns a bounded first page and supporting-file preview | Recovery contract | `src/tools/skill-tool.ts` reports typed partial reasons and executable `MCPTool` continuations. Follow content pages before acting; merge file discovery results with the preview and follow their continuations. |
| Schema field name surprises (`searchText`, `type` for lsp) | Medium | `MCPTool action:"describe"` exposes the current schema before execution |
| Plan HTML uses meta-refresh (3s) | Transport constraint | Refresh behavior is separate from the plan state and review transaction |
