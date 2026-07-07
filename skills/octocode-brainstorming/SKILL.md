---
name: octocode-brainstorming
description: "Use when the user wants to brainstorm or validate an idea against evidence — triggers like \"is this worth building\", \"has anyone built X\", \"validate my idea\", \"check if X exists\", \"prior-art for Y\", \"should we add X to our app\". Diverges then converges, validates claims against local/GitHub/package/web data, stress-tests, and decides Build RFC / Prototype / Narrow / Park. Outputs a decision brief, not code."
---

# Octocode Brainstorming

Evidence-grounded idea exploration: diverge (defer judgment until the framing slate is captured), then validate against local/GitHub/package/web evidence, stress-test, and decide. Flow: `FRAME -> DIVERGE -> RESEARCH -> CROSS-POLLINATE -> STRESS-TEST -> SYNTHESIZE -> DECIDE`.

Modes: **Generate** (user wants ideas → 6-10 angles, validate best 2-3) · **Validate** (worth building? → 2-4 reframings, research deeply) · **Map** (who's built this? → adjacent terms, landscape map).
Declare a Surface Plan before searching: mark Local, Web/top resources, and GitHub/packages/code active or skipped, each with a reason. For repo-targeted ideas, orient locally first; otherwise start with top resources before repo/package/code searches.

## Hard Gates

STOP, recommend one option, and wait when any gate trips:
- idea maps to 3+ unrelated spaces;
- active surfaces stay thin after synonym retries;
- evidence materially conflicts;
- delegation would exceed 5 workers.

## Research Rules

- Recall first (FRAME): use the host's memory tool when available (`memory_recall`/`get-memory --smart`); retry synonyms on zero results and validate recalled facts.
- Capture last (DECIDE): use `memory_record`/`memory_reflect` or `octocode-awareness`'s capture flow when installed; otherwise keep the lesson in the brief, and skip capture if nothing durable survived rebuttal.
- Treat snippets and search summaries as leads; cite fetched pages, exact files, repos, packages, PRs, metrics, or mark claims `weak`.
- Default external loop: top articles/docs/papers -> repos/packages/code -> exact reads -> loop back to sources for contradictions.
- Cross-pollinate at least once per active surface — dispatch the Web Search Scout and/or Trend & Source Scout (`references/tools.md`) when a slice needs a dedicated worker.
- Keep a claim ledger: `claim -> source -> confidence -> next query`; for substantial, multi-surface, or high-confidence runs, start `scripts/brainstorm-run.mjs` via `references/hook-communication.md`.
- Run Critical Architect, Visionary Entrepreneur, and Product lenses before a final verdict unless the worker gate shortens review.

## Reference Map

- `references/tools.md` — when building the surface plan or running local, GitHub, package, and web searches.
- `references/trend-sources.md` — when Tavily/Serper alone don't give a momentum/crowdedness signal, a published-research check (arXiv/Scholar), or confirmation a platform already shipped the idea. Generic across domains, not AI/devtools-only.
- `references/debate.md` — when running the three-lens perspective review and cross-exam.
- `references/output.md` — when presenting the chat brief, confidence markers, or RFC handoff.
- `references/brief-template.md` — when the user confirms saving a fuller decision brief.
- `references/hook-communication.md` — before substantial, multi-turn, or subagent-heavy research.
- `references/grounding.md` — when challenged on methods, SCAMPER, or web-engine contracts.
- `references/octocode.md` — when choosing transport/auth/install/CLI-MCP fallback, or before local/GitHub/npm research (its Research Algorithm section governs how to search).

## Scripts

- `scripts/brainstorm-run.mjs` — record run state/claims/sources/decisions as resumable ledgers; `scripts/eval-brainstorm.mjs` — self-test and evaluate answers against `evals/cases.json`.
- `scripts/serper-search.mjs`, `scripts/tavily-search.mjs` — query Serper/Tavily for normalized JSON web results when API credentials are set.

## Output

Use concise chat sections: `TL;DR`, `Framings`, `Evidence by surface`, `What survived review`, `Verdict`, `Risks`, `Next`. For build-ready ideas, hand off to `octocode-rfc-generator`; for code work, use `octocode-research`. Install: `npx octocode skill --name octocode-brainstorming`.
