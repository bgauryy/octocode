# Octocode Engineer

`octocode-engineer` is the main skill for serious code work: investigation, implementation, review, refactoring, architecture assessment, dead-code research, graph/OQL analysis, and artifact inspection.

It is the right default when the user expects the agent to understand code deeply or change it safely.

## How it works

The skill frames the goal and blast radius, keeps a small hypothesis map, and orients with cheap Octocode searches before exact reads. It then uses file anchors, AST/LSP evidence, history, tests, or measurements to prove the path forward, applies the smallest scoped code change or review finding, and verifies before calling the work done.

## Good asks

- "Find the bug and fix it."
- "Review this PR or local diff."
- "Trace the blast radius before we rename this API."
- "Map the architecture around this package."
- "Find dead exports or safe-delete candidates."
- "Inspect this `.node`, `.wasm`, archive, or generated artifact."

## What you get

- A stated goal and scope before the agent dives in.
- A hypothesis map that keeps alternatives alive until evidence eliminates them.
- Exact anchors from files, lines, AST, LSP, history, tests, or measurements.
- Findings ranked by risk and grounded in the codebase.
- A patch, review report, investigation note, or safe next-step plan, depending on the ask.
- User checkpoints when the work crosses contracts, packages, or risky architecture boundaries.

## Use another skill when

- You only need a quick lookup: route to the `octocode` skill.
- You want broad research without edits: use `octocode-research`.
- You need a written design decision before coding: use `octocode-rfc-generator`.
- You want harsh quality feedback as the product: use `octocode-roast`.

## User value

This skill turns code work into a disciplined investigation instead of a search-and-patch guess. The user gets evidence first, then the smallest safe action that follows from that evidence.
