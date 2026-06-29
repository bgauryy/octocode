# Octocode Brainstorming

`octocode-brainstorming` turns fuzzy ideas into evidence-grounded decisions. It is for "is this worth building?", "has anyone built this?", "what are the angles?", and "should we add this to our product or codebase?" moments.

The skill does not write code or design the final system. It decides whether the idea deserves that next step.

## When to use

- Validate a feature, product, library, workflow, or developer-tool idea before investing.
- Find prior art and adjacent approaches across local code, GitHub, packages, and the web.
- Turn an early hunch into several sharper framings.
- Identify white space, crowded areas, abandoned attempts, and contested assumptions.
- Decide whether the next move should be an RFC, prototype, narrower experiment, or pause.

Use `octocode-research` when the question is already technical and needs code evidence or changes. Use `octocode-rfc-generator` when the decision is made and the user needs a design plan.

## Features

- Divergent framing before convergence, so the search is not trapped by the first wording.
- A surface plan that states which evidence sources are active or skipped and why.
- Local workspace checks when the idea touches the user's repo.
- GitHub, package, and web prior-art mapping with cross-pollinated search terms.
- A claim ledger that tracks claim, source, confidence, and next query.
- Perspective review through critical, entrepreneurial, and product lenses.
- A concise decision brief with confidence, risks, and next step.
- Optional saved brief and RFC handoff once the idea is ready.

## How it works

The skill follows this flow:

```text
FRAME -> DIVERGE -> RESEARCH -> CROSS-POLLINATE -> STRESS-TEST -> SYNTHESIZE -> DECIDE
```

It first reframes the idea into testable claims. Then it searches the most relevant surfaces, using names and clues from one surface to query the next. Signals are grouped into practical buckets such as crowded, partial, abandoned, contested, and open space. The final answer states what survived review and recommends one decision.

## Internal flow

1. Frame the user's idea and list alternative framings.
2. Declare a surface plan: local, GitHub, packages, and web.
3. Gather evidence and mark weak claims instead of overstating search snippets.
4. Cross-pollinate clues between surfaces.
5. Run objection and opportunity review.
6. Deliver a brief with verdict, confidence, risks, and next action.

The `scripts/brainstorm-run.mjs` helper can record run state, claims, sources, and decisions. Optional Serper and Tavily adapters normalize web results when credentials are configured.

## Installation

Install the published skill:

```bash
npx octocode skill --name octocode-brainstorming
```

Install from a GitHub path or fork:

```bash
npx octocode skill --add bgauryy/octocode/skills/octocode-brainstorming
```

## Benefits

- Avoids building the first plausible idea.
- Makes assumptions visible before they become roadmap or architecture debt.
- Combines creative ideation with evidence and structured pushback.
- Produces a decision that is easy to hand to research, RFC, or implementation work.

## For developers

Keep `SKILL.md` as the concise router for flow, gates, references, scripts, and output. Put search mechanics in `references/tools.md`, presentation rules in `references/output.md`, debate behavior in `references/debate.md`, and saved-report structure in `references/brief-template.md`. Run `scripts/eval-brainstorm.mjs` after changing prompts, references, or output shape.
