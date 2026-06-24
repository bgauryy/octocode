# OctoCode Skills

Specialized AI agent skills extending OctoCode. **9 skills** live under `skills/`. They are user-facing playbooks: each one teaches an agent when to load extra knowledge, which evidence to gather, how to keep the work bounded, and what kind of answer or artifact to return.

Use this README as the map. Match your situation to the capability or trigger, not the folder name.

---

## Why use these skills?

Octocode already provides powerful code search, file reading, GitHub/npm research, local structure inspection, and LSP navigation. Skills make those tools feel reliable for a particular job:

- **Less guessing** — the agent follows a purpose-built workflow instead of improvising.
- **Better evidence** — research skills require real anchors such as files, lines, PRs, packages, or test output.
- **Cleaner handoffs** — design, review, memory, and loop skills produce reusable artifacts instead of loose chat.
- **Safer work** — skills add gates for installs, code edits, verification, file locks, and user approval.
- **Less noise** — results are summarized for the user, while raw tool output stays behind the scenes unless it matters.

## What users get

| Need | Best skill family | What you get |
|------|-------------------|--------------|
| Decide whether an idea is worth pursuing | Brainstorming | Prior art, market/code evidence, objections, and a decision brief |
| Understand or change code safely | Engineer | Architecture-aware investigation, implementation/review guidance, and cited findings |
| Converge a clear research question | Loop | Repeated Act -> Observe -> Learn cycles with evidence, verification, and open gaps |
| Write a technical proposal | RFC Generator | Alternatives, trade-offs, blast radius, recommendation, and rollout plan |
| Get hard code-quality feedback | Roast | Severity-ranked issues, concrete fixes, and `file:line` citations |
| Work on skills themselves | Skills | Skill search, rating, linting, install, creation, and safe refactors |
| Coordinate multiple agents or long-running work | Awareness | Memory, file locks, handoffs, messages, and verify-before-done enforcement |
| Inspect Octocode usage | Stats | A dashboard of saved tokens/chars, cache hits, errors, and rate limits |
| Use Octocode from a terminal | CLI | One-off local/GitHub/npm lookups without MCP wiring |

## How they work

Skills use progressive disclosure:

1. The agent sees each skill's `name` and `description`.
2. When the user request matches, the agent reads that skill's `SKILL.md`.
3. The skill may load focused `references/`, run bundled `scripts/`, or call Octocode tools/CLI.
4. The agent returns the user-facing result: a brief, finding list, RFC, trace, dashboard, or verified handoff.

The design goal is practical: keep the active instructions small, load detail only when needed, and make every high-confidence claim traceable to evidence.

---

## When to use each skill

| Skill | Directory | Use it WHEN… |
|-------|-----------|--------------|
| **CLI** | `octocode/` | …you want to research code from the **terminal without MCP** — a one-off local or GitHub/npm code, file, repo, PR, or package lookup in the shell. |
| **Engineer** | `octocode-engineer/` | …you need to **understand, implement, review, refactor, or audit code** — bug investigation, PR/local-diff review, architecture or blast-radius analysis. The default for "work on this code." |
| **Loop** | `octocode-loop/` | …the **goal and research path are already clear** and the work is iterative — run grounded Act→Observe→Learn→Repeat loops over Octocode until evidence converges (research loops, local code-check loops, multi-source loops). |
| **Brainstorming** | `octocode-brainstorming/` | …the **idea is still fuzzy** — "is this worth building", "has anyone built X", "validate my idea", "prior-art for Y". Diverges then validates against evidence; outputs a decision brief, not code. |
| **RFC Generator** | `octocode-rfc-generator/` | …you need a **design doc before coding** — RFC, architecture proposal, migration/implementation plan with alternatives, trade-offs, blast radius, and a recommendation. |
| **Roast** | `octocode-roast/` | …you want **brutal but actionable code critique** — "roast my code", "find antipatterns", severity-ranked sins with `file:line` and fixes. |
| **Skills** | `octocode-skills/` | …you're working on **Agent Skills themselves** — find, evaluate, install, rate/lint, create, or update `SKILL.md` folders. |
| **Awareness** | `octocode-awareness/` | …you need **memory, file locks, or verify-before-conclude** across runs or concurrent agents — before/after big changes, edits, handoffs, or in a shared/dirty repo. |
| **Stats** | `octocode-stats/` | …you want to **visualize Octocode usage** — tokens/chars saved, cache hits, errors, rate limits from `stats.json`. |

### Picking between the research/code skills

They overlap on the surface; the entry condition is what separates them:

- **Idea fuzzy → validate it?** → Brainstorming. **Goal clear → go build/decide?** → next ones.
- **Need to act on code now** (read/change/review) → **Engineer**.
- **Goal is set, just iterate to ground-truth** (converge a question, sweep findings, multi-source dig) → **Loop**.
- **Need a written, reviewable decision before coding** → **RFC Generator**.
- **Just want the harsh quality pass** → **Roast**.

---

## Flows — chaining skills

Skills compose. The canonical handoffs:

```
Brainstorming ──▶ Loop            validate idea → goal locked → run grounded loops to converge evidence
Brainstorming ──▶ RFC Generator   validate idea → turn the decision brief into a formal RFC + rollout plan
Engineer      ──▶ RFC Generator   investigate code + map blast radius → write the change up as a reviewable RFC
```

- **Brainstorming → Loop** — once Brainstorming settles *whether* to build, the goal is clear; hand to **Loop** to iterate Act→Observe→Learn→Repeat until the answer/evidence converges.
- **Brainstorming → RFC Generator** — promote a validated decision brief into a formal RFC with alternatives, trade-offs, and a plan.
- **Engineer → RFC Generator** — Engineer's investigation (architecture, blast radius, prior art) becomes the evidence base for the RFC's recommendation.

---

## Skill details

### CLI — `octocode/`
Drive the `octocode` CLI to research code from a terminal without wiring MCP — across local files and external GitHub/npm with one toolset. Code search, file reads, repo/PR/package lookup.

### Engineer — `octocode-engineer/`
Architecture-aware engineering. CLI-first, schema-first; routes each task to a focused playbook in its `references/` (the references *are* the behavior map). **Every permutation it handles:**

*Research & navigation*
- Orient / understand a local codebase → `research-local`
- Trace symbols, callers, references, types (LSP); exact file reads + pagination → `research-local`
- Structural / AST code search — patterns, rules, gotchas → `context-ast-pattern-cookbook`
- External GitHub/npm research + cross-repo comparison (clone when >3 files or AST/LSP needed) → `research-external`
- Commit & PR history research → `research-external`
- Binary / archive / `.node` / `.wasm` inspection → `research-binary`
- OQL graph, reachability, `--repo` shortcut, `--explain` diagnostics → `workflow` (+ `workflow-graph`)

*Review*
- Remote PR review → `workflow-pr-local-review` (+ `checklist-review-domains`, `template-review-report`)
- Local diff / staged-changes review → `workflow-pr-local-review`
- Large PR/diff (>15 files) parallel review lanes → `workflow-review-parallel-strategy`
- Validate or dismiss a specific finding before reporting it → `workflow-validation-playbooks`

*Analysis & change*
- Architecture review / assessment → `workflow-engineering-research`
- Refactor planning → `workflow-engineering-research`
- Bug investigation (keep ≥2 hypotheses until evidence eliminates one) → `workflow-engineering-research`
- Blast-radius / impact analysis → `workflow-engineering-research` (+ `workflow-graph`)
- Dead-code / reachability / safe-delete / retained-by sweep → `workflow-graph`
- Code-quality / smell sweep → `checklist-quality-signals` → `workflow-validation-playbooks`
- Security finding sweep → `checklist-quality-signals` → `workflow-validation-playbooks`
- Quality metric numbers (knip / tsc / dep-cruiser / ruff / bandit) → `measurement-tools`

*Exact syntax & output*
- CLI command names, flags, raw tools, MCP fallback → `context-cli-mcp-commands`
- Investigation / architecture findings report → `template-artifact-report`
- PR / local review report (≤5–7 key issues) → `template-review-report`

### Loop — `octocode-loop/`
Grounded **Act → Observe → Learn → Repeat** research harness. Every iteration ends in a real tool result with a `status`; deterministic-check verification, context compaction, multi-gate stopping, and failure-mode guards. Three modes: general research, local code-check/findings, full multi-source.

### Brainstorming — `octocode-brainstorming/`
Evidence-first idea validation. Diverges (reframe/invert/analogize/SCAMPER) before converging via parallel GitHub/npm/web/local research and an Advocate-vs-Critic debate. Outputs a decision-ready brief — never designs or code; hands off to RFC Generator for "how to build."

### RFC Generator — `octocode-rfc-generator/`
Turns researched evidence into a technical decision doc: alternatives, trade-offs, blast radius, recommendation, and a practical implementation/rollout plan.

### Roast — `octocode-roast/`
Entertaining, severity-ranked code-quality critique with concrete fixes and `file:line` citations for smells, antipatterns, and maintainability issues — plus redemption paths.

### Skills — `octocode-skills/`
Search, evaluate, create, and update Agent Skills. Inspects real `SKILL.md` files, rates/lints them, installs into agents, and synthesizes new local skills — gating every write/install.

### Awareness — `octocode-awareness/`
Self-awareness (shared memory + work-handoff state), self-harness (verify before concluding; record the test-plan), and files-awareness (pre-flight file locks before any create/edit/delete). For shared workspaces, dirty trees, handoffs, and recurring failures.

### Stats — `octocode-stats/`
Renders an Octocode MCP usage dashboard from `${OCTOCODE_HOME}/stats.json` or `~/.octocode/stats.json` — saved tokens/chars, cache hits, errors, and rate limits.
