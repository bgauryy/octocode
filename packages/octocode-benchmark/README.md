# @octocodeai/octocode-benchmark

A private, source-only benchmark workspace for Octocode. The benchmark is **unified and doc-driven**: it lives in [`compare/unified/`](compare/unified/README.md).

- A **worker** is a folder `compare/unified/workers/<id>/` with an instruction doc (`WORKER.md`) and a tool profile (`profile.json`). The harness iterates `workers/` and never branches on a worker id. Today: `octocode` (Octocode MCP) and `rg-gh` (`rg` + `gh`).
- **Questions**: 30 pinned questions (10 GitHub-heavy: PR review, GitHub code research, bug root-cause; 20 local on cloned repos in 8 languages).
- **Judge**: Opus, blinded X/Y, both orders, tie-break; scores quality 0–10 against evaluator-only references.
- **Tokens**: per-request usage from the stream, split into fixed overhead and research tokens.

Published results: [docs/BENCHMARKS.md](../../docs/BENCHMARKS.md). See [ARCHITECTURE.md](ARCHITECTURE.md) for invariants.
