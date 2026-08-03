# Comparison Suites (v2)

Active head-to-head suites. Shared contracts live one level up:
[methodology](../README.md) · [run instructions](../INSTRUCTIONS.md) ·
[judging](../JUDGING.md) · [scoring](../SCORING.md) ·
[report template](../REPORT_TEMPLATE.md). Canonical questions live per subject
under [`../questions/`](../questions/).

| Suite | Arm A (baseline) | Arm B (treatment) | Question bank |
|---|---|---|---|
| [octocode-vs-gh](octocode-vs-gh/) | `gh` CLI | Octocode **MCP** (remote GitHub only) | [github/research-v2](../questions/github/research-v2/) |
| [octocode-vs-gh-rtk](octocode-vs-gh-rtk/) | `rtk` + `gh` CLI | Octocode **MCP** (remote GitHub only) | [github/research-v2](../questions/github/research-v2/) |
| [octocode-vs-ast-grep](octocode-vs-ast-grep/) | `ast-grep` CLI | Octocode **CLI** (local only) | [local-code/ast-grep-react-v2](../questions/local-code/ast-grep-react-v2/) |

**Arm B surface is fixed per suite and is the only variable under test:** the
remote GitHub suites run Octocode over **MCP** (remote GitHub tools only — no
CLI, local, clone, AST, LSP, npm, or cache advantage); the `ast-grep` suite runs
Octocode over the **local CLI** (`node packages/octocode/out/octocode.js`).
MCP and CLI expose the same tool runners; the surface is pinned so the
comparison stays apples-to-apples with the baseline's reach.

Each suite folder holds ONLY `README.md` — arms, boundaries, and which bank
questions to run. Questions and oracles live once, in the canonical bank;
results ledgers are tracked at [`../../results/<suite>.md`](../results/). v1 suites
were removed from the tree on 2026-08-03 and are not comparable with v2.
