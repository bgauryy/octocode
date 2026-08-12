Mode: Validate

## TL;DR
SQLite-first team memory is worth prototyping, but evidence on vector stores conflicts: contested claims mean the decision narrows to Prototype First rather than a full build. Research limits: none.

## Surface Plan
Local active because the idea touches this skill. GitHub/packages active because memory systems have competing prior art. Web active for formal docs and current project guidance.

## Framings Considered
- Researched: SQLite-first team memory with optional semantic recall.
- Set aside: vector-only memory as the default storage layer.

## Landscape
- Local eval harnesses keep the base workflow inspectable and gated. `moderate` skills/octocode-graph-eval/SKILL.md:8
- Vector memory projects show value for paraphrase recall, but add dependencies and tuning cost. `moderate` https://arxiv.org/abs/2310.08560

## Perspective Review
- Critical Architect: SQLite-first held because local sensors already support keep/discard verification; evidence skills/octocode-graph-eval/SKILL.md:12.
- Visionary Entrepreneur: optional semantic recall held because differentiated recall helps long-running work; evidence https://mem0.ai/blog/introducing-openmemory-mcp.
- Product: default-vector memory was contested because setup friction would hurt first-run use; evidence skills/octocode-graph-eval/references/routing.md:15.
- Conceded: the claim that vector recall should be the default was dropped as weak until dependency and tuning costs are proven.

## Decision Delta
The conflicting evidence changed the decision from Build RFC to Prototype First: keep SQLite-first, test optional semantic recall, and leave vector-default storage unresolved.

Decision: Prototype First

## Recommended Next Step
Run one prototype that compares SQLite/FTS recall against optional semantic recall on real repo handoffs.

## Sources
- skills/octocode-graph-eval/SKILL.md:8 — backs the local/inspectable sensor-first claim above.
- https://arxiv.org/abs/2310.08560 — backs the paraphrase-recall-value/dependency-cost claim above.
- skills/octocode-graph-eval/SKILL.md:12 — backs the keep/discard verification claim above.
- https://mem0.ai/blog/introducing-openmemory-mcp — backs the differentiated-recall claim above.
