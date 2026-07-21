# Octocode skills

Canonical Agent Skills for this monorepo. Each folder is a `SKILL.md` pack; vendor installs (`~/.claude`, `~/.cursor`, `~/.agents`, `~/.codex`, and project `.agents` / `.cursor` / `.claude`) should **symlink here**.

Source of truth: `/Users/guybary/Documents/code/octocode/octocode/skills/`

Sync / install / review: use **`octocode-skills`** (`scripts/skill-sync.mjs`, `scripts/skill-review.mjs`).

## Catalog

| Skill | What it is |
|---|---|
| [octocode-research](./octocode-research/) | Evidence before conclusions — find, explain, diagnose, review diffs, smallest verified fix |
| [octocode-brainstorming](./octocode-brainstorming/) | Explore ideas before building — options, worth-building, Build / Prototype / Narrow / Park |
| [octocode-rfc-generator](./octocode-rfc-generator/) | Decision before coding — RFC, design, migration, rollout, measurable contract |
| [octocode-eval](./octocode-eval/) | Did the change help? — ACCEPT/REVERT, KPI contracts, suites, held-out, TDD-first |
| [octocode-awareness](./octocode-awareness/) | Shared-repo coordination — collisions, handoffs, verification debt, wiki, hooks |
| [octocode-subagent](./octocode-subagent/) | Spawn / parallel host workers — topology, packets, ownership, synthesize |
| [octocode-orchestrator-local-worker](./octocode-orchestrator-local-worker/) | Cloud keeps judgment; local Ollama burns tokens — summarize/extract/… sealed packets |
| [octocode-documentation](./octocode-documentation/) | Write/update docs — README, AGENTS/CLAUDE, ADRs, Diátaxis, agent-facing docs |
| [octocode-roast](./octocode-roast/) | Blunt evidence-backed critique — smells, debt ranking, redemption paths |
| [octocode-prompt-optimizer](./octocode-prompt-optimizer/) | Sharpen prompts/skills/schemas/handoffs — clearer, safer, cheaper, measurable |
| [octocode-skills](./octocode-skills/) | Skill lifecycle — discover, review, create, install, sync `SKILL.md` folders |
| [octocode-chrome-devtools](./octocode-chrome-devtools/) | Browser evidence via CDP — network, console, perf, DOM, auth-gated pages |

## Explanations

### octocode-research

Primary technical research skill. Use when you need **proof from code/repos** before claiming how something works, what’s broken, or what to change. Routes local + GitHub/npm evidence; pairs with LSP when symbol identity matters. Prefer this over brainstorming when the question is factual about an existing system.

### octocode-brainstorming

Disciplined idea exploration **before** commitment. Generates options, stress-tests “is this worth building?”, maps adjacent solutions, and ends in a clear verdict (Build RFC / Prototype / Narrow / Park). Hand off to research for evidence and to RFC once the decision is made.

### octocode-rfc-generator

Turns a consequential choice into a durable decision artifact: RFC, architecture proposal, migration/rollout plan, or measurable implementation contract. Use when coding would lock you into the wrong path without an explicit decision.

### octocode-eval

Measurement and keep/discard. Defines goal→KPI contracts, suites, graders, held-out checks, and ACCEPT/REVERT. Also covers TDD failing-case-first for behavior changes. Use whenever “it feels better” is not enough.

### octocode-awareness

Coordination layer for shared repos (and solo work across sessions). Collision avoidance, handoff packets, verification debt, durable memory/wiki, hooks setup/debug, and repo learning before you edit. Complements research; does not replace it.

### octocode-subagent

General **multi-agent orchestration** for host workers, Task/subagents, specialist handoffs, and A2A peers. Decides spawn vs solo, decomposes work, picks topology/model tier, writes sealed packets, coordinates ownership, recovers failures, synthesizes. **Not** for local Ollama one-shots — see local-worker.

### octocode-orchestrator-local-worker

**Frugal offload:** cloud agent keeps tools, fetch, verify, and writes; local Ollama runs sealed packets for low-risk summarize / extract / classify / translate / draft / checklist / vision / map-reduce. Includes health/worker scripts and evals. Parallel cloud workers stay on **octocode-subagent**.

### octocode-documentation

Produces or updates documentation deliverables (README, API docs, runbooks, `AGENTS.md` / `CLAUDE.md`, ADRs, Diátaxis). Evidence-backed and gate-heavy. Pure code research with no docs output → research; authoring a skill folder → **octocode-skills**.

### octocode-roast

Constructive but blunt critique with evidence: correctness, security, performance, design, testing, maintainability. Ranks cleanup debt and suggests redemption paths for a diff or hot path.

### octocode-prompt-optimizer

Improves instruction surfaces — prompts, skill text, tool schemas, policies, handoffs — for clarity, safety, trigger quality, context cost, and measurability. Optimize behavior, not prose aesthetics.

### octocode-skills

Meta-skill for Agent Skill folders: discover, compare, inspect, review, create, improve, repair, install, sync, rate. Owns description-tuning, skill-review rules, and `skill-sync` to vendor destinations.

### octocode-chrome-devtools

Browser debugging that needs **DevTools-grade** evidence via Chrome DevTools Protocol (network, console, performance, DOM/CSS, screenshots/PDF, security, storage, auth-gated pages). Prefer lighter browser openers when you only need to load a URL.

## Suggested routes

```text
Question about code?     → research
Idea / is it worth it?   → brainstorming → (rfc | research | park)
Need a design contract?  → rfc-generator
Did the change help?     → eval
Shared-repo collisions?  → awareness
Spawn cloud workers?     → subagent
Save tokens via Ollama?  → orchestrator-local-worker
Write docs?              → documentation
Critique code?           → roast
Tune a prompt/skill?     → prompt-optimizer
Change a skill folder?   → skills
Debug in Chrome?         → chrome-devtools
```

## Layout convention

Each skill folder typically includes:

- `SKILL.md` — lobby (trigger `description`, gates, progressive routes)
- `README.md` — human overview / install
- `references/` — on-demand detail (load only what the step needs)
- `scripts/` — deterministic helpers (when present)
- `evals/` — permanent suites (when present); temp under `.octocode/`
