---
name: octocode-brainstorming
description: "Use when an unresolved idea needs options, feasibility testing, adjacent opportunities, or scope exploration before building. Not for implementing a settled decision or evaluating a defined change."
---

# Octocode Brainstorming
tools: `npx octocode` / `octocode-mcp`
related-skill: `octocode-research`
output: `<workspace>/.octocode/` for workspace work | `<home>/.octocode/` when no workspace applies
routes: load/run a reference, doc, or script only when it changes the next action; otherwise keep the rule here.

Evidence-grounded idea exploration.
Flow: `FRAME → DIVERGE → RESEARCH → CROSS-POLLINATE → STRESS-TEST → SYNTHESIZE → DECIDE`.

Artifacts: `<output>/octocode-brainstorming/`; resumable runs: `<output>/brainstorming/runs/`. Chat-only answers stay in chat; approved edits keep their named paths.

## Modes and lobby rules
- Generate: create distinct angles, then validate the strongest few. Validate: reframe enough to avoid anchoring, then investigate. Map: expand adjacent terms and existing solutions.
- Capture framing before judging. Ask one focused question only when direction, audience, or research scope changes the work materially.
- Declare a Surface Plan: mark local, top resources/web, and repository/package/code evidence active or skipped with a reason.
- Treat snippets and summaries as leads; cite exact sources or mark claims weak. Track `claim → source → confidence → next query`.
- Carry useful leads across active surfaces. Use the relevant Critical Architect, Visionary Entrepreneur, and Product lenses; for consequential verdicts, check all three.
- Recall potentially useful context first and validate it; capture only durable lessons that survive rebuttal.

## Decision gate
Pause for direction when the idea contains unrelated decisions, evidence remains too thin, or conflicting for a defensible verdict, or the next research round costs more than it can change. Otherwise state the uncertainty and recommend the smallest decision-changing step.

## Smart routes — load only what the current step needs
- When framing the idea and diverging into angles, build the Surface Plan with `references/tools.md`. When code/repository/package evidence is active, use `octocode-research`; if unavailable, use `octocode-mcp` or `npx octocode`, inspect live context/schema once, and follow executable continuations — choose sources deliberately and delegate technical research correctly.
- When generic results cannot prove momentum, crowdedness, publication, or shipped prior art, load `references/trend-sources.md` — add time-sensitive evidence without domain lock-in.
- When cross-pollinating leads from one active surface into another, stay in `references/tools.md` and `references/web-search-workers.md` — carry each finding across surfaces instead of closing a surface early.
- When stress-testing, load `references/debate.md` — run the three lenses and cross-examination before converging.
- When research is substantial, multi-turn, or delegated, load `references/hook-communication.md`. Run `scripts/brainstorm-run.mjs` to preserve a resumable claim/source/decision ledger.
- During DECIDE and synthesis, load `references/output.md`. Score every prior-art claim with `references/confidence.md`; if you approve a durable artifact, load `references/brief-template.md` — match chat brevity or saved decision depth.
- When methods or source contracts are challenged, load `references/grounding.md` — make the process falsifiable. <!-- style-lint: ignore-line passive-voice -->
- When improving this skill, use `octocode-eval-benchmark`; if unavailable, record goal, KPI, baseline, smallest change, comparable result, and accept/revert — require measurable acceptance.

## Related routes
- Use `octocode-rfc-generator` for a Build verdict and `octocode-eval-benchmark` for measurable experiments. For technical evidence, `octocode-research` owns the MCP/CLI workflow and live tool/grammar discovery.
- Use `octocode-skills` when changing this skill folder.
- Use `octocode-subagent` to dispatch and synthesize workers — see `references/web-search-workers.md` for the brainstorm-specific Scout/Aggregator/Checker topology.

## Scripts — every one takes `--help`
| Script | Run when | How |
|---|---|---|
| `scripts/brainstorm-run.mjs` | research is substantial, multi-turn, delegated, or saved as a brief | `node <skill_dir>/scripts/brainstorm-run.mjs start --idea "<idea>" --mode Validate`, then `checkpoint --run-id <id>`, then `finish --run-id <id>`; `hook --event <event>` serves `hooks/hooks.json`; commands in `references/hook-communication.md` |
| `scripts/serper-search.mjs` | validating web credentials at session start | `node <skill_dir>/scripts/serper-search.mjs --check` — broad Google results (`SERPER_API_KEY`) |
| `scripts/tavily-search.mjs` | validating web credentials at session start | `node <skill_dir>/scripts/tavily-search.mjs --check` — curated/deeper research (`TAVILY_API_KEY`) |
| `scripts/exa-search.mjs` | validating web credentials at session start | `node <skill_dir>/scripts/exa-search.mjs --check` — neural/category search (`EXA_API_KEY`) |

- Use the `--check` scripts for credential presence only; fetching and search output come from the host web tool. Pick engines by the evidence need and add another when it can reduce a real gap (`references/tools.md`).
- All four scripts import the vendored `scripts/octocode-config.mjs` for Octocode home and env; never import `@octocodeai/config`, which is absent when this folder installs alone.

## Output
Use the compact shape in `references/output.md`: framing, evidence, what survived review, verdict, risks, and next step. When evidence was cited, end with a consolidated `Sources` list. Save only with approval using `references/brief-template.md`. <!-- style-lint: ignore-line passive-voice -->
