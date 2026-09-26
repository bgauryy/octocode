# @octocodeai/pi-extension — Architecture

This document describes the Octocode Pi Extension (`packages/octocode-pi-extension`): system prompt assembly, tool registration, skill discovery, session data layout, plan lifecycle, and the HTML/Markdown plan surface. Source contracts remain authoritative. Capability, prompt, worker, and discovery sections checked 2026-09-18.

---

## 1. Ownership

| Area | Source contract |
|---|---|
| Main-agent policy | [`src/contracts/prompts`](src/contracts/prompts) and the Pi adapter in [`src/prompts/system-prompt.ts`](src/prompts/system-prompt.ts) |
| Runtime measurements | [`src/adapters/pi-physiology.ts`](src/adapters/pi-physiology.ts) and [`src/adapters/pi-physiology-regulation.ts`](src/adapters/pi-physiology-regulation.ts) own local measurements and bounded advice |
| Shared communication | [`src/tools/communication-runtime.ts`](src/tools/communication-runtime.ts) binds the bundled Rust catalog, lifecycle and edit-lease checks |
| Context assembly and lifecycle | [`src/index.ts`](src/index.ts) composes the host and preserves hook order; [`src/tools/prompt-preflight.ts`](src/tools/prompt-preflight.ts) owns the two-stage `before_agent_start` preparation/`agent_start` cancellation boundary; [`src/tools/session-prompt-context.ts`](src/tools/session-prompt-context.ts) and [`src/tools/context-segments.ts`](src/tools/context-segments.ts) own attributed prompt context and budgets |
| Internal error logging | [`src/internal-error-log.ts`](src/internal-error-log.ts) owns private paths, redaction, formatting, and best-effort append behavior |
| Direct tool names and Pi builtin policy | [`src/constants.ts`](src/constants.ts) declares the public inventory; [`src/tools/tool-registration.ts`](src/tools/tool-registration.ts) owns the effective palette, static registration order, and builtin filtering |
| Query execution and partial receipts | [`src/tools/query-envelope.ts`](src/tools/query-envelope.ts) owns ordering, preflight, concurrency, and cancellation. Schema conversion preserves the owning validator's constraints. [`src/tools/query-batch-error.ts`](src/tools/query-batch-error.ts) preserves complete evidence from successful and failed rows, including a first-item failure. Operation-specific validation stays with each tool. |
| Host tool failure adaptation | [`src/tools/tool-result-error.ts`](src/tools/tool-result-error.ts) converts internal error results into Pi's thrown-error channel without clipping content. Images are serialized as JSON in that text-only channel. Success result shapes remain unchanged. |
| File mutations | [`docs/FILE_MUTATIONS.md`](docs/FILE_MUTATIONS.md) defines the dedicated `@octocodeai/octocode-extension-rust` boundary: native snapshots, mutations, sync and diff; TypeScript owns text semantics, path policy, batches and receipts. |
| Synchronous state publication | [`src/tools/atomic-state-file.ts`](src/tools/atomic-state-file.ts) owns atomic private-state and rebuildable-workspace publication; registries, MCP configuration, and discovery use that boundary |
| MCP discovery and execution | [`src/tools/mcp-tool.ts`](src/tools/mcp-tool.ts) orchestrates catalog discovery, freshness, and action dispatch. [`src/tools/mcp/connection-manager.ts`](src/tools/mcp/connection-manager.ts) owns live transports; [`src/tools/mcp/config-watcher.ts`](src/tools/mcp/config-watcher.ts) owns hot reload; [`src/tools/mcp/dynamic-proxy.ts`](src/tools/mcp/dynamic-proxy.ts) owns described proxy state; [`src/tools/mcp/register-tool.ts`](src/tools/mcp/register-tool.ts) owns the Pi gateway adapter; and [`src/tools/mcp/action-execution.ts`](src/tools/mcp/action-execution.ts) owns resource/prompt/tool-call execution. [`src/tools/mcp/catalog-model.ts`](src/tools/mcp/catalog-model.ts) owns snapshot identity, [`src/tools/mcp/catalog-execution.ts`](src/tools/mcp/catalog-execution.ts) owns freshness and invalidation fencing, and [`src/tools/mcp/presentation.ts`](src/tools/mcp/presentation.ts) owns rendering and schema diagnostics. |
| Skill discovery | [`src/tools/skill-discovery.ts`](src/tools/skill-discovery.ts); the `skill` tool consumes that inventory from [`src/tools/skill-tool.ts`](src/tools/skill-tool.ts) |
| MCP client requests | [`src/tools/mcp/client-handlers.ts`](src/tools/mcp/client-handlers.ts) owns roots, sampling, elicitation, and request-scoped cancellation. Sampling sends complete message payloads; display previews never become model input. The gateway owns catalog invalidation through a callback. |
| Worker spawning and waits | [`src/tools/agents/tool.ts`](src/tools/agents/tool.ts), [`src/tools/agents/lifecycle.ts`](src/tools/agents/lifecycle.ts), and [`src/tools/agents/wait.ts`](src/tools/agents/wait.ts) |
| Capability sources and review | [`src/contracts/capability-sources.ts`](src/contracts/capability-sources.ts), [`src/contracts/agent-skills.ts`](src/contracts/agent-skills.ts), and [`src/contracts/capability-state.ts`](src/contracts/capability-state.ts); Pi applies host trust and enablement through the skill/MCP adapters |
| Model and command-hook adapters | [`src/adapters/pi-capability-adapters.ts`](src/adapters/pi-capability-adapters.ts) owns initialization, refresh, and disposal; [capability reference](docs/CAPABILITIES.md) owns source and execution contracts |
| Effective snapshots and worker grants | [`src/tools/capability-session.ts`](src/tools/capability-session.ts), [`src/tools/worker-capabilities.ts`](src/tools/worker-capabilities.ts), and [`src/tools/mcp/broker.ts`](src/tools/mcp/broker.ts); shared Zod schemas live in [`src/contracts/capabilities.ts`](src/contracts/capabilities.ts) |
| Pi retained-context evidence | [`src/adapters/pi-retained-context.ts`](src/adapters/pi-retained-context.ts) |
| Execution journal and replay | [`src/tools/execution-events.ts`](src/tools/execution-events.ts) owns typed events and the reducer; [`src/tools/execution-runtime.ts`](src/tools/execution-runtime.ts) binds Pi branch persistence; [`src/tools/lifecycle-ui.ts`](src/tools/lifecycle-ui.ts) observes host events |
| Plan projection | [`src/tools/plan-read-model.ts`](src/tools/plan-read-model.ts) |

This reference does not assign quality grades or claim token savings without a measured baseline.

The [runtime audit](docs/RUNTIME_AUDIT.md) records implementation repairs, upstream
comparisons, and validation limits for the September 13, 2026 review.

---

## 2. System prompt assembly

### 2.1 Composition

The extension combines a short Pi host capability/trust adapter with the canonical coder kernel in `src/contracts/prompts`. Root policy owns execution, delegation, verification, continuation and routing. Workers receive bounded role contracts and interaction safety, without root user-facing authority. Operational detail comes from current catalogs and selected skills.

Source: `src/prompts/system-prompt.ts` → `SYSTEM_PROMPT`.
Bundled artifact: `dist/system/SYSTEM_PROMPT.md`.

### 2.2 Versioned capability projection and live turn context

Before every main-agent or worker turn, the controller in `prompt-preflight.ts`
refreshes declarative adapters, then `preparePromptCapabilities` resolves the
capabilities allowed by that process and publishes a validated snapshot with
`schemaVersion: 1` and a content revision. `assembleSessionPromptContext` attributes
and bounds the segments. The hook replaces only its prior owned projection,
preserving the host's current prompt. Unchanged content remains byte-stable;
changed capabilities appear on the next turn without a new session. If preparation
fails, the controller leaves the scope armed and its `agent_start` guard aborts the
provider run rather than sending an incomplete prompt.

The reported context status is `ready` when this projection is assembled; it is
not a provider cache-hit receipt. Estimates count the active direct tools and
prompt/turn overhead, separately from Pi's retained conversation measurements.
MCP discovery revalidates configuration before publishing, and coalesces changes
behind in-flight discovery. Skill execution receives raw Pi metadata, never a
previously resolved inventory that could introduce a second source identity. A
skill body becomes a restore-only current source only after a successful `skill`
load; merely appearing in the available-skill inventory does not register its body
or consume one of the bounded current-source slots.

| Segment key | Content | Budget |
|---|---|---|
| `octocode-product-policy` | Bundled `SYSTEM_PROMPT.md` | 12k tokens |
| `mcp-tool-contracts` | Enabled server/tool routing metadata in `<mcp_catalog_index>`; `MCPTool action:"describe"` loads one exact schema and activates a Pi proxy when the host admits its dynamic name | 6k tokens; the initial index is also bounded to 18k characters and larger catalogs expose an executable continuation |
| `runtime-tool-contracts` | `<runtime_capabilities>`; the mutable `capability_revision` is a separate per-turn context segment (96-token budget), preserving the cacheable system prefix | 500 tokens |
| `dynamic-tool-contracts` | Dynamic skill addendum (excludes installed skill names already in catalog) | 6k tokens |
| `available-skills` | `<available_skills>` — discovered skill list | 5k tokens |
| `session-artifact-contract` | Session memory and audit paths | 500 tokens |
| `agents-protocol` | Exact native `AGENTS.md` content, attributed as user-authority project instructions | 12k tokens |

Direct Pi tool names, descriptions, schemas, snippets, and guidelines remain on
Pi's native tool-contract channel; the extension does not duplicate them in a
second `<native_tools>` system-prompt catalog.

These are per-segment estimated maxima, not reserved allocations. Initial segments
also share a 50k aggregate ceiling. The final prompt, direct tool contracts, and
new turn context must fit the smaller of 80k estimated tokens and the selected
model's valid declared context window. Missing model metadata retains the 80k
fallback. Overflow fails preparation without clipping content. Estimates use
`ceil(UTF-16 characters / 4)`; they are not tokenizer counts or provider usage.
Pi owns retained conversation, output allocation, and automatic compaction.
Octocode never treats an inactive local plan as evidence that a threshold crossing
can be cancelled; completed turns may still belong to a continuing session.

Workers receive only segments supported by their grant of native tools, exact
skill IDs, and MCP server/tool pairs. Their role prompt remains caller-owned;
MCP/skill catalogs and host bindings use the shared attribution and token budgets.
Native project instruction discovery is suppressed for workers, whose task context
comes from the explicit worker packet.

Explicit worker thinking levels pass to Pi unchanged; omitted levels use Pi's
settings. Pi's model/provider adapter owns reasoning compatibility, not model-name
heuristics in the extension.

The active plan is a separate attributed turn-context segment, budgeted at 15k
tokens. After compaction or resume, its current full content is delivered when
absent from retained context, even if unchanged. This allowance is separate from
the 8k recovery budget for other segments; retained or freshly delivered plans
are validated without consuming that recovery budget. Runtime physiology is another turn segment, limited to 128 estimated
tokens and never rehydrated as current state. It carries changed advisory actions
from fresh host receipts; unavailable sensors do not establish recovery. The
observation contract and thresholds live in Pi runtime primitives; actual
compaction/retry control remains with Pi. The observer uses
the same hook composer as lifecycle and prompt middleware, retains only a
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

Prompt preparation remains pending until the complete assembly succeeds. If it
fails, the `agent_start` hook aborts before provider dispatch. This uses Pi's
active-run cancellation boundary: `before_agent_start` has no active run signal,
and Pi catches errors from that hook. The guard belongs to the session-state
identity and exact scope; replacement sessions do not inherit a failed preparation.
Physiology advisories also commit only after successful assembly, so a failed
attempt cannot consume a warning that never reached model context. Passive tool
observations include their monotonic tool counter in their identity, preserving
distinct same-millisecond outcomes while deduplicating terminal-event replays.

Execution also validates current enablement at use time. A changed MCP or skill
source updates the next prompt projection. Bounded MCP list → describe and skill
list → load flows expose exact metadata with revision-bound continuations; a
catalog change restarts paging instead of mixing revisions.

### 2.3 Sub-prompts

| File | Content |
|---|---|
| `src/prompts/system-prompt.ts` | Pi host adapter plus root/worker selection over the shared canonical prompt builders |
| `src/prompts/plan-prompt.ts` | Thin Pi call-syntax adapter over the shared atomic-Start plan prompt |

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

Registered in `registerSupportTools` in [`src/tools/tool-registration.ts`](src/tools/tool-registration.ts). The composition root keeps lifecycle order, while this module is the single static-tool and builtin-policy boundary:

[`registerUniqueTool`](src/tools/octocode-tools.ts) preserves tool-owned schemas and input preparation, adapts failures, and prefixes prompt guidelines with the owning tool name because Pi combines those guidelines into a flat section.

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
| `MCPTool` | `mcp-tool.ts` | MCP 2026-07-28 client → all research tools |

### 3.3 MCP research tools (catalog-driven via MCPTool → octocode-mcp server)

The built-in catalog contains thirteen tools, including `astTopology` and credential-gated `clasify`. MCP omits it when the resolved `OCTOCODE_CLASSIFICATION_API` is absent or blank. It accepts one complete semantic query or a batch of resource-question matrices and preserves ordered same-resource pages without hidden reduction. The tools are served through `MCPTool`; omitted `server` defaults to the built-in
`octocode` server for research, resource, and prompt actions. Stdio and Streamable HTTP connections receive one bounded startup retry by default; `startupRetries` (0–5) and `retryDelayMs` (0–10000) can override that policy per server. Retry waits honor request cancellation, and targeted server stops abort their pending starts. `MCPTool action:"status"` pings live connections, reports healthy/unhealthy/disconnected rows, and evicts failed connections so their next use reconnects. Legacy SSE remains unsupported because no reviewed active source requires it; URL servers use Streamable HTTP.

Their schemas are
discovered through the gateway instead of registered individually in Pi's direct
tool palette. Measure the live contracts before estimating context savings.

| Tool | Field gotcha |
|---|---|
| `localSearch` | `searchText` for text search; `include` and `exclude` accept globs |
| `localFetch` | `fullContent`, `matchString`, and `startLine` plus `endLine` are exclusive extraction modes |
| `astSearch` | `files` uses `names`, `pathPattern`, or `pathRegex`; `pattern` and `rule` belong to `match` |
| `lspSearch` | `operation` selects semantic query |
| `ghSearch` | Standard |
| `ghGetFileContent` | Standard |
| `ghSearchHistory` | Standard |
| `ghGetHistoryItem` | Standard |
| `ghCloneRepo` | Standard |
| `artifactSearch` | Standard |

**Protocol**: The prompt publishes bounded routing metadata, not input schemas. Use `MCPTool action:"describe"` to load the selected exact JSON schema. When the host admits dynamic names, describe returns and activates a namespaced Pi tool; call that returned tool directly with the target arguments. A fixed host allowlist is reported explicitly and leaves the generic gateway as the callable fallback. `MCPTool action:"call"` is blocked until the same server/tool schema was described, keeps target fields in `arguments`, and revalidates the described schema digest before execution. Compaction clears receipts that no longer have an active provider-visible proxy, so deferred-schema calls cannot outlive model-visible schema context.

### 3.4 MCP binary resolution (`mcp/config.ts`)

```
resolveLocalOctocodeMcpBin():
  1. import.meta.resolve('octocode-mcp')  → fileURLToPath → local binary
     (requires ESM context; package.json: "type":"module" ✓)
  2. fallback: npx -y octocode-mcp@<declared-dependency-version>

buildDefaultOctocodeMcpServer():
  { command: process.execPath, args: [localBin] }  ← preferred
  { command: 'npx', args: ['-y', `octocode-mcp@${version}`] }
```

[`src/package-metadata.ts`](src/package-metadata.ts) reads the fallback version from the extension manifest's `dependencies.octocode-mcp`. It also supplies the extension version to MCP client identification, update checks, and the UI. No separate version pin needs synchronization.

### 3.5 Discovery ownership

The extension's `src/contracts/` modules own host-independent skill/MCP source discovery, JSON and full TOML
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



---

## 4. Skill System

### 4.1 Bundled skills

The build places bundled skills in `dist/skills/`. See the
[README inventory](README.md#bundled-skills-14) for names. The inventory is checked
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

Persistent synchronous JSON writers use `writePrivateFileAtomicSync`. The
rebuildable workspace discovery snapshot uses `writeEphemeralFileAtomicSync`.
Session artifacts retain their separate contained writer because they also
enforce session-root and producer-registration invariants.

### 5.2 Identity and authority

`sessionKey` is the filesystem-safe directory name; real Pi session IDs live inside the manifest. Session-file/process fallbacks use opaque deterministic IDs, so private paths are not copied into identifiers. Plan/task IDs reuse the active plan IDs; the session backlog is a local projection. The manifest and four JSON indexes share `SESSION_ARTIFACT_VERSION = 2`; incompatible versions fail explicitly. Pi session state owns plans; the communication database owns shared leases and messages.

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

The native `plan` schema publishes two separate `action:"start"` variants because
they represent different transitions. An executing plan can start one runnable
step with an optional `index`. A reviewed proposal supplies the exact `revision`
and answered `authorizationInteractionId`, and it must omit `index`. Recovery from
`accepted` can reuse its persisted receipt. Keep these fields in separate schema
branches; aggregating them advertises a call that preflight must reject.

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
| `loadOctocoderc(home?)` | global `.octocoderc` config file |
| `loadOctocodercLayers({ home, cwd, env })` | `[workspace, global]` `.octocoderc` layers for `resolveConfigFields` |
| `PROTECTED_KEYS` | Keys never propagated |

Never reimplement — import from `@octocodeai/config`.

---

## 8. Known gaps

| Gap | Severity | Workaround / Fix |
|---|---|---|
| A source can change after a page or prompt was generated | Revision boundary | Execution revalidates access; stale browser mutations are rejected and the next turn projects current capabilities. Restart list paging with its returned continuation. |
| Skill loading returns a bounded first page and supporting-file preview | Recovery contract | `src/tools/skill-tool.ts` reports typed partial reasons and executable `MCPTool` continuations. Follow content pages before acting; merge file discovery results with the preview and follow their continuations. |
| Plan HTML uses meta-refresh (3s) | Transport constraint | Refresh behavior is separate from the plan state and review transaction |

## Communication and session persistence

The bundled communication runtime owns peer presence, messages, documents, and advisory leases. Pi owns plans, context recovery, and durable user approvals. See [communication and local state](docs/COMMUNICATION_AGENT_FLOW.md).
