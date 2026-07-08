# Octocode Awareness

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

**1. Run the CLI** (discover commands; no global install required):

```bash
npx @octocodeai/octocode-awareness
npx @octocodeai/octocode-awareness maintenance init --compact
```

**2. Install the Agent Skills** so the host knows the operating loop (awareness) and how to install/update/lint skills (`octocode-skills`, bundled beside awareness on npm):

```bash
# From npm / marketplace
npx octocode skill --name octocode-awareness --platform common
npx octocode skill --name octocode-skills --platform common

# Or from a local / bundled skill folder (monorepo after build, or npm package skills/)
npx octocode skill --add --path packages/octocode-awareness/skills/octocode-awareness --platform common
npx octocode skill --add --path packages/octocode-awareness/skills/octocode-skills --platform common --force
```

`--platform common` installs into `~/.agents/skills` (shared discovery). For host-specific destinations use `--platform claude`, `cursor`, `codex`, `pi`, or `all`. After install, tell the agent: *use octocode-awareness before planning or editing this repo; use octocode-skills to install/update/lint skills.*

**3. Optional reflexes (hooks)** — preview, then install:

```bash
npx @octocodeai/octocode-awareness hooks install --host codex --project-dir . --dry-run --compact
npx @octocodeai/octocode-awareness hooks install --host codex --project-dir . --compact
npx @octocodeai/octocode-awareness hooks check --host codex --project-dir . --strict --compact
```

| Host | How awareness attaches |
|---|---|
| Claude Code | Skill frontmatter hooks + CLI |
| Codex / Cursor | `hooks install` → `.codex/hooks.json` / `.cursor/hooks.json` + CLI. Host enablement varies — read [docs/HOOKS.md](docs/HOOKS.md) and skill `references/hooks.md` before assuming write-time enforcement. |
| Pi | In-process `wirePiAwarenessHooks(pi)` + skill |
| Custom | Import `@octocodeai/octocode-awareness` or call the CLI |

Everyday first commands after install:

```bash
export OCTOCODE_AGENT_ID="${OCTOCODE_AGENT_ID:-my-agent}"
npx @octocodeai/octocode-awareness attend --workspace "$PWD" --query "current task" --compact
npx @octocodeai/octocode-awareness schema commands --compact
npx @octocodeai/octocode-awareness docs list --compact   # skill reference catalog
```

**Local-first rule** (the same rule the skill teaches): prefer an installed CLI — `node scripts/awareness.mjs` inside an installed skill, or `node packages/octocode-awareness/dist/bin/awareness.js` in this monorepo after a build. `npx @octocodeai/octocode-awareness` is the fallback when no local copy exists.

Guide: [docs/SKILLS.md](docs/SKILLS.md) · product site: [octocode.ai](https://octocode.ai)

---

## How it works

Surfaces over one store:

| Piece | Job |
|---|---|
| **CLI** (`octocode-awareness` / `npx @octocodeai/octocode-awareness`) | Execute operations: attend, lock, signal, verify, reflect, query, inject, digest. |
| **Skill** (`skills/octocode-awareness`) | Teach the agent *when* to call the CLI (before / during / after), with recipes and hooks. |
| **Hooks** (host lifecycle) | Reflexes on the same CLI/runtime: claim, pending verify, stop gate, briefing, session capture. |
| **Skill** (`skills/octocode-skills`) | Install, update, lint, rate, and improve Agent Skills (vendored at build from repo-root `skills/octocode-skills`). |

Canonical data lives in **`~/.octocode/memory/awareness.sqlite3`** (override dir with `OCTOCODE_MEMORY_HOME`). Workspace path scopes rows so projects stay isolated. WAL mode lets concurrent agents read/write safely.

**`<repo>/.octocode/` is not the DB.** `repo inject` publishes capped Markdown / CSV / HTML projections for humans and LLMs. `.octocode/AGENTS.md` is the digested awareness map; root `AGENTS.md` should point there. Regenerate projections; do not treat them as source of truth.

```text
Agent A ──┐
Agent B ──┼──▶  SQLite (canonical)  ──▶  attend / workboard / signals / locks
Agent C ──┘              │
                         └──▶  .octocode/ wiki (projection, optional)
                                    └── AGENTS.md = map → MEMORY/GOTCHAS/LEARN/…
```

### State machine (skill + CLI + hooks)

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
| PENDING_VERIFY | Post-edit hook (or manual) | `post-edit.sh` → task stays pending |
| VERIFIED | Skill (after) + stop gate | `verify mark`; `stop-verify.sh` blocks silent conclude |
| REFLECT / LEARN | Skill (after) | `reflect record`, `memory record` |
| PROJECTED | Skill (after) | `repo inject` → `.octocode/AGENTS.md` + wiki |
| HAND_OFF | Session-end hook or manual | `session capture`, `refinement *`, `signal publish` |
| IDLE / clean | Housekeep | `maintenance digest --dry-run`, prune |

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
| Status | `workspace status` | DB health, locks, pending verify, memory counts. |
| Memory | `memory record\|recall\|forget` | Durable lessons; lexical FTS by default; `--semantic` only with `OCTOCODE_EMBED_CMD`; recall returns few rows by default — raise `--limit` when you need more; `--explain` shows the scoring. |
| File claims | `lock acquire\|wait\|release\|prune` | Visible concurrency; exit `2` = conflict. |
| Verification debt | `verify audit\|mark` | Released ≠ success until the declared check ran. |
| Signals | `signal publish\|list\|reply\|ack\|resolve\|prune` | Blockers, questions, decisions, handoffs; thread replies with `--in-reply-to`. |
| Agents | `agent register\|list` | Who is active in this workspace. |
| Handoffs | `refinement *`, `session capture` | Backlog outside chat history. Session-captured handoffs are `quality=handoff`; read them back with `refinement get --include-handoffs`. |
| Reflection | `reflect record\|mine-weakness\|export-harness` | Lessons + weakness clusters; outcomes are `worked\|partial\|failed`; record `--failure-signature` on failures or mine-weakness has nothing to cluster; harness preview is human-gated. |
| Workboard / views | `query <view>` | JSON / table / CSV / Markdown / HTML over live rows. |
| LLM Wiki | `repo inject` | Bounded `.octocode/` projections (`AGENTS`, `MEMORY`, `GOTCHAS`, `LEARN`, `BOOKMARKS`, …). Generated files can contain machine-local absolute paths — review before committing a projection. |
| Skill docs | `docs list\|show` | Catalog skill `references/*.md` (not package `docs/**`). |
| Docs drift | `docs staleness` | Flag docs lagging `edit_log` source activity; paths must match how edits were recorded (prefer absolute). |
| Metabolism | `maintenance digest\|init\|self-test` | Init, smoke, report-first cleanup. |
| Schema discovery | `schema commands\|list\|json-schema\|example\|validate` | Machine-readable command map and contracts for agents. |
| Hooks / Pi | `hooks *`, `wirePiAwarenessHooks` | Reflexes: claim, pending verify, stop gate, briefing, session capture. |
| Library | `@octocodeai/octocode-awareness` | Same runtime for host integrations. |

Code search is **not** bundled here — use `npx octocode search …` or Octocode MCP (see skill `references/octocode.md`).

---

## Everyday loop

```bash
# Attend
npx @octocodeai/octocode-awareness attend --workspace "$PWD" --query "current task" --compact
npx @octocodeai/octocode-awareness query workboard --workspace "$PWD" --format table

# Claim → edit → verify
npx @octocodeai/octocode-awareness lock acquire --agent-id "$OCTOCODE_AGENT_ID" \
  --workspace "$PWD" --target-file src/file.ts --rationale "why" --test-plan "yarn test" --compact
# …edit…
npx @octocodeai/octocode-awareness verify mark --agent-id "$OCTOCODE_AGENT_ID" \
  --workspace "$PWD" --all-pending --message "tests passed" --compact
npx @octocodeai/octocode-awareness lock release --agent-id "$OCTOCODE_AGENT_ID" --workspace "$PWD" --compact

# Learn → project → housekeep
npx @octocodeai/octocode-awareness reflect record --agent-id "$OCTOCODE_AGENT_ID" \
  --workspace "$PWD" --task "…" --outcome worked --lesson "…" --compact
npx @octocodeai/octocode-awareness repo inject --workspace "$PWD" --compact
npx @octocodeai/octocode-awareness maintenance digest --workspace "$PWD" --dry-run --compact
```

Self-improvement stays supervised: `reflect export-harness` previews guidance; humans (or gated harness apply) merge it. See [docs/HARNESS.md](docs/HARNESS.md).

Exit codes: **0** success, **1** usage/validation error, **2** lock conflict / wait timeout / incomplete release / schema-validation failure. Most DB commands print a JSON envelope with `ok` and `db_path`; unknown flags hard-error with the full `known_flags` list. `docs show` without `--compact` prints raw markdown.

---

## Two `.octocode` locations

| Location | Scope | Contents |
|---|---|---|
| `~/.octocode/` (global home) | Machine / user | Config + **canonical** `memory/awareness.sqlite3` |
| `<repo>/.octocode/` | One workspace | Generated wiki / CSV / HTML from `repo inject` |

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

**Owns:** memory, locks, signals, refinements, verify, reflection, sessions, hooks runtime, CLI, skill sources, Pi awareness bridge.

**Does not own:** Octocode research tools / MCP brain, `octocode` skill installer packaging, Pi system prompt, `@octocodeai/config` env loading.

---

## Docs & build

| Doc | For |
|---|---|
| [docs/README.md](docs/README.md) | Feature → doc map |
| [docs/SKILLS.md](docs/SKILLS.md) | User / agent install recipes |
| [docs/DB.md](docs/DB.md) | Schema & entities |
| [docs/WIKI.md](docs/WIKI.md) | Projections & workboard |
| [docs/LOCKS.md](docs/LOCKS.md) | File claims, task states & verification |
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
