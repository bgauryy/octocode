# Octocode skills

Canonical Agent Skills for this monorepo. Each folder is a `SKILL.md` pack; vendor installs (`~/.claude`, `~/.cursor`, `~/.agents`, `~/.codex`, and project `.agents` / `.cursor` / `.claude`) should **symlink here**.

Source of truth: this `skills/` directory in the repository checkout.

Sync / install / review: use **`octocode-skills`** (`scripts/skill-sync.mjs`, `scripts/skill-review.mjs`).

## Workspace output contract

Every skill keeps chat-only results in chat. When a skill creates a new artifact without an approved destination, durable output belongs under `<workspace>/.octocode/<skill-name>/` and scratch/run data under `<workspace>/.octocode/tmp/<skill-name>/`. Existing specialized namespaces such as `.octocode/rfc/`, `.octocode/brainstorming/`, `.octocode/worker/`, `.octocode/tmp/scrape/`, and `.octocode/tmp/chrome-devtools/` remain valid. Requested code/docs/source edits, skill installations, symlinks, and configuration are target mutations rather than generated artifacts and stay at their explicitly approved paths. Skills must fail clearly when the workspace output root is unwritable; they must not redirect artifacts to a user-level Octocode home.

## Catalog

| Skill | What it is |
|---|---|
| [octocode-research](./octocode-research/) | Evidence before conclusions — find, explain, diagnose, review diffs, smallest verified fix |
| [octocode-code-graph](./octocode-code-graph/) | Repository dependency topology — cycles, paths, layering, reachability, impact, verified dead-code candidates |
| [octocode-brainstorming](./octocode-brainstorming/) | Explore ideas before building — options, worth-building, Build / Prototype / Narrow / Park |
| [octocode-rfc-generator](./octocode-rfc-generator/) | Decision before coding — RFC, design, migration, rollout, measurable contract |
| [octocode-eval-benchmark](./octocode-eval-benchmark/) | Did the change help? — loop & graph-of-loops evals, sensors, ACCEPT/REVERT, KPI contracts, suites, held-out, TDD-first |
| [octocode-subagent](./octocode-subagent/) | Spawn / Task / A2A / challenge techniques **or** local Ollama sealed-packet offload |
| [octocode-documentation](./octocode-documentation/) | Write/update docs — README, runbooks, CONTRIBUTING, ADRs, Diátaxis, agent-facing docs — plus the full Google developer documentation style guide and a Markdown style linter |
| [octocode-roast](./octocode-roast/) | Blunt evidence-backed critique — smells, debt ranking, autopsy, redemption |
| [octocode-prompt-optimizer](./octocode-prompt-optimizer/) | Sharpen prompts/skills/schemas/handoffs — clearer, safer, cheaper, measurable |
| [octocode-skills](./octocode-skills/) | Skill lifecycle — discover, review, create, install, sync `SKILL.md` folders |
| [octocode-chrome-devtools](./octocode-chrome-devtools/) | Live browser CDP evidence — network, console, perf, DOM, HAR, auth-gated |
| [octocode-scraping](./octocode-scraping/) | Public web → local cited corpus; keyless first; blocked/thin recovery |

## Explanations

### octocode-research

Primary technical research skill. Use when you need **proof from code/repos** before claiming how something works, what’s broken, or what to change. Routes local + GitHub/npm evidence; pairs with LSP when symbol identity matters. Prefer this over brainstorming when the question is factual about an existing system.

### octocode-code-graph

Turns repository file topology into ranked, falsifiable code findings. Use it for dependency cycles, shortest import paths, fan-in/out, layering, reachability, change impact, and dead-code candidates. It combines `localAnalyzeGraph` with exact imports, AST shape, LSP identity, and runnable checks; graph output alone never proves a defect or safe deletion.

### octocode-brainstorming

Disciplined idea exploration **before** commitment. Generates options, stress-tests “is this worth building?”, maps adjacent solutions, and ends in a clear verdict (Build RFC / Prototype / Narrow / Park). Hand off to research for evidence and to RFC once the decision is made.

### octocode-rfc-generator

Turns a consequential choice into a durable decision artifact: RFC, architecture proposal, migration/rollout plan, or measurable implementation contract. Use when coding would lock you into the wrong path without an explicit decision.

### octocode-eval-benchmark

Smart eval and benchmark design for a single agent, a change, or a multi-agent workflow. Defines goal→KPI contracts, runnable sensors, suites, graders, held-out checks, guardrails, and ACCEPT/REVERT. Covers benchmark contamination, deterministic and model graders, TDD failing-case-first, don't-stop-till-done optimization, and per-node attribution for multi-agent workflows. `eval-skill.mjs --batch <dir>` grades an answer set in one command. Use whenever “it feels better” is not enough.

### octocode-subagent

General **multi-agent orchestration** for host workers, Task/subagents, specialist handoffs, A2A peers, **and** frugal local Ollama offload. Decides spawn vs solo vs Ollama; decomposes work; picks topology/model tier; writes sealed packets; coordinates ownership; recovers failures; synthesizes. Ollama path: parent keeps tools/verify/writes; local model does summarize/extract/classify/translate/draft/check/vision/map-reduce (`references/local-ollama.md`). Measuring keep/discard → **octocode-eval-benchmark**.

### octocode-documentation

Produces or updates documentation deliverables (README, API docs, runbooks, troubleshooting, CONTRIBUTING, changelog, onboarding, `AGENTS.md` / `CLAUDE.md`, ADRs, Diátaxis, architecture/migration guides). Evidence-backed and gate-heavy. Also owns **editorial style**: the complete [Google developer documentation style guide](https://developers.google.com/style) is split across `references/style-*.md` (voice, grammar, words, global/inclusive, structure, blocks, format, punctuation, numbers, code, UI, links, claims, API reference), with every guide page mapped in `references/style-sources.md`, the 598-entry word list as data in `assets/google-word-list.tsv`, and `scripts/style-lint.mjs` for deterministic Markdown checks. Pure code research with no docs output → research; authoring a skill folder → **octocode-skills**.

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

## Suggested routes

```text
Question about code?     → research
Dependency graph issue?  → code-graph
Idea / is it worth it?   → brainstorming → (rfc | research | park)
Need a design contract?  → rfc-generator
Did the change help?     → eval-benchmark
Design an eval/bench?    → eval-benchmark
Loop until a target?     → eval-benchmark (sensor + target + budget first)
Spawn cloud workers?     → subagent
Save tokens via Ollama?  → subagent (local-ollama.md)
Write docs?              → documentation
Critique code?           → roast
Tune a prompt/skill?     → prompt-optimizer
Change a skill folder?   → skills
Debug in Chrome?         → chrome-devtools
Scrape / build corpus?   → scraping
```

## Layout convention

Each skill folder typically includes:

- `SKILL.md` — lobby (trigger `description`, gates, progressive routes)
- `README.md` — human overview / install
- `references/` — on-demand detail (load only what the step needs)
- `scripts/` — deterministic helpers (when present)
