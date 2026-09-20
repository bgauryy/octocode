---
name: octocode-jev-reasoning-loop
description: "Use when the pure Jev CLI can make a bounded semantic judgment that changes the next action; skip exact checks and settled decisions."
---
# Jev CLI

tools: `npx octocode` / `octocode-mcp`
related-skill: `octocode-research`
output: `<workspace>/.octocode/` for workspace work | `<home>/.octocode/` when no workspace applies
routes: load/run a reference, doc, or script only when it changes the next action; otherwise keep the rule here.

Flow: `INSPECT SCHEMA → CHOOSE MATRIX OR INDEPENDENT PAIRS → BOUND RESOURCES → JUDGE → READ RETAINED PROOF`.

Inspect `octocode scheme jev --view query --compact` once; use its description and query schema as the instructions. Reuse them while current.
Run `octocode jev --input request.json --compact`. Prefer root `reasoning`, `resources:[{id,context}]`, and `questions:[{id,question}]` for a shared question set; the runtime captures each resource once and returns resource-major rows with both IDs. Use `queries[]` only when the context/question pairs are independent and their cross-product would be wrong; those rows use ordered `index`. Keep the matrix at 25 cells or fewer (maximum 25 resources and five questions). Split huge files/browser bodies into bounded resources and submit successive matrices until all chunks are judged. Read retained proof for partial, insufficient, relevant, and errored chunks. The runtime selects the model.
In this repository replace `octocode` with `node packages/octocode/out/octocode.js`.
For setup → [ojql.md](references/ojql.md). For optional examples → [jev-workflows.md](references/jev-workflows.md).
