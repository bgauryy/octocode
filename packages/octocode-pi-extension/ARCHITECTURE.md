# @octocodeai/pi-extension — Architecture

This document is the authoritative design reference for the Octocode Pi Extension (`packages/octocode-pi-extension`). It covers system prompt assembly, tool registration, skill discovery, session data layout, plan lifecycle, and the HTML/Markdown plan surface. Updated 2026-09-03.

---

## 1. Ratings (as of 2026-09-03)

| Area | Grade | Notes |
|---|---|---|
| System prompts | **B+** | Well-structured; 6 segments injected; engineering kernel always present. Gap: `buildOctocodeSystemPrompt` lives in `octocode-shared` — prompt changes require releasing an external package. |
| Direct Pi tools (17) | **A−** | Clean registration, descriptions, render hooks. Gap: `DISABLED_BUILTIN_TOOL_NAMES` list is hard-coded; a Pi host rename fails at contract-test time, not compile time. |
| MCP research tools (10) | **B+** | Intentional token-cost trade-off. Gap: agent must know the `MCPTool` schema before calling (`searchText` vs `query`; `type` vs `operation`) — `describe`-first is required but not enforced. |
| Skill tool | **A** | `type:load / type:call` unified facade, piConcrete guard, multi-source discovery, proper precedence. |
| Discovery phase | **B** | Config loads cleanly; MCP warms at `session_start`; skills discovered at `before_agent_start`. Gap: bundled skills silently skipped on dev builds (try/catch). MCP binary fallback uses `latest` not pinned. |
| Session data paths | **C** | Files land under `workspaces/{key}/sessions/{sessionKey}/` — nested and hard to navigate by session ID. Desired: `sessions/{sessionId}/` flat structure. |
| Plan ID | **A** (fixed 2026-09-03) | `planId` now in `PlanReadModelV1` (from `coordination.sourcePlanKey`). Previously absent. |
| Plan HTML UX | **B** (improved 2026-09-03) | Progress bar, phase timeline, decision section, Mermaid dep graph. Added: plan ID pill header, step grid layout, expandable step details (checkCommand/acceptance/paths), `data-plan-id` on root section, title includes short ID. Gap: no real-time SSE push — still meta-refresh every 3s. |

---

## 2. System Prompt Assembly

### 2.1 Composition

The full system prompt injected by the extension is the concatenation of:

```
buildOctocodeSystemPrompt(EXTERNAL_AGENT_AWARENESS_PROMPT)
  └── @octocodeai/octocode-shared/prompts  (base Octocode policy)
  └── EXTERNAL_AGENT_AWARENESS_PROMPT      (Awareness coordination rules)
+ <engineering> block                      (src/prompts/prompt.ts — THINK→PLAN→CODE→REVIEW)
```

Source: `src/prompts/prompt.ts` → `SYSTEM_PROMPT`.
Bundled: `dist/system/SYSTEM_PROMPT.md` (16.9 KB, rebuilt by `scripts/build.mjs`).

### 2.2 Per-turn context segments

Six segments are injected/refreshed on every turn by `assembleContextSegments` (called from `before_agent_start`):

| Segment key | Content | Budget |
|---|---|---|
| `octocode-product-policy` | Bundled `SYSTEM_PROMPT.md` | 20k tokens |
| `mcp-tool-contracts` | `<mcp_catalog_index>` (compact, default) or `<mcp_catalog>` (full, `OCTOCODE_COMPACT_MCP=0`) | 30k tokens |
| `runtime-tool-contracts` | `<runtime_capabilities>` — inline image flags | 10k tokens |
| `dynamic-tool-contracts` | Dynamic skill addendum (excludes installed skill names already in catalog) | 20k tokens |
| `available-skills` | `<available_skills>` — discovered skill list | 20k tokens |
| `active-plan` | Live plan state (never frozen across compactions) | 15k tokens |

### 2.3 Sub-prompts

| File | Content |
|---|---|
| `src/prompts/prompt.ts` | `SYSTEM_PROMPT` constant (base + engineering) |
| `src/prompts/plan-prompt.ts` | Re-exports `buildPlanPrompt`, `PLAN_PROMPT_MAX_GOAL` from `octocode-shared` |
| `src/prompts/subagent-shared.ts` | Re-exports `expandSubagentPrompt`, `SUBAGENT_FRAGMENTS` from `octocode-shared` |

---

## 3. Tool Registration

### 3.1 Pi builtin disposition

```ts
// src/constants.ts
DISABLED_BUILTIN_TOOL_NAMES = ['read', 'edit', 'write', 'grep', 'find', 'ls']
// Replaced by MCPTool → octocode-mcp (localGetFileContent, localSearch)

OVERRIDDEN_BUILTIN_TOOL_NAMES = ['bash']
// Octocode owns the implementation (path guard, write-target guard)
```

### 3.2 Direct Pi tools (17)

Registered in `registerSupportToolPhase` (`src/index.ts:956`):

| Tool | Source file | Purpose |
|---|---|---|
| `file` | `file-tool.ts` | Guarded file mutations (edit/write/delete) |
| `bash` | `bash-tool.ts` | Shell tasks (overrides Pi weak builtin) |
| `readMedia` | `read-media-tool.ts` | Inspect image/video/audio |
| `media` | `create-media-tool.ts` | Create/transform media |
| `runFfmpeg` | `run-ffmpeg-tool.ts` | Raw ffmpeg argv |
| `web` | `web-tool.ts` | Web search and fetch |
| `chromeDebug` | `chrome-debug.ts` | CDP browser automation |
| `agent` | `agent-tools.ts` | Spawn/manage subagents |
| `callTool` | `call-tool.ts` | Dynamic reusable tool registry |
| `skill` | `skill-tool.ts` | Load installed skills + manage dynamic skills |
| `plan` | `plan-tool.ts` | Compaction-safe task checklist |
| `localServer` | `local-server-tool.ts` | Local static server |
| `askUser` | `ask-user-tool.ts` | Interactive user input |
| `memory` | `memory-tool.ts` | Durable Awareness learning |
| `lock` | `lock-tool.ts` | Exclusive file locks |
| `message` | `message-tool.ts` | Cross-agent messages |
| `MCPTool` | `mcp-tool.ts` | MCP 2026-07-28 client → all research tools |

### 3.3 MCP research tools (10 via MCPTool → octocode-mcp server)

These are NOT direct Pi tools. They are served through `MCPTool(server:"octocode", ...)`. Removing 13 tool definitions from the Pi palette saves ~30k tokens per turn.

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
  2. fallback: npx -y octocode-mcp@latest  (slower; uses latest, not pinned)

buildDefaultOctocodeMcpServer():
  { command: process.execPath, args: [localBin] }  ← preferred
  { command: 'npx', args: ['-y', 'octocode-mcp@latest'] }  ← fallback
```

### 3.5 Discovery timing

```
session_start
  └── warmMcpCatalog(ctx, signal)  ← fire-and-forget
  └── initializationTasks.push(mcpCatalogReady(ctx))  ← awaited in Promise.allSettled

before_agent_start
  └── await mcpCatalogReady(ctx)  ← blocks until MCP ready
  └── getCachedMcpCatalogAddendum(ctx)  ← compact or full catalog text
  └── discoverSkills(cwd, latestPiSkills)  ← all 5 sources
  └── assembleContextSegments(...)  ← inject 6 segments
```

---

## 4. Skill System

### 4.1 Bundled skills (14)

All live in `dist/skills/` after `yarn build`. Source: `skills/` symlinked to `.agents/skills/`.

| Skill | Trigger phrase |
|---|---|
| `octocode-research` | Check code claims; trace callers; map systems; diagnose failures |
| `octocode-orchestrator` | Coordinate workstreams, TDD, subagents, evals |
| `octocode-subagent` | Delegate substantial work; A2A; local Ollama offload |
| `octocode-architect` | Consequential code planning (ItaiC-style) |
| `octocode-brainstorming` | Explore ideas before building |
| `octocode-chrome-devtools` | Live CDP evidence; network/DOM/performance |
| `octocode-code-graph` | Dep graph; cycle analysis; change-impact |
| `octocode-documentation` | Missing/stale docs; README; API docs |
| `octocode-eval-benchmark` | Measure if a change helped; trustworthy evals |
| `octocode-prompt-optimizer` | Improve prompts, schemas, policies |
| `octocode-rfc-generator` | Write RFC before consequential changes |
| `octocode-roast` | Evidence-backed code critique |
| `octocode-scraping` | Extract/map public web content |
| `octocode-skills` | Find/rate/create/install skills |

### 4.2 Discovery sources (precedence order)

```
discoverAllSkills(cwd, piSkills, home)  ← src/tools/skill-tool.ts:47

1. piSkills concrete (path provided)    ← never overwritten; piConcrete guard
2. defaultAgentSkillSources(cwd, home)  ← platform engine roots
3. ~/.octocode/skills                   ← user scope  (exists: 12 skills ✓)
4. <cwd>/.octocode/skills               ← workspace scope
5. dist/skills (getAssetPaths())        ← bundled, try/catch silences dev builds
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

## 5. Session Data Layout

### 5.1 Current structure

```
$OCTOCODE_HOME/extension/
  workspaces/
    {workspaceKey}/              ← sha256(cwd)-based key
      discovery.json             ← MCP server discovery state
      mcp/                       ← MCP config and server state
      lsp/                       ← LSP pool state
      sessions/
        {sessionKey}/            ← {sessionId}-{sha256(sessionId+workspace).slice(0,12)}
          manifest.json          ← artifact producer registry
          checkpoint-ref.json    ← compaction checkpoint pointer
          plan/
            plan.html            ← live HTML plan page
            plan.md              ← shareable Markdown
          compaction/
            latest.md            ← compaction checkpoint
          logs/                  ← session logs
          workers/
            {agentId}/           ← per-worker artifacts
  tmp/
    plan/{scope-hash}/           ← fallback when session not initialized
    tool-results/                ← heavy tool output artifacts
```

### 5.2 Desired structure (gap / TODO)

The user-visible path `workspaces/{key}/sessions/{key}/plan/plan.md` is hard to navigate by session ID alone. The desired layout separates workspace-scoped state from session-scoped state:

```
$OCTOCODE_HOME/extension/
  workspaces/
    {workspaceKey}/              ← workspace config stays here
      discovery.json
      mcp/
      lsp/
  sessions/                      ← flat session root (desired)
    {sessionId}/                 ← raw Pi session ID (no hash suffix)
      manifest.json
      checkpoint-ref.json
      plan/
        plan.html
        plan.md
      compaction/
      logs/
      workers/
```

**Migration path**: Change `sessionArtifactRoot` in `session-artifacts.ts` to use
`path.join(extensionHome(octocodeHome), 'sessions', identity.sessionId)` where
`sessionId` is the raw Pi session ID. Keep `workspaceAgentRoot` for workspace-only
state (discovery, mcp, lsp). Add a one-time migration pass that moves existing
`workspaces/{key}/sessions/{key}/` directories to the new flat location.

### 5.3 Path builder API

| Function | File | Returns |
|---|---|---|
| `extensionHome(octocodeHome?)` | `extension-paths.ts` | `$OCTOCODE_HOME/extension` |
| `extensionWorkspaceRoot(cwd, home?)` | `extension-paths.ts` | `...extension/workspaces/{workspaceKey}` |
| `sessionArtifactRoot(input)` | `session-artifacts.ts` | `...workspaces/{key}/sessions/{sessionKey}` |
| `planArtifactsDir(scope)` | `plan-html.ts` | `...sessions/{sessionKey}/plan/` |
| `artifactContextForScope(scope)` | `session-artifacts.ts` | Full artifact context (resolve, writeText, registerProducer) |

---

## 6. Plan Lifecycle

### 6.1 Plan identity

Every plan has a stable **plan ID** (`planId`) derived from `coordination.sourcePlanKey`.
Format: `pi-plan-{uuid4}`. Generated once in `freshCoordination()` and preserved across
all mutations, compactions, and reloads. Available in `PlanReadModelV1.planId` (added 2026-09-03).

### 6.2 Plan phases

```
researching → needs_answers → draft → in_review → accepted → executing → verifying → complete
                                                                                     ↓
                                                                                  abandoned
```

### 6.3 Storage

Plan steps are held in memory (an in-process Map keyed by scope), snapshotted as branch-aware Pi CustomEntries, and projected to `plan/state.json` in the session artifact tree. The manifest records that projection; it is not the plan-state authority.
The scope key = `{cwd}\0id:{sessionId}` when a session ID is available.

### 6.4 Stored versions

| Version | Changes |
|---|---|
| v3 | Base: steps, review state, coordination, RFC path |
| v4 | + `cleared: boolean`, `outcomeReason` |

### 6.5 HTML page data flow

```
 plan(set/start/complete/add/remove)
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

### 6.6 HTML page structure (post 2026-09-03)

```html
<section data-plan-read-model="1" data-plan-id="{planId}" data-revision="{revision}">

  <!-- 1. Plan identity header: plan ID pill, workspace, revision, steps count -->
  <div class="plan-meta">
    <div class="plan-meta-item"><span class="plan-meta-label">Plan ID</span>
      <span class="plan-meta-val plan-id-pill">{shortId}…</span></div>
    <div class="plan-meta-item"><span class="plan-meta-label">Workspace</span>…</div>
    <div class="plan-meta-item"><span class="plan-meta-label">Steps</span>done/total</div>
  </div>

  <!-- 2. Phase timeline: Research → Clarify → Draft → Review → Execute → Verify → Complete -->
  <section class="timeline">…</section>

  <!-- 3. RFC (when accepted RFC exists) -->
  <section class="rfc">…</section>

  <!-- 4. Decisions (clarify phase answers) -->
  <section>…</section>

  <!-- 5. Progress: bar + stats grid (done/total/in-progress/blocked/decisions) -->
  <section class="plan-stats">…</section>

  <!-- 6. Browser reply: feedback textarea + contextual action buttons -->
  <section data-browser-reply>…</section>

  <!-- 7. Flow gates (5 standard research/RFC/discuss/derive/verify gates) -->
  <section>…</section>

  <!-- 8. Steps list (rich per-step rendering) -->
  <section>
    <ul class="steps">
      <li class="done|doing|todo|blocked" data-task-id="{id}">
        <span class="glyph">✓|▸|○|⊘</span>
        <span class="step-main">{index}. {label}</span>
        <!-- expandable detail when checkCommand/acceptance/paths are set -->
        <details class="step-detail">
          <code class="check-cmd">$ {checkCommand}</code>
          <p class="acceptance">{acceptance}</p>
          <ul class="step-paths"><li>{path}</li>…</ul>
        </details>
      </li>
    </ul>
  </section>

  <!-- 9. Dependency flow (Mermaid flowchart TD) -->
  <section>…</section>

  <!-- 10. Raw markdown collapsible -->
  <details>…</details>
</section>
```

### 6.7 Step CSS classes

| Status | Border | Glyph color | Background tint |
|---|---|---|---|
| `done` | cyan 28% | `--cyan` | none |
| `doing` | gold 35% | `--gold` | gold 4% |
| `todo` | `--line` | `--muted` | none |
| `blocked` | orange 28% | `#EA580C` | none |

---

## 7. Discovery Phase Detail

### 7.1 Session initialization sequence

```
pi: extension loaded
  ↓
before_session_start
  → loadOctocoderc() + propagateOctocodeEnv()  (env/config)
  → validate Pi version compatibility
  → register support tools phase (17 direct tools)
  → disableBuiltinTools(pi)  (removes read/edit/write/grep/find/ls)

session_start
  → initializeOctocodeSession(pi, ctx, event)
  → warmMcpCatalog(ctx, signal)              (fire-and-forget)
  → initializationTasks = [mcpCatalogReady] (awaited)
  → openPersistentAwareness(ctx)
  → loadActiveSession(ctx, event)
  → restoreActivePlanIfNeeded(ctx)
  → Promise.allSettled(initializationTasks)

before_agent_start
  → await mcpCatalogReady(ctx)               (blocks here until MCP ready)
  → discoverSkills(cwd, piSkills)
  → assembleContextSegments(ctx, scope)      (6 segments)
  → frozenSystemPrompt = pi.setSystemPrompt(...)
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

## 8. Known Gaps

| Gap | Severity | Workaround / Fix |
|---|---|---|
| Session paths too nested (`workspaces/{key}/sessions/{key}/`) | Medium | See §5.2 for desired flat structure and migration path |
| Skills silently skipped on dev builds (try/catch in `getAssetPaths()`) | Low | Run `yarn build:skills` to populate `dist/skills/`; add a logged warning |
| MCP binary fallback uses `latest` not pinned version | Low | Local binary resolves correctly in production; fallback only in restricted envs |
| Schema field name surprises (`searchText`, `type` for lsp) | Medium | Always call `MCPTool action:"describe"` first; add a schema cheat-sheet to SYSTEM_PROMPT |
| Plan HTML uses meta-refresh (3s) not SSE push | Low | Fine for current usage; SSE would eliminate perceived lag |
| `buildOctocodeSystemPrompt` lives in external `octocode-shared` | Low | Prompt changes need an `octocode-shared` release; mitigated by the engineering block inlined here |
| `graph_facts.rs` export detection broken for Go/Java/PHP/Kotlin | High | See `.octocode/graph-facts-audit.md`; affects `localAnalyzeGraph` dead-code results |
