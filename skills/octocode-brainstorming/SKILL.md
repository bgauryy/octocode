---
name: octocode-brainstorming
description: "Use when an idea needs disciplined exploration before building: generate options, test worth-building, map adjacent solutions, challenge assumptions, narrow scope, or choose Build RFC / Prototype / Narrow / Park — even without saying brainstorm."
---

# Octocode Brainstorming
Evidence-grounded idea exploration. Flow: `FRAME → DIVERGE → RESEARCH → CROSS-POLLINATE → STRESS-TEST → SYNTHESIZE → DECIDE`.

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
- When building the Surface Plan, load `references/tools.md`; when code/repository/package evidence is active, load `references/octocode.md` — choose sources deliberately and delegate technical research correctly.
- When generic results cannot prove momentum, crowdedness, publication, or shipped prior art, load `references/trend-sources.md` — add time-sensitive evidence without domain lock-in.
- When stress-testing, load `references/debate.md` — run the three lenses and cross-examination before converging.
- When research is substantial, multi-turn, or delegated, load `references/hook-communication.md` and run `scripts/brainstorm-run.mjs` — preserve a resumable claim/source/decision ledger.
- When presenting, load `references/output.md`; if the user approves a durable artifact, load `references/brief-template.md` — match chat brevity or saved decision depth.
- When methods or source contracts are challenged, load `references/grounding.md` — make the process falsifiable.
- When improving this skill, prefer `octocode-eval`; otherwise load `references/improve-loop.md` — require measurable acceptance.

## Related routes and scripts
- Use `octocode-rfc-generator` for a Build verdict; `octocode-research` for technical evidence; `octocode-eval` for measurable experiments.
- Use `octocode-awareness` to capture durable lessons; `octocode-skills` when changing this skill folder.
- Use `octocode-subagent` to dispatch and synthesize workers — see `references/web-search-workers.md` for the brainstorm-specific Scout/Aggregator/Checker topology and the five-worker ceiling.
- Use `scripts/serper-search.mjs`, `scripts/tavily-search.mjs`, or `scripts/exa-search.mjs` for configured web search — default to querying every validated engine and consolidating, not a first-success ladder (`references/tools.md`); run `scripts/eval-brainstorm.mjs` when changing this skill.

## Output
Use `TL;DR`, `Framings`, `Evidence by surface`, `What survived review`, `Verdict`, `Risks`, `Next`, `Sources`.
**Always end with `Sources`** — a consolidated list of every URL/path actually cited above with the claim it backs, even for chat-only answers never saved as a brief. Template: `references/output.md` and `references/brief-template.md`.
