# @octocodeai/pi-extension

<div align="center">
<img src="https://github.com/bgauryy/octocode-mcp/raw/main/packages/octocode-pi-extension/assets/logo.png" width="640px" alt="Octocode + Pi">
</div>

> **Octocode for [Pi](https://github.com/earendil-works/pi)** — native code-research tools, live web search, persistent memory, edit-safety hooks, 10 bundled skills (plus a browser subagent skill), and a full operating-model system prompt. One package install.

```bash
pi install npm:@octocodeai/pi-extension
/octocode-status
```

The Octocode CLI is **bundled** at `dist/cli/octocode.js` and exposed as `$OCTOCODE_CLI` at startup.
Run it with: `node $OCTOCODE_CLI <command>`

```bash
bash: node $OCTOCODE_CLI unzip archive.zip    # unpack archive
bash: node $OCTOCODE_CLI skill --list          # list available skills
bash: node $OCTOCODE_CLI context               # show agent protocol
```

---

## Harness at a glance

| Surface | Count |
|---|---|
| System prompt (operating model) | 1 block |
| Native Octocode tools | 13 |
| Support tools (web, chromeDebug, memory, agent) | 20 |
| Custom edit tool | 1 |
| Slash commands | 6 |
| Bundled skills | 10 (+1 browser subagent skill) |

---

## What loads at install

When Pi loads the extension, four things happen automatically:

1. **System prompt injected.** The Octocode operating model block is appended to Pi's system prompt — authority model, tool-routing rules, research proof ladder, memory protocol, code discipline, and safety gates. The block is idempotent: appended once, never duplicated.

2. **Native tools registered.** 13 Octocode tools (GitHub/local/LSP/npm/binary) execute directly through `@octocodeai/octocode-tools-core`. No MCP server is spawned for these calls.

3. **Support tools registered.** Web search, context management, agent spawning, and 13 typed memory/coordination tools are registered as Pi tools.

4. **Slash commands registered.** Six harness commands: `/octocode-status`, `/octocode-harness`, `/octocode-setup`, `/octocode-skills-update`, `/octocode-memory-digest`, `/octocode-memory-forget`.

**What does NOT happen automatically:**
- No repo is cloned without explicit agent/user invocation.
- No destructive command runs without the normal Pi/tool confirmation path.
- No GitHub token is read from `.env`; tokens use shell env or Octocode auth storage.
- No extra install is needed for the CLI — it is bundled at `dist/cli/octocode.js` and available as `$OCTOCODE_CLI`.

---

## Quick start

```bash
pi install npm:@octocodeai/pi-extension
/octocode-status        # health check: prompt, skills, memory, tools, web provider
/octocode-harness       # exact list of every registered tool, command, and skill
```

Pin the operating model to a project or globally:

```bash
/octocode-setup          # writes .pi/APPEND_SYSTEM.md in the current project
/octocode-setup --global # writes ~/.pi/agent/APPEND_SYSTEM.md
```

---

## System prompt — operating model

**Source:** `packages/octocode-pi-extension/src/prompts/sections/` (composed to `dist/system/SYSTEM_PROMPT.md`)

The injected system prompt defines eight protocol blocks that govern agent behavior for every session:

| Block | What it enforces |
|---|---|
| `<authority>` | Conflict resolution order: safety → correctness → minimal scope. |
| `<operating_model>` | Work loop: orient → scope → hypothesize → search/prove → act → verify. Request mode classification (answer / diagnose / plan / change / monitor). |
| `<memory>` | Typed Awareness dispatch table — when to recall, coordinate, verify, record, reflect, digest, or forget. |
| `<tools>` | Tool routing ladder. AST+LSP combined workflow. GitHub discovery/read/clone/history flow. Minify aggressively. |
| `<research>` | Proof ladder: candidate search → exact read → AST/shape → LSP → independent corroboration → verdict. Confidence levels: confirmed / likely / uncertain. |
| `<skills>` | Skill dispatch: when each bundled skill applies; how to load SKILL.md before following. |
| `<code>` | Scope discipline. No stubs, no fake wiring, no fallbacks unless requested. Trace real flow before changing contracts. |
| `<safety>` | Never log secrets. Validate paths. Ask before destructive actions. Same failure 3× → stop and re-plan. |

---

## Native Octocode tools (13)

Execute directly via `@octocodeai/octocode-tools-core` — no MCP server, no network hop.

### GitHub tools (6)

| Tool | Purpose |
|---|---|
| `ghSearchCode` | Search code contents or file paths across any GitHub repo. Use `match:"path"` for filenames first; `match:"file"` for content snippets. |
| `ghSearchRepos` | Discover repos by name, topic, language, stars, forks, or date. `concise:true` for triage. |
| `ghHistoryResearch` | Research PRs and commit history. List mode + detail mode (prNumber). Patch selection: `patches.mode:"selected"` is cheapest. |
| `ghGetFileContent` | Read a file or region from any GitHub repo. `minify:"symbols"` first for large/unknown files. |
| `ghViewRepoStructure` | Browse a repo tree before reading files. Cheaper than content search when path is unknown. |
| `ghCloneRepo` | Materialize a repo (or sparse subtree) locally for repeated reads, AST, or LSP. Use `sparsePath`. Needs `ENABLE_CLONE`. |

### Local tools (5)

| Tool | Purpose |
|---|---|
| `localSearchCode` | Text, regex, or AST structural search. `mode:"structural"` with `pattern:` or `rule:` for code-shape queries. Returns file+line anchors for LSP escalation. |
| `localFindFiles` | Find files by name glob, path pattern, regex, size, time, permissions, or type. |
| `localGetFileContent` | Read a local file or exact region. `matchString` returns slices + matchRanges. `minify:"symbols"` for orientation. |
| `localViewStructure` | Browse a local directory tree. Cheapest first orientation step — no content loaded. |
| `localBinaryInspect` | Inspect archives, compressed streams, and binaries. Modes: inspect / list / extract / decompress / strings / unpack. |

### LSP tool (1)

| Tool | Purpose |
|---|---|
| `lspGetSemantics` | definitions · references · callers/callees · callHierarchy · hover · documentSymbols · workspaceSymbol · diagnostic · typeDefinition · implementation · supertypes/subtypes. Bundled servers: TS/JS, Python (pyright), Rust (rust-analyzer), C/C++ (clangd), YAML, JSON, HTML, CSS, Shell. |

### Package + archive tools (2)

| Tool | Purpose |
|---|---|
| `npmSearch` | Look up npm packages and find their source repos. Exact name → rich result; keyword → paged candidates. |
| `unzip` | Unpack any archive (.zip, .tar.gz, .jar, .7z, .deb, .dmg) to a local path. Wrapper for `localBinaryInspect mode:"unpack"`. |

---

## Support tools (20)

### Web search (1)

| Tool | Purpose |
|---|---|
| `web` | Search the live web (`query`) or fetch clean page text (`url`). Provider order: Tavily → Serper → DuckDuckGo fallback. |

### Browser DevTools (3)

| Tool | Purpose |
|---|---|
| `chromeDebug` | Connect to Chrome DevTools Protocol (CDP) to debug, inspect, and control a live browser. `scheme` selects the debug need (`debug`, `network`, `console`, `screenshot`, `raw`, …). `scheme:"raw"` + `method:"Domain.method"` gives full CDP API control. Screenshots/PDFs are written to `<workspace>/.octocode/screenshots/`. Requires Chrome with `--remote-debugging-port=9222 --user-data-dir=~/.octocode/chrome-debug/profile`, or pass `launch:true`. Chrome ≥136: always uses a non-default `--user-data-dir`. |
| `spawnSubagent` | Spawn a typed, pre-configured Pi subagent with the right tools, system prompt, and skill. Pass `agent:"browser-agent"` to spawn a Chrome DevTools specialist with `chromeDebug` + web + local read tools. The subagent stays alive for multi-turn interaction via `AgentMessage`. Accepts `url`, `port`, `launch`, `task`, `context`. Each subagent type lives under `subagents/<name>/` with its own `SYSTEM_PROMPT.md` and `skills/`. |
| `browserAgent` | Generates a complete `spawnAgent` configuration for a dedicated browser debugging subagent. Pass `task` and optionally `url` + `port`. Returns a system prompt (with CDP reference + `chromeDebug` usage guide) and tool list `["chromeDebug"]` ready for `spawnAgent`. The spawned subagent stays alive for multi-turn interaction via `AgentMessage` — use for iterative browser work: navigate, analyze, steer, re-analyze. Pairs with the `browser-agent` skill which documents the communication protocol and all 57 CDP domains. |

### Context management (1)

| Tool | Purpose |
|---|---|
| `manage_context` | `type:"compact"` — summarize history to free context window space; auto-triggered at ≥60% full. `type:"new"` — start a fresh session; otherwise use `/new` manually. |

### Agent spawning (2)

| Tool | Purpose |
|---|---|
| `spawnAgent` | Start a background Pi worker process over RPC. Returns `agentId`. Workers cannot spawn workers. Registry/output previews are process-local. |
| `AgentMessage` | List, status, send, steer, followUp, wait, abort, or kill spawned workers in the current Pi process. |

### Memory + coordination tools (13 agent tools + 2 user commands)

All memory is stored in a local SQLite DB under Octocode memory home (`~/.octocode/memory/` by default).

Detailed live Awareness flow and examples: [`docs/MEMORY_AGENT_FLOW.md`](docs/MEMORY_AGENT_FLOW.md). Post-task reflection, memory hygiene, and skill/harness proposals are documented in [`docs/REFLECT.md`](docs/REFLECT.md).

| Tool | Purpose |
|---|---|
| `memory_recall` | Awareness: recall durable lessons before risky/unfamiliar work. Accepts `query`, `smart:true`, `references`, `regex`, `sort`, `strict_scope`. |
| `memory_record` | Awareness reflection: store a root cause, decision, workaround, or verified gotcha after evidence exists. Attaches `file`/`files`/`folders`/`repo`/`workspace_path` scope. Skips duplicates unless `supersedes` or `allow_similar:true`. |
| `memory_reflect` | Awareness reflection: capture a reusable lesson after non-trivial work. Prefer over `memory_record` when `fix_repo`, `fix_harness`, or `failure_signature` apply — creates refinements and clusters failure patterns. |
| `workspace_status` | Show active file locks, working agents, and memory/coordination stats for current workspace. |
| `memory_workspace_status` | Compatibility alias for `workspace_status`. |
| `agent_signal` | Common agent coordination inbox: publish/list/reply/resolve/ack questions, handoffs, blockers, decisions, and FYIs. |
| `file_lock` | Stateful file lock manager for parallel agents: lock/release/status/renew by `task_id`. |
| `memory_file_lock` | Compatibility alias for `file_lock`. |
| `memory_refine_get` | List open repo-fix refinements. Use after reflections may have left actionable fixes. |
| `memory_audit_unverified` | List pending edit tasks that still need verification. Use after every edit batch. |
| `memory_verify` | Mark a pending edit task as verified or failed. Three call forms: `{task_id}` (single), `{task_ids:[...]}` (batch array), `{allPending:true}` (clear all pending for this agent in one call). |
| `memory_export_harness` | Awareness reflection: export agent improvement proposals (fix_harness reflections + high-importance lessons) as markdown for AGENTS.md/CLAUDE.md. Never writes files — review and paste after human approval. |
| `memory_notify` | Compatibility alias for `agent_signal({action:"publish"})`; prefer `agent_signal` for list/reply/resolve. |

User-owned maintenance commands:

| Command | Purpose |
|---|---|
| `/octocode-memory-digest` | Preview or apply memory cleanup. Default is dry-run; `--apply` mutates after confirmation. Use `--export-doc` to write a markdown report. |
| `/octocode-memory-forget` | Preview or apply memory deletion by `--id`, `--tag`, `--before`, or `--max-importance`. Default is dry-run; `--apply` mutates after confirmation. |

---

## Memory — out of the box

Memory works with zero configuration. A local SQLite database is created on first use at `~/.octocode/memory/` (override with `OCTOCODE_HOME`). No server, no signup, no sync.

### What gets stored

Memory is intentionally selective. The agent only stores things that are reusable across sessions:

| Good to store | Skip |  
|---|---|
| Root causes of recurring bugs | Routine status updates |
| Architectural decisions and their rationale | Raw logs or test output |
| Verified workarounds | Facts already captured in git/docs |
| Recurring failure signatures | Secrets or tokens |
| Security findings | One-off observations |

### Memory labels and default importance

Every memory has a label and an importance score (1–10). The agent picks these automatically, but you can override:

| Label | Default importance | When used |
|---|---|---|
| `SECURITY` | 9 | Security findings, auth issues, data exposure |
| `INCIDENT` | 9 | Production incidents, data loss events |
| `BUG` | 8 | Confirmed bugs with root cause |
| `RELEASE` | 8 | Release decisions, publish blockers |
| `GOTCHA` | 7 | Non-obvious traps, counterintuitive behavior |
| `IMPROVEMENT` | 7 | Verified performance or quality improvements |
| `DECISION` | 6 | Architecture and design decisions |
| `ARCHITECTURE` | 6 | Structural patterns and ownership rules |
| `FEATURE` · `DOCS` · `CONFIG` · `WORKFLOW` · `REFACTOR` · `API` · `TEST` · `BUILD` · `EXPERIENCE` · `OTHER` | varies | General purpose |

### Core memory loop

```
Awareness before work   → memory_recall({ query, smart:true })
Awareness after edits   → memory_audit_unverified
                          → memory_verify({ allPending: true })           # clear all at once
                          → memory_verify({ task_ids: [...] })            # clear a subset
                          → memory_verify({ task_id, status })            # single
Awareness reflect after work → memory_reflect({ task, outcome, worked, lesson })
Specific finding        → memory_record({ task_context, observation, label, importance })
```

### Scoping — why it matters

Attach scope so recall is context-aware. Without scope, memories are global. With scope, `memory_recall` surfaces the most relevant lessons first:

```
file:         a specific file the memory concerns
files:        multiple related files
folders:      a directory subtree
repo:         owner/repo — repo-wide scope
workspace_path: absolute path to the workspace root
```

### Multi-agent awareness

When multiple agents work in the same workspace, `memory_workspace_status` shows:
- Which files are currently locked by another agent
- Active PENDING tasks awaiting verification
- Agent IDs of running workers

`memory_notify` lets agents post structured messages to each other (kinds: `claim` / `handoff` / `question` / `reply` / `blocker` / `request` / `decision` / `fyi`).

### Keeping the store clean

```bash
# Preview what would be pruned
/octocode-memory-digest

# Export a full markdown report
/octocode-memory-digest --export-doc
# writes to .octocode/memory-reports/

# Preview deletion by tag or age
/octocode-memory-forget --tag EXPERIENCE --before 2026-01-01 --max-importance 5

# Apply after confirmation
/octocode-memory-forget --tag EXPERIENCE --before 2026-01-01 --max-importance 5 --apply
```

The `supersedes` field on `memory_record` / `memory_reflect` replaces stale entries instead of accumulating duplicates.

---

## Subagents — out of the box

`spawnAgent` starts a real background Pi process over RPC and returns an `agentId` immediately. The parent session continues while the worker runs independently.

### How it works

```
spawnAgent({ task, ... })  →  agentId (returned immediately)
         ↓
   Pi worker process (isolated, no shared context)
         ↓
AgentMessage({ action: "wait", agentId })  →  collect result
```

Workers run with `OCTOCODE_PI_SUBAGENT=1` in their environment. They are true child processes — not threads, not coroutines.

The parent keeps worker records, output previews, and `agentId` lookup in memory. Collect needed results with `AgentMessage({ action: "wait" | "status" })` before session shutdown or reload; after that, spawn fresh workers instead of relying on old IDs.

### Resource modes

| Mode | What the worker loads | When to use |
|---|---|---|
| `lean` (default) | No extensions, no skills, no prompt templates | Lightweight tasks, scripting, data processing |
| `octocode` | This extension only (`@octocodeai/pi-extension`) | Worker needs native tools and memory |
| `default` | Full Pi discovery (all installed extensions/skills) | Worker needs the full user environment |

### Forbidden worker tools

Workers can **never** call `spawnAgent` or `AgentMessage` — recursive spawning is blocked at the process level. Orchestration always stays in the parent session.

### Worker lifecycle

```
starting  →  running  →  idle  →  exited
                    ↘  failed
                    ↘  killed
```

### AgentMessage actions

| Action | Use |
|---|---|
| `list` | Show all spawned workers and their status |
| `status` | Check one worker's status, last output, exit code |
| `send` | Send a follow-up message to a running worker |
| `steer` | Interrupt and redirect the worker's next turn |
| `followUp` | Queue a message to deliver after the worker completes its current turn |
| `wait` | Block until the worker finishes (with optional `timeoutMs`) |
| `kill` | Terminate a worker process |

### TUI output

Both agent tools have rich TUI rendering:

**Call site** — what you see when the tool fires:
```
spawnAgent  auth-researcher — Research the auth token flow · sonnet:high
AgentMessage  wait  auth-researcher
AgentMessage  send  auth-researcher — also check the tests
```

**Result (collapsed):**
```
⧗ spawnAgent · auth-researcher · running  · expand for output
✓ AgentMessage · auth-researcher · exited · expand for output
✗ AgentMessage · code-scanner · failed    · expand for output
```

**Result (expanded):** shows `status · elapsed` line then agent's full output, split on real newlines (no JSON-escaped `\n`).

**List action (collapsed):**
```
◧ AgentMessage list · 3 agents · 1 running · 2 done · 0 failed
```
**List action (expanded):** one line per agent — `name (id[:8]) · status · elapsed — output preview`

**Status bar during `wait`:**
```
⧗ Waiting for "auth-researcher"…    ← shown in Pi status bar for the full wait duration
```

Status icon key: `✓` exited · `✗` failed/killed · `⧗` running · `◎` idle · `○` starting

### Worker prompt discipline

Worker prompts must be **fully self-contained** — workers cannot see the parent conversation. Include:

- **Goal** — what to produce, not how
- **Non-goals** — what is explicitly out of scope
- **Constraints** — style, scope, allowed files/commands
- **Evidence anchors** — specific file paths, line numbers, or facts (never "based on earlier research")
- **Allowed scope** — which files/packages the worker may touch
- **Verification** — what the worker should check before reporting done
- **Stop conditions** — when to stop and report a blocker instead of continuing

### When to spawn vs stay in the parent

| Spawn a worker | Stay in parent |
|---|---|
| Large independent task with clear output | Bug fix or refactor that needs shared context |
| Long-running work (clone, benchmark, eval) | Dependent steps where order matters |
| Adversarial/coverage check on the parent's work | Small/medium tasks |
| Truly parallel independent packages or hypotheses | Tasks where worker output feeds the next decision |

### Parallel pattern

```
// Spawn all independent workers first
const a = spawnAgent({ task: "audit package A", resourceMode: "octocode" })
const b = spawnAgent({ task: "audit package B", resourceMode: "octocode" })

// Then collect — do NOT wait one by one before spawning the next
AgentMessage({ action: "wait", agentId: a.agentId })
AgentMessage({ action: "wait", agentId: b.agentId })

// Verify before relaying results
// Treat worker reports as claims until artifacts are confirmed
```

---

## Custom edit tool

The extension replaces Pi's built-in edit tool with an enhanced version:

| Feature | Description |
|---|---|
| Exact current-file matching | `oldText` matched against original file content, not after earlier edits are applied |
| Batch edits | Multiple `edits[]` replacements in one call, computed before writing |
| Multi-file queries | `queries[]` for logically-related changes across files, all computed before any file is written |
| `matchMode:"normalized"` | Opt-in fuzzy matching for whitespace/indentation/unicode quote/dash drift when exact copied bytes do not match |
| `matchMode:"lineRange"` | Replace by freshly-read line range |
| Stale-read detection | Detects edits against stale content and surfaces actionable mismatch diagnostics |
| `replaceAll` | File-wide replacement when intentional |
| Diff/patch detail in results | Shows exactly what changed |

**Edit safety bridge:** Before every Pi write/edit call, the awareness bridge claims a lock on target files. After the edit result, it releases locks and records a `PENDING` task. The system prompt instructs the agent to run the stated verification and call `memory_verify` to clear the task. Pi's in-process `agent_end` gate can send a follow-up instead of letting a session silently conclude while `PENDING` tasks remain.

---

## Bundled skills (8)

The 8 skills below ship in `dist/skills/` and load via Pi resource discovery. A separate **browser subagent skill** (below) ships under `subagents/browser-agent/skills/` and is loaded only into the spawned browser subagent, not the main session.

One of the eight skill folders is copied at build time from [`@octocodeai/octocode-awareness`](../octocode-awareness). `octocode-awareness` owns coordination, signals, reflection, and hooks. Older prompts that mention `octocode-reflection` or `octocode-agent-communication` should load Awareness. The extension imports the same package directly for memory tools and edit-safety hooks. See [Development notes](#development-notes) for the source-of-truth rule.

### `browser-agent` (subagent skill)

Chrome DevTools Protocol specialist subagent. Load to understand when/how to spawn a browser debugging subagent via `spawnSubagent({agent:"browser-agent"})`. Covers: multi-turn CDP session management, `[FINDING]/[ACTION]/[DONE]` output protocol, all 57 CDP domains, `chromeDebug` scheme guide, and AgentMessage coordination patterns.

**When to load:** any time browser debugging work needs multiple turns — security audits, network analysis, DOM inspection, coverage, workers/service workers, emulation, or automation.

### `octocode-research`

> Evidence-first technical research, investigation, planning, review, and code changes.

**Modes:** Map · Validate · Investigate · Plan · Review · Change · Loop

**References:** `octocode.md` · `research-flow.md` · `code-research.md` · `finding-checks.md` · `long-research.md` · `github-landscape.md`

**Output:** Finding/Evidence/Confidence/Next for quick answers; TL;DR/scope/verdict/risks for decision briefs; severity-ranked file:line findings for reviews.

---

### `octocode-brainstorming`

> Validate ideas against evidence before building. Diverge then converge.

**Flow:** FRAME → DIVERGE → RESEARCH → CROSS-POLLINATE → STRESS-TEST → SYNTHESIZE → DECIDE

**Modes:** Generate · Validate · Map

**Hard gates:** Stops and recommends one option when: idea maps to 3+ unrelated spaces; surfaces stay thin after synonym retries; evidence materially conflicts; or delegation would exceed 5 workers.

**Output:** TL;DR / Framings / Evidence by surface / What survived review / Verdict / Risks / Next. Build-ready ideas hand off to `octocode-rfc-generator`.

---

### `octocode-rfc-generator`

> RFCs, architecture proposals, migration plans, and research-backed decisions before coding.

**Flow:** UNDERSTAND → RESEARCH → COMPARE OPTIONS → WRITE RFC → CLOSE OPEN QUESTIONS → DERIVE KPIs → VALIDATE → DELIVER

**Output structure:** Three-file folder at `<workspace>/.octocode/rfc/{name}/`:

| File | Role |
|---|---|
| `RFC.md` | **Decide** — reviewer-facing; frozen at decision; single source of truth for goals/scope/decision |
| `IMPLEMENTATION.md` | **Build** — implementer-facing; live; every RFC open question closed with evidence |
| `KPI.md` | **Verify** — outlives ship date; acceptance criteria + success signals + traceability matrix |

Always compares ≥2 alternatives including do-nothing. `IMPLEMENTATION.md` and `KPI.md` reference RFC section anchors — never restate them.

---

### `octocode-roast`

> Brutally honest code review with severity-ranked findings and fix paths.

**Severity tiers:** Capital offenses (security/data loss) → Felonies (type abuse/N+1/brittle async) → Crimes (magic numbers/hidden state) → Slop (AI verbosity/duplicates) → Misdemeanors (TODOs/console logs)

**Laws:** Cite or drop it — every finding needs a `file:line`. Punch the code, not the coder. Never output a secret value. Present findings before editing.

**Output:** Top roast / Findings by severity / Autopsy / Redemption paths / Fix checkpoint.

---

### `octocode-prompt-optimizer`

> Improve prompts, SKILL.md files, AGENTS.md, and agent instructions.

**Flow:** READ → UNDERSTAND → RATE → FIX → VALIDATE → OUTPUT (6 gated steps; no gate may be skipped)

**Modes:** Full Path (all gates separate) for multi-section/ambiguous/high-risk; Fast Path (READ+UNDERSTAND and RATE+FIX may combine) for short/low-risk.

**Non-negotiables:** Preserve working logic and intent. Target <10% line increase. VALIDATE is never skipped.

---

### `octocode-skills`

> Find, evaluate, lint, install, and author Agent Skills.

**Flow:** UNDERSTAND → DISCOVER → INSPECT → JUDGE → RECOMMEND → USER GATE → ACT → VERIFY

**Hard rules:** Inspect real SKILL.md before recommending. Gate every install/write/overwrite/symlink behind user approval.

**Scripts:** `scripts/skill-lint.mjs` — lint skill structure, routing, scripts, and prompt quality. Runs before any created or edited skill is reported done.

---

### `octocode-subagents`

> Spawn, coordinate, and synthesize background Pi worker agents.

**When to use:** Delegation decisions, writing self-contained worker prompts, coordinating parallel agents with `spawnAgent`/`AgentMessage`, synthesizing multi-agent results, and understanding worker limitations.

**Covers:** `spawnAgent` parameters (task, context, model, thinking, tools, resourceMode), worker prompt templates, `AgentMessage` actions (list, status, wait, send, steer, followUp, kill, abort), result synthesis rules, tool allowlists, and automatic cleanup on `session_shutdown`.

---

### `octocode-awareness`

> Live workspace coordination: recall, file locks, verification gates, signals, reflection, and lifecycle hooks.

**Load it when:** starting or planning work, claiming files before an edit, checking for other agents' locks, processing messages, recording lessons, or finishing a task.

**Default loop:** Think/Plan (`memory_recall`, `workspace_status`, inbox check) → Before edits (`file_lock` or CLI `lock acquire`) → After edits (`memory_audit_unverified` → `memory_verify`) → Communicate when needed (`agent_signal`) → Finish/learn (`memory_reflect`, `memory_record`).

Backed by the `wirePiAwarenessHooks` bridge described in [Awareness bridge — edit safety](#awareness-bridge--edit-safety) and the `memory_*` / `file_lock` / `agent_signal` tools in [Support tools](#support-tools-20).

---

## Slash commands (6)

| Command | Purpose |
|---|---|
| `/octocode-status` | Extension health check: prompt loaded, skills found, memory home, tools count, web provider. |
| `/octocode-harness` | List every registered tool, command, and skill with surface counts. |
| `/octocode-setup` | Install/update the managed `APPEND_SYSTEM.md` block. `--global` writes to `~/.pi/agent/`. |
| `/octocode-skills-update` | `pi update <source>` for this package + reload Pi resources. Prompts for confirmation. |
| `/octocode-memory-digest` | Preview/apply memory cleanup. Default dry-run; `--apply` mutates after confirmation. |
| `/octocode-memory-forget` | Preview/apply memory deletion by id/tag/age/importance. Default dry-run; `--apply` mutates after confirmation. |

---

## Configuration

Octocode config is optional. Config files load in this order:

```
~/.octocode/.env           # global
<project>/.octocode/.env   # project-local (trusted projects only)
```

Useful keys:

```bash
TAVILY_API_KEY=tvly-...     # best web search quality
SERPER_API_KEY=...          # Google SERP alternative
OCTOCODE_WEB_USER_AGENT=... # optional fetch user-agent override
ENABLE_CLONE=1              # enable ghCloneRepo
ENABLE_LOCAL=1              # enable local tools (default on)
```

GitHub auth:

```bash
# Preferred: shell env (not in .env files — protected keys are blocked)
export GITHUB_TOKEN=ghp_...
export GH_TOKEN=ghp_...

# Or: Octocode auth storage
node $OCTOCODE_CLI auth login
node $OCTOCODE_CLI auth status
```

Full config docs: [CONFIGURATION.md](https://github.com/bgauryy/octocode-mcp/blob/main/docs/CONFIGURATION.md) · [AUTHENTICATION.md](https://github.com/bgauryy/octocode/blob/main/docs/AUTHENTICATION.md)

---

## Web search provider order

| Priority | Provider | Key required |
|---|---|---|
| 1 | Tavily | `TAVILY_API_KEY` or `TAVILY_API_TOKEN` |
| 2 | Serper | `SERPER_API_KEY` |
| 3 | DuckDuckGo | No key — automatic fallback |

---

## Model configuration (`models.json`)

Custom models live in `~/.pi/agent/models.json`. Hot-reloads when you open `/model`.

**Critical fields:**

| Field | Effect |
|---|---|
| `contextWindow` | Pi context accounting and compaction timing |
| `maxTokens` | Maximum generated tokens — **set to the provider's real published limit or the model stops mid-answer** |
| `reasoning` | Enables Pi thinking controls |
| `input` | Include `"image"` only for vision-capable models |
| `compat.supportsDeveloperRole` | `false` for servers that reject the OpenAI `developer` role |
| `compat.supportsReasoningEffort` | `false` for servers that reject `reasoning_effort` |
| `compat.supportsUsageInStreaming` | `false` for servers that reject streaming usage options |

Minimal example:

```json
{
  "providers": {
    "my-provider": {
      "baseUrl": "https://api.example.com/v1",
      "api": "openai-completions",
      "apiKey": "$MY_API_KEY",
      "compat": {
        "supportsDeveloperRole": false,
        "supportsReasoningEffort": false
      },
      "models": [
        {
          "id": "model-id",
          "name": "Human label",
          "contextWindow": 131072,
          "maxTokens": 65536,
          "reasoning": false,
          "input": ["text"]
        }
      ]
    }
  }
}
```

API key forms: `"$ENV_VAR"` · `"!op read 'op://vault/item/cred'"` · `"literal-key-avoid-in-shared-files"`

Full Pi model docs: [models.md](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/models.md)

---

## Awareness bridge — edit safety

The awareness bridge runs automatically on every Pi edit/write tool call:

1. **Before edit:** Claims a file lock for each target path. Other agents see the lock via `memory_workspace_status`.
2. **After edit:** Releases locks and records a `PENDING` task in memory.
3. **Agent duty:** The system prompt instructs the agent to run the stated verification and call `memory_verify` to clear the task. Use `{allPending:true}` to clear all pending tasks in one call, or `{task_ids:[...]}` for a subset.
4. **Verify gate:** Pi's in-process awareness bridge sends a follow-up before conclusion while any `PENDING` task remains unverified.

```bash
node packages/octocode-awareness/skills/octocode-awareness/scripts/awareness.mjs hooks install --host codex --project-dir . --dry-run
```

Pi does not need that shell-hook install step; the extension wires `wirePiAwarenessHooks` in-process. The command above is only useful when configuring shell-hook hosts such as Codex, Claude, or Cursor from the same repo.

To bypass a stuck verify gate (misfires only):

```bash
OCTOCODE_NO_VERIFY_GATE=1 pi ...
```

---

## Troubleshooting

| Symptom | Fix |
|---|---|
| Extension seems inactive | Run `/octocode-status`, then `/octocode-harness`. |
| Model stops mid-answer | Set `maxTokens` to the provider's real published limit in `~/.pi/agent/models.json`. |
| Model missing from `/model` | Check `apiKey`, `baseUrl`, and `api` value in `models.json`. |
| Web search is weak or slow | Add `TAVILY_API_KEY` or `SERPER_API_KEY` to Octocode env. |
| GitHub calls are unauthenticated | Run `node $OCTOCODE_CLI auth login` or export `GITHUB_TOKEN` / `GH_TOKEN` / `OCTOCODE_TOKEN` in shell env. |
| Agent uses `grep`/`cat`/`curl` instead of native tools | Run `/octocode-harness`; remind the agent to use native Octocode tools. |
| Verify gate blocks conclusion | Run the stated verification, then call `memory_verify({ allPending: true })` to clear all in one call (or `{ task_ids: [...] }` for a subset). If no stop hook is installed, pending tasks appear in `memory_audit_unverified` but do not block the UI. |
| Stuck pending tasks from a dead session | Use `octocode-awareness` memory hygiene: preview the stale agent scope with `memory_audit_unverified`, then abandon only after approving that scope. |
| `ghCloneRepo` not available | Set `ENABLE_CLONE=1` in Octocode env. |
| Local tools not available | Check `ENABLE_LOCAL` — defaults on; set `ENABLE_LOCAL=1` if overridden. |

---

## Further reading

| Doc | Covers |
|---|---|
| [`docs/TOOLS.md`](docs/TOOLS.md) | Full tool inventory (every family, including `bash`/`edit`/`write`) and a task→tool routing guide |
| [`docs/MEMORY_AGENT_FLOW.md`](docs/MEMORY_AGENT_FLOW.md) | Live Awareness flow with worked examples |
| [`docs/REFLECT.md`](docs/REFLECT.md) | Post-task learning, memory hygiene, skill/harness improvement proposals |
| [`@octocodeai/octocode-awareness` README](../octocode-awareness/README.md) | The shared runtime: CLI, library API, hooks, and data model behind the memory tools |

---

## Development notes

Canonical sources (do **not** edit generated copies — build overwrites them):

- **System prompt:** `packages/octocode-pi-extension/src/prompts/sections/` (composed to `dist/system/SYSTEM_PROMPT.md`)
- **Awareness source of truth:** [`packages/octocode-awareness`](../octocode-awareness) owns the DB schema, task/signal API, hooks bridge, and the primary `octocode-awareness` skill. Pi imports `@octocodeai/octocode-awareness` directly for runtime behavior and copies `packages/octocode-awareness/skills/octocode-awareness`.
- **Skills source:** repo-root `skills/` plus `packages/octocode-awareness/skills/octocode-awareness` for the awareness-owned primary skill.
- **Generated skill copies:** `packages/octocode-pi-extension/skills/` is gitignored and regenerated by `yarn workspace @octocodeai/pi-extension build:skills`.
- **Build script:** `packages/octocode-pi-extension/scripts/build.mjs` (syncs generated package skills, copies them to `dist/skills/`, injects `octocode-config.mjs` into each skill's `scripts/` dir)

```bash
yarn workspace @octocodeai/pi-extension build
yarn workspace @octocodeai/pi-extension build:skills
yarn workspace @octocodeai/pi-extension test     # builds first, then runs vitest
```

The test suite verifies that this README lists every live tool, command, and bundled skill registered by the extension harness.

---

[Octocode](https://octocode.ai) · [GitHub](https://github.com/bgauryy/octocode-mcp) · [Configuration](https://github.com/bgauryy/octocode-mcp/blob/main/docs/CONFIGURATION.md) · [Authentication](https://github.com/bgauryy/octocode/blob/main/docs/AUTHENTICATION.md) · [Pi packages](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/packages.md)
