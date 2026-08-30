---
name: octocode-brainstorming
description: "Use when an idea needs disciplined exploration before building: generate options, test whether it is worth building, map adjacent solutions, challenge assumptions, narrow scope, or choose Build RFC / Prototype / Narrow / Park — even if nobody said brainstorm. Already decided to implement a specific plan → skip."
---

# Octocode Brainstorming
Evidence-grounded idea exploration.
Flow: `FRAME → DIVERGE → RESEARCH → CROSS-POLLINATE → STRESS-TEST → SYNTHESIZE → DECIDE`.

Workspace output contract: chat-only answers stay in chat. New artifacts default to `<workspace>/.octocode/octocode-brainstorming/`; resumable run ledgers use `<workspace>/.octocode/brainstorming/runs/`. User-approved source edits keep their named paths. Never fall back to a user-level Octocode home for artifacts.

## Modes and lobby rules
- Generate: create 6–10 angles, then validate the best 2–3. Validate: produce 2–4 reframings, then investigate deeply. Map: expand adjacent terms and landscape existing solutions.
- Capture framing before judging. When direction, audience, or research surface is unclear, ask the user to choose before deep research.
- Declare a Surface Plan: mark local, top resources/web, and repository/package/code evidence active or skipped with a reason.
- Treat snippets and summaries as leads; cite exact sources or mark claims weak. Track `claim → source → confidence → next query`.
- Cross-pollinate across each active surface and run Critical Architect, Visionary Entrepreneur, and Product lenses before verdict.
- Recall potentially useful context first and validate it; capture only durable lessons that survive rebuttal.

## Hard gate
Stop, recommend one option, and wait when the idea spans three unrelated spaces, active evidence stays thin after synonym retries, evidence materially conflicts, or delegation would exceed five workers.

## Smart routes — load only what the current step needs
- When framing the idea and diverging into angles, build the Surface Plan with `references/tools.md`; when code/repository/package evidence is active, load `references/octocode.md` — choose sources deliberately and delegate technical research correctly.
- When generic results cannot prove momentum, crowdedness, publication, or shipped prior art, load `references/trend-sources.md` — add time-sensitive evidence without domain lock-in.
- When cross-pollinating leads from one active surface into another, stay in `references/tools.md` and `references/web-search-workers.md` — carry each finding across surfaces instead of closing a surface early.
- When stress-testing, load `references/debate.md` — run the three lenses and cross-examination before converging.
- When research is substantial, multi-turn, or delegated, load `references/hook-communication.md` and run `scripts/brainstorm-run.mjs` — preserve a resumable claim/source/decision ledger.
- When synthesizing and deciding, load `references/output.md` and score every prior-art claim with `references/confidence.md`; if the user approves a durable artifact, load `references/brief-template.md` — match chat brevity or saved decision depth.
- When methods or source contracts are challenged, load `references/grounding.md` — make the process falsifiable.
- When improving this skill, prefer `octocode-eval-benchmark`; otherwise load `references/improve-loop.md` — require measurable acceptance.

## Related routes
- Use `octocode-rfc-generator` for a Build verdict; `octocode-research` for technical evidence; `octocode-eval-benchmark` for measurable experiments.
- Use `octocode-skills` when changing this skill folder.
- Use `octocode-subagent` to dispatch and synthesize workers — see `references/web-search-workers.md` for the brainstorm-specific Scout/Aggregator/Checker topology and the five-worker ceiling.

## Scripts — every one takes `--help`
| Script | Run when | How |
|---|---|---|
| `scripts/brainstorm-run.mjs` | research is substantial, multi-turn, delegated, or saved as a brief | `node <skill_dir>/scripts/brainstorm-run.mjs start --idea "<idea>" --mode Validate`, then `checkpoint --run-id <id>`, then `finish --run-id <id>`; `hook --event <event>` serves `hooks/hooks.json`; commands in `references/hook-communication.md` |
| `scripts/serper-search.mjs` | validating web credentials at session start | `node <skill_dir>/scripts/serper-search.mjs --check` — broad Google results (`SERPER_API_KEY`) |
| `scripts/tavily-search.mjs` | validating web credentials at session start | `node <skill_dir>/scripts/tavily-search.mjs --check` — curated/deeper research (`TAVILY_API_KEY`) |
| `scripts/exa-search.mjs` | validating web credentials at session start | `node <skill_dir>/scripts/exa-search.mjs --check` — neural/category search (`EXA_API_KEY`) |

- Use the `--check` scripts for credential presence only; fetching and search output come from the host web tool. Default to querying every validated engine and consolidating, not a first-success ladder (`references/tools.md`).
- All four scripts import the vendored `scripts/octocode-config.mjs` for Octocode home and env; never import `@octocodeai/config`, which is absent when this folder installs alone.

## Output
Use `TL;DR`, `Framings`, `Evidence by surface`, `What survived review`, `Verdict`, `Risks`, `Next`, `Sources`.
**Always end with `Sources`** — a consolidated list of every URL/path actually cited above with the claim it backs, even for chat-only answers never saved as a brief. Template: `references/output.md` and `references/brief-template.md`.
