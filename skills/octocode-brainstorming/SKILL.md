---
name: octocode-brainstorming
description: "Use when the user needs evidence-grounded idea validation, prior-art mapping, or white-space brainstorming: diverge before converging, validate claims against local/GitHub/packages/web data, stress-test assumptions, and decide Build RFC / Prototype / Narrow / Park. Outputs a decision brief, not code or designs."
---

# Octocode Brainstorming

Evidence-grounded idea exploration: diverge first, validate top resources, local/GitHub/package/code evidence, stress-test claims, then decide. Flow: `FRAME -> DIVERGE -> RESEARCH -> CROSS-POLLINATE -> STRESS-TEST -> SYNTHESIZE -> DECIDE`.

- Generate: user wants ideas; create 6-10 angles, then validate the best 2-3.
- Validate: user asks if an idea is worth building; create 2-4 reframings, then research deeply.
- Map: user asks who has built something; use adjacent search terms and build a landscape map.
Always declare a Surface Plan before searching: Local, Web/top resources, and GitHub/packages/code active/skipped, each with a reason. Do quick local orientation first for repo-targeted ideas; otherwise start with top resources before repo/package/code searches.

## Hard Gates

Stop and ask before passing these gates: idea maps to 3+ unrelated spaces; all active surfaces stay thin after synonym retries; evidence materially conflicts; delegation would exceed 5 workers. State the choice, recommend one option, and wait.

## Research Rules

- Defer judgment during divergence; converge only after the framing slate is captured.
- Treat snippets and search summaries as leads; cite fetched pages, exact files, repos, packages, PRs, metrics, or mark claims `weak`.
- Default external loop: top articles/docs/papers -> repos/packages/code -> exact reads -> loop back to sources for contradictions. Cross-pollinate at least once per active surface.
- Keep a claim ledger: `claim -> source -> confidence -> next query`; for substantial, multi-surface, or high-confidence runs, start `scripts/brainstorm-run.mjs` via `references/hook-communication.md`.
- Run Critical Architect, Visionary Entrepreneur, and Product lenses before a final verdict unless the worker gate shortens review.

## Reference Map

- `references/tools.md` — when building the surface plan or running local, GitHub, package, and web searches.
- `references/debate.md` — when running the three-lens perspective review and cross-exam.
- `references/output.md` — when presenting the chat brief, confidence markers, or RFC handoff.
- `references/brief-template.md` — when the user confirms saving a fuller decision brief.
- `references/hook-communication.md` — before substantial, multi-turn, or subagent-heavy research.
- `references/grounding.md` — when challenged on methods, SCAMPER, or web-engine contracts.
- `references/octocode.md` — when choosing transport, auth, install, or CLI/MCP fallback behavior.

## Scripts

- `scripts/brainstorm-run.mjs` — record run state, claims, sources, decisions, and resumable ledgers.
- `scripts/eval-brainstorm.mjs` — self-test and evaluate brainstorm answers against `evals/cases.json`.
- `scripts/serper-search.mjs` — query Serper with normalized JSON results when API credentials are available.
- `scripts/tavily-search.mjs` — query Tavily with normalized JSON results when API credentials are available.

## Output

Use concise chat sections: `TL;DR`, `Framings`, `Evidence by surface`, `What survived review`, `Verdict`, `Risks`, `Next`. For build-ready ideas, hand off to `octocode-rfc-generator`; for code work, use `octocode-research`.

Install hint: `npx octocode skill --name octocode-brainstorming`.
