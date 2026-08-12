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
| [octocode-graph-eval](./octocode-graph-eval/) | Did the change help? — loop & graph-of-loops evals, sensors, ACCEPT/REVERT, KPI contracts, suites, held-out, TDD-first |
| [octocode-subagent](./octocode-subagent/) | Spawn / Task / A2A / challenge techniques **or** local Ollama sealed-packet offload |
| [octocode-documentation](./octocode-documentation/) | Write/update docs — README, runbooks, CONTRIBUTING, ADRs, Diátaxis, agent-facing docs |
| [octocode-roast](./octocode-roast/) | Blunt evidence-backed critique — smells, debt ranking, autopsy, redemption |
| [octocode-prompt-optimizer](./octocode-prompt-optimizer/) | Sharpen prompts/skills/schemas/handoffs — clearer, safer, cheaper, measurable |
| [octocode-skills](./octocode-skills/) | Skill lifecycle — discover, review, create, install, sync `SKILL.md` folders |
| [octocode-chrome-devtools](./octocode-chrome-devtools/) | Live browser CDP evidence — network, console, perf, DOM, HAR, auth-gated |
| [octocode-scraping](./octocode-scraping/) | Public web → local cited corpus; keyless first; blocked/thin recovery |
| [octocode-mannequin](./octocode-mannequin/) | Anatomical 3D skeleton/manikin — pose, ROM clamps, viewer, WebMCP drive |

## Explanations

### octocode-research

Primary technical research skill. Use when you need **proof from code/repos** before claiming how something works, what’s broken, or what to change. Routes local + GitHub/npm evidence; pairs with LSP when symbol identity matters. Prefer this over brainstorming when the question is factual about an existing system.

### octocode-brainstorming

Disciplined idea exploration **before** commitment. Generates options, stress-tests “is this worth building?”, maps adjacent solutions, and ends in a clear verdict (Build RFC / Prototype / Narrow / Park). Hand off to research for evidence and to RFC once the decision is made.

### octocode-rfc-generator

Turns a consequential choice into a durable decision artifact: RFC, architecture proposal, migration/rollout plan, or measurable implementation contract. Use when coding would lock you into the wrong path without an explicit decision.

### octocode-graph-eval

Measurement and keep/discard — for a single agent loop or a graph of loops (multi-agent workflow). Defines goal→KPI contracts, feedback-loop prerequisites (runnable sensor + numeric target + budget before iterating), suites, graders, held-out checks, and ACCEPT/REVERT. Covers loop engineering (don't-stop-till-done optimization against a sensor) and graph evals (primary KPI at the graph boundary, per-node sensors, attribution by bisection, strengthen verifiers before adding nodes). Also covers TDD failing-case-first; `eval-eval.mjs --batch <dir>` grades an answer set in one command. Use whenever “it feels better” is not enough.

### octocode-subagent

General **multi-agent orchestration** for host workers, Task/subagents, specialist handoffs, A2A peers, **and** frugal local Ollama offload. Decides spawn vs solo vs Ollama; decomposes work; picks topology/model tier; writes sealed packets; coordinates ownership; recovers failures; synthesizes. Ollama path: parent keeps tools/verify/writes; local model does summarize/extract/classify/translate/draft/check/vision/map-reduce (`references/local-ollama.md`). Measuring keep/discard → **octocode-graph-eval**.

### octocode-documentation

Produces or updates documentation deliverables (README, API docs, runbooks, troubleshooting, CONTRIBUTING, changelog, onboarding, `AGENTS.md` / `CLAUDE.md`, ADRs, Diátaxis, architecture/migration guides). Evidence-backed and gate-heavy. Pure code research with no docs output → research; authoring a skill folder → **octocode-skills**.

### octocode-roast

Constructive but blunt critique with evidence: correctness, security, performance, design, testing, maintainability. Ranks cleanup debt, runs smell inventory/autopsy, and suggests redemption paths for a diff or hot path. Polite PR review → research.

### octocode-prompt-optimizer

Improves instruction surfaces — prompts, skill text, tool schemas, policies, handoffs — for clarity, safety, trigger quality, context cost, and measurability. Optimize behavior, not prose aesthetics.

### octocode-skills

Meta-skill for Agent Skill folders: discover, compare, inspect, review, create, improve, repair, install, sync, rate. Owns description-tuning, skill-review rules, and `skill-sync` to vendor destinations.

### octocode-chrome-devtools

Browser debugging that needs **DevTools-grade** evidence via Chrome DevTools Protocol (network, console, performance, DOM/CSS, screenshots/PDF, security, storage, auth-gated pages). Prefer lighter browser openers when you only need to load a URL. Static crawl/bulk extract → **octocode-scraping**.

### octocode-scraping

Public web → local cited corpus: scrape/crawl, extract tables/fields, diagnose blocked/thin pages, answer from saved sessions. Keyless first; ask before hosted spend. Live clicks/HAR/perf → **octocode-chrome-devtools**.

### octocode-mannequin

Anatomical 3D humanoid manikin: pose commands, ROM clamps, walk/run/dance/backflip sequences, Three.js viewer, WebMCP agent drive. Not for general scenes, physics/ragdoll, IK, or mocap.

## Suggested routes

```text
Question about code?     → research
Idea / is it worth it?   → brainstorming → (rfc | research | park)
Need a design contract?  → rfc-generator
Did the change help?     → graph-eval
Loop until a target?     → graph-eval (sensor + target + budget first)
Spawn cloud workers?     → subagent
Save tokens via Ollama?  → subagent (local-ollama.md)
Write docs?              → documentation
Critique code?           → roast
Tune a prompt/skill?     → prompt-optimizer
Change a skill folder?   → skills
Debug in Chrome?         → chrome-devtools
Scrape / build corpus?   → scraping
Pose a manikin?          → mannequin
```

## Layout convention

Each skill folder typically includes:

- `SKILL.md` — lobby (trigger `description`, gates, progressive routes)
- `README.md` — human overview / install
- `references/` — on-demand detail (load only what the step needs)
- `scripts/` — deterministic helpers (when present)
- `evals/` — permanent suites (when present); temp under `.octocode/`
