# @octocodeai/octocode-benchmark

A private, source-only benchmark workspace for Octocode. The benchmark is **unified and doc-driven**: it lives in [`compare/unified/`](compare/unified/README.md).

- A **worker** is a folder `compare/unified/workers/<id>/` with an instruction doc (`WORKER.md`) and a tool profile (`profile.json`). The harness iterates `workers/` and never branches on a worker id. The baseline pair is `octocode` (Octocode MCP) and `rg-gh` (`rg` + `gh`); the other `octocode-*` folders are profile variants.
- **Questions**: pinned GitHub, local (cloned repos at pinned commits) and mixed questions; the category table is in [`compare/unified/questions/QUESTIONS.md`](compare/unified/questions/QUESTIONS.md).
- **Judge**: Opus, blinded X/Y, both orders, tie-break; scores quality 0–10 against evaluator-only references.
- **Tokens**: per-request usage from the stream, split into fixed overhead and research tokens.

Published results: [compare/unified/RESULTS.md](compare/unified/RESULTS.md). See [ARCHITECTURE.md](ARCHITECTURE.md) for invariants.
