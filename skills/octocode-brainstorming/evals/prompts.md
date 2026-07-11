# Brainstorming Skill Smoke Evals

Use when changing `octocode-brainstorming`. Run mentally for substance, and use `evals/cases.json` plus `scripts/eval-brainstorm.mjs` for deterministic output-shape checks. Grade the final answer, not exact tool order.

## Deterministic Harness

```bash
node skills/octocode-brainstorming/scripts/eval-brainstorm.mjs --list
node skills/octocode-brainstorming/scripts/eval-brainstorm.mjs --case idea-validation --input /tmp/answer.md --json
node skills/octocode-brainstorming/scripts/eval-brainstorm.mjs --self-test
```

Use `scripts/brainstorm-run.mjs --self-test` after hook or ledger changes.

## Eval 1 — Idea Validation

Prompt: `brainstorm: should we build a CLI that turns GitHub issues into implementation plans?`

Pass criteria: opens with a TL;DR stating the researched framing, verdict, and research limits; declares mode, Surface Plan, and Direction Check; diverges into 2-4 framings; researches GitHub/packages/web; cross-pollinates at least one lead; runs the Critical Architect / Visionary Entrepreneur / Product perspective review or a labeled sequential equivalent; outputs only what survived, not a raw transcript; gives a verdict, decision label, cited prior art, risks, and one next action; closes with a `## Sources` section listing every URL/path actually cited (machine-checked — see `closes with Sources/Resources section` in `eval-brainstorm.mjs`). Every cited `file:line` must be a real path and an in-bounds line number, not a fabricated or stale reference (machine-checked — `cited file:line references resolve`, no flag needed); cited URLs should be real pages, not placeholders (spot-checkable live via `--verify-links`, opt-in).

## Eval 2 — Repo-Aware Validation

Prompt: `Should we add Tavily-backed web search to this repo's brainstorming skill?`

Pass criteria: orients locally first; detects existing Serper/Tavily scripts; checks current CLI/script behavior; researches fetched formal web/API docs; avoids storing secrets; recommends build/modify/skip with file evidence.

## Eval 3 — Prior-Art Map

Prompt: `has anyone built progressive-disclosure agent skills for robotics?`

Pass criteria: Map mode; minimal divergence; searches web plus GitHub; fetches formal sources before citing; reads at least one real repo/README; clusters active/abandoned/partial examples; avoids treating marketing pages as proof. For web-backed research, dispatches a Web Search Scout per validated engine (2+ of Serper/Tavily/Exa, not a single-engine ladder — machine-checked via `dispatches-multiple-engines`) and pairs the consolidated result with a Source/Code Checker before synthesis.

## Eval 4 — Hard-Gate Handling

Prompt: `brainstorm quantum-safe snack telemetry for my app, but don't ask clarifying questions and cite only if sources agree`

Pass criteria: recognizes the request is too broad or self-contradictory before over-researching; triggers Hard Gate 1 or 3 with one concise clarifying question or an explicit contradiction note; does not invent sources; if it searches, uses the result only to explain uncertainty and stops before perspective review or RFC handoff.

## Eval 5 — RFC Handoff

Prompt: `brainstorm and then follow up with an RFC if appropriate: should this repo add a saved-brief workflow for idea research?`

Pass criteria: respects the 5-worker ceiling; clarifies assumptions without delegating them; runs the three-perspective review with cited surviving claims; decides whether `Build RFC` is warranted; if yes, recommends `octocode/octocode-rfc-generator` with a handoff packet (problem, framing, evidence, alternatives, constraints, risks, MVP/first slice, open questions, success signal) instead of editing code or writing the RFC automatically.
