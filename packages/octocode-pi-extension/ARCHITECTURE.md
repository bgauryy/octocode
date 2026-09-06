# @octocodeai/pi-extension — Architecture

This document describes the Octocode Pi Extension (`packages/octocode-pi-extension`): system prompt assembly, tool registration, skill discovery, session data layout, plan lifecycle, and the HTML/Markdown plan surface. Source contracts remain authoritative. Prompt and discovery sections checked 2026-09-05.

---

## 1. Ownership

| Area | Source contract |
|---|---|
| Main-agent policy | [`src/prompts/system-prompt.ts`](src/prompts/system-prompt.ts): shared policy and Awareness prompt plus the Pi engineering delta |
| Context assembly and lifecycle | [`src/index.ts`](src/index.ts) and [`src/tools/context-segments.ts`](src/tools/context-segments.ts) |
| Direct tool names | [`src/constants.ts`](src/constants.ts); registration in `registerSupportToolPhase` |
| MCP discovery and execution | [`src/tools/mcp-tool.ts`](src/tools/mcp-tool.ts) and [`src/tools/mcp-config.ts`](src/tools/mcp-config.ts) |
| Skills | [`src/tools/skill-tool.ts`](src/tools/skill-tool.ts); bundled inventory in [README.md](README.md#bundled-skills-12) |
| Plan projection | [`src/tools/plan-read-model.ts`](src/tools/plan-read-model.ts) |

This reference does not assign quality grades or claim token savings without a measured baseline.

---

## 2. System prompt assembly

### 2.1 Composition

The full system prompt injected by the extension is the concatenation of:

```
buildOctocodeSystemPrompt(EXTERNAL_AGENT_AWARENESS_PROMPT)
  └── @octocodeai/agent-contracts/prompts  (base Octocode policy)
  └── EXTERNAL_AGENT_AWARENESS_PROMPT      (Awareness coordination rules)
+ <engineering> block                      (src/prompts/system-prompt.ts — THINK→PLAN→CODE→REVIEW)
```

Source: `src/prompts/system-prompt.ts` → `SYSTEM_PROMPT`.
Bundled artifact: `dist/system/SYSTEM_PROMPT.md`.

### 2.2 Frozen policy and live turn context

On the first main-agent turn, `before_agent_start` assembles six stable segments
and freezes the composed system prompt for the session. Later turns reuse those
exact bytes; they do not rediscover the skill catalog or rebuild the system prompt.
Session initialization resets the frozen prompt.

| Segment key | Content | Budget |
|---|---|---|
| `octocode-product-policy` | Bundled `SYSTEM_PROMPT.md` | 20k tokens |
| `mcp-tool-contracts` | `<mcp_catalog_index>` (compact, default) or `<mcp_catalog>` (full, `OCTOCODE_COMPACT_MCP=0`) | 30k tokens |
| `runtime-tool-contracts` | `<runtime_capabilities>` — inline image flags | 10k tokens |
| `dynamic-tool-contracts` | Dynamic skill addendum (excludes installed skill names already in catalog) | 20k tokens |
| `available-skills` | `<available_skills>` — discovered skill list | 20k tokens |
| `session-artifact-contract` | Session memory and audit paths | 1k tokens |

The active plan is a separate attributed turn-context segment, budgeted at 15k
tokens. The hook recomputes it every turn and delivers it when first available,
changed, or cleared. Session memory is delivered initially and registered as a
current recovery source. Compaction recovery validates current sources; it does
not copy the frozen policy segments into the recovery ledger.

MCP execution can refresh independently of the frozen routing prompt. When its
catalog changes, the runtime marks context stale and announces that `/new`
exposes the updated catalog. The `skill` tool rediscovers enabled skills on each
load or list call; the initial prompt inventory stays frozen.

### 2.3 Sub-prompts

| File | Content |
|---|---|
| `src/prompts/system-prompt.ts` | `SYSTEM_PROMPT` constant (base + engineering) |
| `src/prompts/plan-prompt.ts` | Builds the Pi review-and-Start workflow, using shared goal length and truncation constants |
| `@octocodeai/agent-contracts/prompts` | Defines shared worker prompt fragments and expansion; build and tests import the owner directly |

---

## 3. Tool registration

### 3.1 Pi builtin disposition

```ts
// src/constants.ts
DISABLED_BUILTIN_TOOL_NAMES = ['read', 'edit', 'write', 'grep', 'find', 'ls']
// Replaced by MCPTool → octocode-mcp (localGetFileContent, localSearch)

OVERRIDDEN_BUILTIN_TOOL_NAMES = ['bash']
// Octocode owns the implementation (path guard, write-target guard)
```

### 3.2 Direct Pi tools (17)

Registered in `registerSupportToolPhase` in [`src/index.ts`](src/index.ts):

| Tool | Source file | Purpose |
|---|---|---|
| `file` | `file-tool.ts` | Guarded file mutations (edit/write/delete) |
| `bash` | `bash-tool.ts` | Shell tasks (overrides Pi weak builtin) |
| `readMedia` | `read-media-tool.ts` | Inspect image/video/audio |
| `media` | `create-media-tool.ts` | Create/transform media |
| `runFfmpeg` | `run-ffmpeg-tool.ts` | Raw ffmpeg argv |
| `web` | `web-tool.ts` | Web search and fetch |
| `chromeDebug` | `chrome-debug-tool.ts` | CDP browser automation |
| `agent` | `unified-agent-tool.ts` | Spawn/manage subagents |
| `callTool` | `call-tool.ts` | Dynamic reusable tool registry |
| `skill` | `skill-tool.ts` | Load installed skills + manage dynamic skills |
| `plan` | `plan-tool.ts` | Compaction-safe task checklist |
| `localServer` | `local-server-tool.ts` | Local static server |
| `askUser` | `ask-user-tool.ts` | Interactive user input |
| `memory` | `memory-tool.ts` | Durable Awareness learning |
| `lock` | `awareness-coordination-tools.ts` | Exclusive file locks |
| `message` | `awareness-coordination-tools.ts` | Cross-agent messages |
| `MCPTool` | `mcp-tool.ts` | MCP 2026-07-28 client → all research tools |

### 3.3 MCP research tools (10 via MCPTool → octocode-mcp server)

These are served through `MCPTool` with `server:"octocode"`. Their schemas are
discovered through the gateway instead of registered individually in Pi's direct
tool palette. Measure the live contracts before estimating context savings.

| Tool | Field gotcha |
|---|---|
| `localSearch` | `searchText` for text search (not `query`) |
| `localGetFileContent` | Standard |
| `localAnalyzeGraph` | Standard |
| `lspGetSemantics` | `type` field for operation (not `operation`) |
| `ghSearch` | Standard |
| `ghGetFileContent` | Standard |
| `ghSearchHistory` | Standard |
| `ghGetHistoryItem` | Standard |
| `ghCloneRepo` | Standard |
| `npmSearch` | Standard |

**Protocol**: Always call `MCPTool(action:"describe", server:"octocode", tool:"<name>")` before the first call to an unfamiliar tool.

### 3.4 MCP binary resolution (`mcp-config.ts`)

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
[`src/tools/mcp-config.ts`](src/tools/mcp-config.ts); it is not `latest` or an exact version pin.

### 3.5 Discovery timing

```
session_start
  └── warmMcpCatalog(ctx, signal)  ← fire-and-forget
  └── initializationTasks.push(mcpCatalogReady(ctx))  ← awaited in Promise.allSettled

first main-agent before_agent_start
  └── await mcpCatalogReady(ctx)  ← bounded wait for startup discovery
  └── getCachedMcpCatalogAddendum(ctx)  ← compact or full catalog text
  └── discoverSkills(cwd, latestPiSkills)  ← effective enabled inventory
  └── assembleContextSegments(...)  ← freeze stable segments; deliver live context separately
```

---

## 4. Skill System

### 4.1 Bundled skills

The build places bundled skills in `dist/skills/`. See the
[README inventory](README.md#bundled-skills-12) for names. The inventory is checked
by `tests/docs-consistency.test.ts`; `tests/package.test.ts` checks bundled artifacts.

### 4.2 Discovery sources

```
discoverAllSkills(cwd, piSkills, home)  ← src/tools/skill-tool.ts

1. piSkills concrete (path provided)    ← never overwritten; piConcrete guard
2. defaultAgentSkillSources(cwd, home)  ← shared platform roots
3. ~/.octocode/skills                   ← user scope
4. <cwd>/.octocode/skills               ← workspace scope
5. dist/skills (getAssetPaths())        ← warns if asset path resolution fails
6. piSkills SKILL.md runtime paths      ← unique roots not already in sources
```

### 4.3 Skill tool schema

```ts
skill({ queries: [{
  reasoning: "...",
  type: 'load' | 'call',          // default: 'load'
  // type:load fields:
  action: 'load' | 'list',        // default: 'load'
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
      checkpoint-ref.json        ← optional compaction checkpoint pointer
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
      logs/
      workers/
  tmp/
    plan/{scope-hash}/           ← fallback when session is not initialized
    tool-results/                ← ephemeral heavy tool output artifacts
```

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

### 6.3 Storage

Plan steps are held in memory (an in-process Map keyed by scope), snapshotted as branch-aware Pi CustomEntries, and projected to `plan/state.json` in the session artifact tree. The manifest records that projection; it is not the plan-state authority.
The scope key = `{cwd}\0id:{sessionId}` when a session ID is available.

### 6.4 Stored version

Active plan snapshots use version 4, including stable step IDs, review state,
coordination, RFC path, `cleared`, and `outcomeReason`. Both branch CustomEntries
and session plan projections reject other versions; version 3 is not restored or
migrated. See [`active-plan.ts`](src/tools/active-plan.ts).

### 6.5 HTML page data flow

```
 plan(set/propose/start/complete/add/remove)
       ↓
 active-plan.ts (in-memory state mutation)
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

first main-agent before_agent_start
  → await mcpCatalogReady(ctx)               (bounded wait)
  → discoverSkills(cwd, piSkills)
  → assembleContextSegments(...)            (stable system segments and live turn context)
  → cache composed systemPrompt and return it from the hook

later main-agent before_agent_start
  → recompute live plan; validate pending recovery
  → return frozen systemPrompt plus changed turn context
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
| Frozen routing catalogs can differ from live execution discovery | Operational constraint | Use `skill` list for enabled skill changes; start `/new` for an updated MCP routing prompt |
| Skill loading returns a bounded first page and supporting-file preview | Recovery contract | `src/tools/skill-tool.ts` reports typed partial reasons and executable `MCPTool` continuations. Follow content pages before acting; merge file discovery results with the preview and follow their continuations. |
| Schema field name surprises (`searchText`, `type` for lsp) | Medium | Always call `MCPTool action:"describe"` first; add a schema cheat-sheet to SYSTEM_PROMPT |
| Plan HTML uses meta-refresh (3s) | Transport constraint | Refresh behavior is separate from the plan state and review transaction |
| `buildOctocodeSystemPrompt` lives in external `octocode-agent-contracts` | Low | Prompt changes need an `octocode-agent-contracts` release; mitigated by the engineering block inlined here |
