# Octocode Awareness

<p align="center">
  <img src="assets/logo.png" alt="Octocode Awareness" width="300" />
</p>

**Shared situational awareness for AI coding agents — the workspace nervous system.**

Humans stay oriented with memory, attention, error signals, and cleanup sleep. Agents usually do not: each run rediscovers facts, races the same files, and loses handoffs in chat. `@octocodeai/octocode-awareness` gives any agent that same organ-like loop — local, inspectable, and shared across Cursor, Claude Code, Codex, Pi, and custom hosts.

```text
CLI  = control plane          Skill = when/how to use it
SQLite = memory organ         .octocode/ = capped projections
Hooks  = reflexes             You = oversight for self-improvement
```

No server. No daemon. Zero npm runtime deps for the library. One DB under the global Octocode home.

---

## Install (any agent)

Requires **Node >= 22**.

### Flow

```
Run CLI → read <AGENT_INSTRUCTIONS> → init DB → install bundled skills → attend
```

The CLI is agent-aware: running it with no arguments prints `<AGENT_INSTRUCTIONS>` with the real bundled skill paths on your machine and the exact commands to run next.

---

**Step 1 — Run the CLI. Let the agent read it.**

```bash
npx @octocodeai/octocode-awareness
```

The output contains a `<AGENT_INSTRUCTIONS>` block with:
- Real absolute paths to the two bundled skills on this machine
- The first commands to run after install
- A full command-surface map

---

**Step 2 — Init the DB and install the bundled skills.**

The `<AGENT_INSTRUCTIONS>` block shows the exact paths. Run these commands (the agent can copy-paste them directly from the CLI output):

```bash
# 1. Initialise the awareness DB
npx @octocodeai/octocode-awareness maintenance init --compact

# 2. Install the bundled skills using the paths shown in <AGENT_INSTRUCTIONS>
#    The agent reads the CLI output and installs from the paths it sees there.
#    octocode-awareness  →  full CLI operating loop (memory, locks, planning, signals, hooks)
#    octocode-skills     →  teaches the agent how to install, manage, and create skills
```

The **`octocode-awareness` skill contains ALL instructions** for using every CLI command — memory, locks, planning, coordination, signals, verification, repo context, and hooks. Once installed, the agent knows exactly how and when to call this CLI without any additional guidance.

The **`octocode-skills` skill** teaches the agent how to install, update, and manage skills going forward.

Do not install `@octocodeai/octocode-awareness` by registry name — the bundled paths shown by the CLI are the canonical source.

---

**Step 3 — Start.**

```bash
# Attend: compact start packet — profile, workboard, evidence, gaps, drive state
npx @octocodeai/octocode-awareness attend --workspace "$PWD" --compact

# Full command map (machine-readable)
npx @octocodeai/octocode-awareness schema commands --compact

# Operating loop reference
npx @octocodeai/octocode-awareness docs show full-flow
```

---

**Step 4 — Optional reflexes (hooks).** Preview first, then install:

```bash
npx @octocodeai/octocode-awareness hooks install --host <claude|codex|cursor> --project-dir . --dry-run --compact
npx @octocodeai/octocode-awareness hooks install --host <claude|codex|cursor> --project-dir . --compact
npx @octocodeai/octocode-awareness hooks check --host <claude|codex|cursor> --project-dir . --strict --compact
```

| Host | How awareness attaches |
|---|---|
| Claude Code | Skill frontmatter hooks when active; project-wide install writes `.claude/settings.json`. |
| Codex | `hooks install --host codex` writes `.codex/hooks.json`; host hooks must be enabled for the session. |
| Cursor | `hooks install --host cursor` writes `.cursor/hooks.json`; Cursor cloud supports fewer event classes, so smoke write paths. |
| Pi | No shell hook file; use in-process `wirePiAwarenessHooks(pi, { skillRoot })` or `@octocodeai/pi-extension`. |
| Custom | Import `@octocodeai/octocode-awareness` or call the CLI |

Guide: [docs/SKILLS.md](docs/SKILLS.md) · product site: [octocode.ai](https://octocode.ai)

---

## How it works

Surfaces over one store:

| Piece | Job |
|---|---|
| **CLI** (`octocode-awareness` / `npx @octocodeai/octocode-awareness`) | Execute operations: attend, plan, task, lock, signal, verify, reflect, query, inject, digest. |
| **Skill** (`skills/octocode-awareness`) | Teach the agent *when* to call the CLI (before / during / after), with recipes and hooks. |
| **Hooks** (host lifecycle) | Reflexes on the same CLI/runtime: claim, pending verify, stop gate, briefing, session capture. |
| **Skill** (`skills/octocode-skills`) | Install, update, lint, rate, and improve Agent Skills (vendored at build from repo-root `skills/octocode-skills`). |

Canonical data lives in **`~/.octocode/memory/awareness.sqlite3`** (override dir with `OCTOCODE_MEMORY_HOME`). Rows are scoped first by `workspace_path`, with optional `artifact`, `repo`, and `ref` scopes when one workspace needs finer isolation. WAL mode lets concurrent agents read/write safely.

**`<repo>/.octocode/` is not the DB.** `repo inject` publishes capped Markdown / CSV / HTML projections for humans and LLMs. `.octocode/plan/<timestamp-name>/` is the managed exception: it stores plan narrative and supporting docs while SQLite stores live task/claim state. `repo inject` preserves plan folders.

Memories, signals, and generated wiki pages are leads, not authority. User instructions, current source, tests, and fresh verification win when remembered context disagrees.

```text
Agent A ──┐
Agent B ──┼──▶  SQLite (canonical)  ──▶  plans / tasks / runs / signals / locks
Agent C ──┘              │
                         └──▶  .octocode/ wiki (projection, optional)
                                    └── AGENTS.md = map → MEMORY/GOTCHAS/LEARN/…
```

### State machine (skill + CLI + hooks)

Collaborative work uses three non-overlapping entities:

```text
Plan (objective + lead + docs)
  └── Task (reasoning + acceptance + paths + dependencies)
        └── TaskRun (one attempt; owns exact file locks and verification)
```

Agents may skip Plan/Task for a quick edit: `lock acquire` creates a standalone TaskRun. A claimed task instead reuses one linked run across hook-driven edits.

```text
                    ┌─ notify-deliver / sessionStart (hook) ─┐
                    ▼                                         │
 IDLE ──attend──▶ ATTEND ──lock acquire / pre-edit──▶ CLAIMED
   ▲                 │                                  │
   │                 │ memory/signal/refine             │ edit (agent)
   │                 ▼                                  ▼
   │              LEARN ◀──────────────────── PENDING_VERIFY
   │                 ▲                         ▲     │
   │                 │                         │     │ verify mark
   │                 │                    post-edit  │
   │                 │                         │     ▼
   │            REFLECT ◀── reflect record ── VERIFIED
   │                 │
   │                 ├── session capture / signal handoff ──▶ HAND_OFF
   │                 └── repo inject ──▶ PROJECTED (.octocode wiki)
   │                                        │
   └──────── digest / prune ◀───────────────┘
```

| State | Who drives it | Typical command / hook |
|---|---|---|
| ATTEND | Skill (before) + optional briefing hook | `attend`, `query workboard`, `signal list` / `notify-deliver` |
| CLAIMED | Skill (during) + pre-edit hook | `lock acquire` / `pre-edit.sh` |
| PENDING_VERIFY | `task submit`, or standalone post-edit/release | linked task → `VERIFY`; run → `PENDING` |
| VERIFIED | Skill (after) + stop gate | `verify mark`; `stop-verify.sh` blocks silent conclude |
| REFLECT / LEARN (bookkeep) | Skill (after) | `reflect record`, `memory record` |
| PROJECTED | Skill (after) | `repo inject` → `.octocode/AGENTS.md` + wiki |
| HAND_OFF | Session-end hook or manual | `session capture`, `refinement *`, `signal publish` |
| IDLE / clean (housekeep) | Housekeep | `maintenance digest --dry-run`, prune |

Manual CLI works without hooks. Hooks only automate the same transitions.

---

## Theory: awareness as an organ

Validated research direction (nature systems + agent memory + retrieval compression): **do not grow an endless memory chain.** Run a **Homeostatic Awareness Loop** — sense, act, learn, then metabolize.

```text
ATTEND → CHOOSE → CLAIM → ACT → VERIFY → REFLECT
   ↑                                           │
   └─ PROJECT ← PRUNE ← CONSOLIDATE ← REPLAY ←─┘
```

| Body heuristic | Awareness surface | Rule |
|---|---|---|
| Senses / interoception | `workspace status`, `.octocode/AGENTS.md` | Sense before acting. |
| Attention | `attend`, `query workboard` | Small packet > dump every doc. |
| Hippocampus | `memory recall\|record`, `reflect record` | Store only what helps a future run. |
| Prediction error | failed tests, lock conflicts, user corrections | Fall → update → get smarter. |
| Microglia / immune prune | supersession, `memory forget --dry-run` | Tag weak/stale/unsafe; prefer report over silent delete. |
| Sleep / glymphatic | `maintenance digest --dry-run` | Replay → consolidate → prune → project (**report-first**; no silent wipe). |
| Corpus / bridge | `signal *`, `refinement *`, locks | Coordinate agents without hidden chat memory. |
| Executive control | `lock acquire`, `verify mark` | Claim → act → close with evidence. |

**Biology gives heuristics, not a license to delete.** Default sleep verbs: report, group, supersede, archive; deletion is explicit policy.

**Collective identity, not a persona.** `attend` can expose derived `drive_state` / `organ_state` (goal, gaps, resource leads, team norms, who-knows-what). Do not invent a permanent agent personality — preserve shared vision, curiosity via learning gaps, and transactive memory.

**Context is circulation.** Too little → missed signals. Too much wiki/memory mass → sluggish, imprecise agents. Compact attend packets and projection budgets keep the system in shape.

Deeper map for agents: [`skills/.../references/homeostatic-loop.md`](skills/octocode-awareness/references/homeostatic-loop.md). Unshipped ideas (`sleep` command, dedicated trust gate) are marked **NOT SHIPPED** there — do not invent CLI for them.

### Research anchors

Biology is a **heuristic map**, not a delete license. The shipped loop was stress-tested against nature systems, agent-memory prior art, and retrieval/compression practice:

| Idea | What it means here | Shipped surface |
|---|---|---|
| Neuroplasticity / prediction error | Mistakes and failed checks update future behavior | `verify *`, `reflect record`, failure signatures |
| Microglia / synaptic pruning | Weak or unused connections get tagged and cleaned | supersession, `memory forget --dry-run`, digest reports |
| Sleep / glymphatic clearance | Cleanup is report-first: replay → consolidate → prune → project | `maintenance digest --dry-run`, `repo inject` |
| Attention / compression | Dumping all memory hurts; attend with a small packet | `attend --compact`, `query workboard`, projection budgets |
| Corpus / stigmergy | Agents coordinate through shared traces, not hidden chat | `signal *`, `refinement *`, locks |
| Collective drive (not persona) | Shared goal, gaps, norms, who-knows-what | derived `drive_state` / `organ_state` on `attend` |

Agent-facing organ map: [`homeostatic-loop.md`](skills/octocode-awareness/references/homeostatic-loop.md). Package docs: [WIKI.md](docs/WIKI.md), [HARNESS.md](docs/HARNESS.md), [DB.md](docs/DB.md).

---

## What users get

Ask your agent to use awareness and you should hear plain answers to:

- what prior runs already learned,
- which files are claimed,
- what messages / handoffs are waiting,
- what verification is still owed,
- what was saved for the next session.

That is human-grade situational awareness — for machines that share your repo.

---

## Features

| Feature | Commands | One-liner |
|---|---|---|
| Start packet | `attend` | Compact profile + workboard + evidence + gaps + drive/organ state. |
| Status | `workspace status` | Active plans, ready/claimed/verify tasks, runs, locks, and memory counts. |
| Plans | `plan create\|list\|show\|join\|doc\|status` | Shared objective, lead agent, members, lifecycle, and `.octocode/plan/**` documents. |
| Tasks | `task create\|list\|ready\|show\|claim\|heartbeat\|submit\|release\|depend` | Agents choose dependency-ready work and coordinate through leased claims. |
| Memory | `memory record\|recall\|forget` | Durable lessons; lexical FTS by default; `--semantic` only with `OCTOCODE_EMBED_CMD`; recall returns few rows by default — raise `--limit` when you need more; `--explain` shows the scoring. |
| File claims | `lock acquire\|wait\|release\|prune` | Visible concurrency; exit `2` = conflict. |
| Verification debt | `verify audit\|mark` | Released ≠ success until the declared check ran. |
| Signals | `signal publish\|list\|reply\|ack\|resolve\|prune` | Blockers, questions, decisions, handoffs; thread replies with `--in-reply-to`. |
| Agents | `agent register\|list` | Who is active in this workspace. |
| Handoffs | `refinement *`, `session capture` | Session continuity outside chat history; durable selectable work belongs in plan tasks. |
| Reflection | `reflect record\|mine-weakness\|export-harness` | Lessons + weakness clusters; outcomes are `worked\|partial\|failed`; three feedback targets `--fix-repo\|--fix-harness\|--fix-instructions`; record `--failure-signature` on failures or mine-weakness has nothing to cluster; harness preview is human-gated. |
| Developer review | `reflect developer-review` | Agent feedback to the human who authored the instructions (`--fix-instructions`); grouped Open/Resolved; regenerated into `.octocode/DEVELOPER_REVIEW.md`. |
| Workboard / views | `query <view>` | JSON / table / CSV / Markdown / HTML over live rows. |
| LLM Wiki | `repo inject` | Bounded `.octocode/` projections (`AGENTS`, `MEMORY`, `GOTCHAS`, `LEARN`, `BOOKMARKS`, `DEVELOPER_REVIEW`, …). `AGENTS.md` carries a Retro Files Map indexing them. Generated files can contain machine-local absolute paths — review before committing a projection. |
| Skill docs | `docs list\|show` | Catalog skill `references/*.md` (not package `docs/**`). |
| Docs drift | `docs staleness` | Flag docs lagging `edit_log` source activity; paths must match how edits were recorded (prefer absolute). |
| Metabolism | `maintenance digest\|init\|self-test` | Init, smoke, report-first cleanup. |
| Schema discovery | `schema commands\|list\|json-schema\|example\|validate` | Machine-readable command map and contracts for agents. |
| Hooks / Pi | `hooks *`, `wirePiAwarenessHooks` | Reflexes: claim, pending verify, stop gate, briefing, session capture. |
| Library | `@octocodeai/octocode-awareness` | Same runtime for host integrations. |

### CLI commands by type

Source of truth: `octocode-awareness schema commands --compact`. The groups below name every shipped CLI command and why you would reach for it.

| Type | Command | Why |
|---|---|---|
| Orientation | `attend` | Start a run with a compact packet: profile, workboard, evidence, gaps, organ state, and drive state. |
| Orientation | `workspace status` | Check DB health, locks, pending verification, and memory counts before work. |
| Orientation | `query` | Read live DB views as JSON, table, CSV, Markdown, or HTML. |
| Planning | `plan create\|list\|show\|join\|doc\|status` | Create/govern shared objectives and register narrative docs. |
| Planning | `task create\|list\|ready\|show\|claim\|heartbeat\|submit\|release\|depend` | Define, choose, lease, and complete collaborative work. |
| Memory | `memory recall` | Bring back relevant lessons before planning or editing. |
| Memory | `memory record` | Save durable lessons, decisions, gotchas, or observations for future agents. |
| Memory | `memory forget` | Remove selected stale memories; dry-run first. |
| Claims | `lock acquire` | Claim files before edits and expose conflicts with exit code `2`. |
| Claims | `lock wait` | Wait for existing file locks without taking ownership. |
| Claims | `lock release` | Close file claims as `SUCCESS`, `FAILED`, or `PENDING`. |
| Claims | `lock prune` | Clean expired or stale lock rows without marking work successful. |
| Verification | `verify audit` | Find pending or stale verification debt before finishing. |
| Verification | `verify mark` | Record that the declared check actually ran. |
| Signals | `signal list` | Read blockers, questions, requests, decisions, handoffs, and FYIs. |
| Signals | `signal publish` | Send a new blocker, question, request, handoff, decision, or FYI. |
| Signals | `signal reply` | Continue an existing signal thread. |
| Signals | `signal ack` | Mark handled signals as read. |
| Signals | `signal resolve` | Close handled signals or whole threads. |
| Signals | `signal prune` | Remove resolved, old, or selected signals; dry-run first. |
| Agents | `agent register` | Register or refresh an agent identity in the shared workspace. |
| Agents | `agent list` | See known agents in the current scope. |
| Handoffs | `refinement get` | Read unfinished handoffs or follow-up work. |
| Handoffs | `refinement set` | Save work state or next-step context for another run. |
| Handoffs | `refinement delete` | Delete stale refinement rows; dry-run first. |
| Handoffs | `session capture` | Capture unresolved session state from locks and the dirty git tree. |
| Reflection | `reflect record` | Record outcome, lesson, and optional failure signature after work. |
| Reflection | `reflect mine-weakness` | Find recurring failure clusters worth fixing. |
| Reflection | `reflect export-harness` | Preview candidate harness guidance from memories for human review. |
| Reflection | `reflect developer-review` | Read agent feedback on the instructions themselves (from `reflect record --fix-instructions`); feeds `.octocode/DEVELOPER_REVIEW.md`. |
| Repo context | `repo inject` | Generate bounded `.octocode/` projections for agents and humans. |
| Skill docs | `docs list` | List bundled skill reference docs in `references/*.md`. |
| Skill docs | `docs show` | Display one bundled skill reference by name. |
| Skill docs | `docs staleness` | Flag docs that may lag recorded source edits. |
| Maintenance | `maintenance digest` | Preview or run report-first memory, signal, and refinement cleanup. |
| Maintenance | `maintenance init` | Initialize the awareness database. |
| Maintenance | `maintenance self-test` | Run in-memory DB smoke checks. |
| Hooks | `hooks install` | Install awareness-owned hook config after preview. |
| Hooks | `hooks check` | Check installed hook config and detect drift. |
| Hooks | `hooks remove` | Remove awareness-owned hook config. |
| Hooks | `hook run` | Internal hook dispatcher used by wrapper scripts. |
| Schema | `schema commands` | Print the command-to-schema map for agents and tooling. |
| Schema | `schema list` | Print available schema names only. |
| Schema | `schema json-schema` | Print one JSON schema contract. |
| Schema | `schema example` | Print example JSON for one schema. |
| Schema | `schema validate` | Validate a JSON payload against a schema. |

Code search is **not** bundled here — use `npx octocode search …` or Octocode MCP (see skill `references/octocode.md`). Skill install/update/lint workflows also use `npx octocode skill ...`, pointed at bundled/local skill paths.

---

## Everyday loop

```bash
# Attend
npx @octocodeai/octocode-awareness attend --workspace "$PWD" --query "current task" --compact
npx @octocodeai/octocode-awareness query workboard --workspace "$PWD" --format table

# Collaborative flow: choose a ready task, then use its run id for submit/verify
npx @octocodeai/octocode-awareness task ready --plan-id plan_123 --compact
npx @octocodeai/octocode-awareness task claim --task-id task_123 \
  --agent-id "$OCTOCODE_AGENT_ID" --compact
# hooks attach edits to the claimed run; when ready:
npx @octocodeai/octocode-awareness task submit --task-id task_123 --run-id run_123 \
  --agent-id "$OCTOCODE_AGENT_ID" --message "tests passed" --compact
npx @octocodeai/octocode-awareness verify mark --run-id run_123 \
  --agent-id "$OCTOCODE_AGENT_ID" --message "tests passed" --compact

# Quick-edit flow: no plan/task required
npx @octocodeai/octocode-awareness lock acquire --agent-id "$OCTOCODE_AGENT_ID" \
  --workspace "$PWD" --target-file src/file.ts --rationale "why" --test-plan "yarn test" --compact
# …edit…
# Hooks often release as PENDING after edits; do the same manually when hooks are off:
npx @octocodeai/octocode-awareness lock release --agent-id "$OCTOCODE_AGENT_ID" \
  --workspace "$PWD" --target-file src/file.ts --status PENDING --compact
npx @octocodeai/octocode-awareness verify mark --agent-id "$OCTOCODE_AGENT_ID" \
  --workspace "$PWD" --all-pending --message "tests passed" --compact
npx @octocodeai/octocode-awareness lock release --agent-id "$OCTOCODE_AGENT_ID" \
  --workspace "$PWD" --target-file src/file.ts --status SUCCESS --verified --compact

# Learn → project → housekeep
npx @octocodeai/octocode-awareness reflect record --agent-id "$OCTOCODE_AGENT_ID" \
  --workspace "$PWD" --task "…" --outcome worked --lesson "…" --compact
npx @octocodeai/octocode-awareness repo inject --workspace "$PWD" --compact
npx @octocodeai/octocode-awareness maintenance digest --workspace "$PWD" --dry-run --compact
```

Bloat / prune playbook (report-first): see workspace `.octocode/rfc/awareness-audit-hardening/DIGEST-PLAN.md` when present — prefer `attend`/`query`, then `memory forget --dry-run` + `repo inject`; digest alone may not shrink memories.

Self-improvement stays supervised: `reflect export-harness` previews guidance; humans (or gated harness apply) merge it. See [docs/HARNESS.md](docs/HARNESS.md).

Exit codes: **0** success, **1** usage/validation error, **2** lock conflict / wait timeout / incomplete release / schema-validation failure. Most DB commands print a JSON envelope with `ok` and `db_path`; unknown flags hard-error with the full `known_flags` list. `docs show` without `--compact` prints raw markdown.

---

## Two `.octocode` locations

| Location | Scope | Contents |
|---|---|---|
| `~/.octocode/` (global home) | Machine / user | Config + **canonical** `memory/awareness.sqlite3` |
| `<repo>/.octocode/` | One workspace | Generated wiki/CSV/HTML plus managed `.octocode/plan/**` narrative docs |

Rule: **global home stores; repo folder publishes.** Stale projection → fix facts in the DB → `repo inject` again.

| Env | Purpose |
|---|---|
| `OCTOCODE_HOME` | Broader Octocode home / config |
| `OCTOCODE_MEMORY_HOME` | Directory for `awareness.sqlite3`. Per-call override: global `--db <path>` flag — honored by every command except `hook run`, which reads only this env var |
| `OCTOCODE_AGENT_ID` | Stable agent identity |
| `OCTOCODE_EMBED_CMD` | Optional host embedder for semantic recall |
| `OCTOCODE_NO_VERIFY_GATE=1` | Disable stop-time verify block |
| `OCTOCODE_ALLOW_HARNESS_APPLY=1` | Allow skill self-edits (non-main + guard) |

---

## Package boundaries

**Owns:** plans, tasks, task runs/claims/dependencies, memory, locks, signals, refinements, verify, reflection, sessions, hooks runtime, CLI, skill sources, Pi awareness bridge.

**Does not own:** Octocode research tools / MCP brain, `octocode` skill installer packaging, Pi system prompt, `@octocodeai/config` env loading.

---

## Docs & build

| Doc | For |
|---|---|
| [docs/README.md](docs/README.md) | Feature → doc map |
| [docs/HOW_IT_WORKS.md](docs/HOW_IT_WORKS.md) | CLI + bundled skills + hooks + shared-store concept map |
| [docs/SKILLS.md](docs/SKILLS.md) | User / agent install recipes |
| [docs/DB.md](docs/DB.md) | Schema & entities |
| [docs/WIKI.md](docs/WIKI.md) | Projections & workboard |
| [docs/LOCKS.md](docs/LOCKS.md) | File claims, execution runs & verification |
| [docs/MEMORY_NAVIGATION.md](docs/MEMORY_NAVIGATION.md) | Attend packet & active memory routing |
| [docs/HOOKS.md](docs/HOOKS.md) | Host hooks & Pi bridge |
| [docs/HARNESS.md](docs/HARNESS.md) / [REFLECTION.md](docs/REFLECTION.md) | Self-improvement loop |
| [skills/octocode-awareness/SKILL.md](skills/octocode-awareness/SKILL.md) | Agent operating map |

```bash
yarn workspace @octocodeai/octocode-awareness build
yarn workspace @octocodeai/octocode-awareness test
yarn workspace @octocodeai/octocode-awareness verify
```

Edit skill sources only under `packages/octocode-awareness/skills/octocode-awareness` and repo-root `skills/octocode-skills`. Build vendors `octocode-skills` into `packages/octocode-awareness/skills/octocode-skills` (gitignored) and mirrors into `dist/skills/` + `.agents/skills/` — do not hand-edit those mirrors.
