# Octocode Architect

Analyze software architecture with exact code evidence, then make or verify the smallest safe change.

## Use when

- A change affects algorithms, boundaries, contracts, data/control flow, persisted state, or several consumers.
- You need dependency, cycle, reachability, dead-code, coupling, or blast-radius analysis.
- A maintainability or performance problem may justify a safe refactor.

## Not for

- Gathering evidence without a specific architecture decision → `octocode-research`
- Behavior-preserving cleanup of dead code and agent residue → `octocode-clean-agentic-code`
- Open-ended exploration before a decision is formed → `octocode-brainstorming`

## Workflow

```text
FRAME → MODEL → PROVE → CHANGE → VERIFY
```

Review-only tasks stop at evidence-backed findings. Authorized implementation tasks include the smallest verified refactor needed for the named quality goal.

Evidence comes through `octocode-research`. File topology uses beta `astTopology` (set `OCTOCODE_BETA=true`) or the persisted `octocode graph ingest` / `octocode graph query` CLI; both return syntactic leads that need exact reads and LSP identity before a verdict.

## Install

```bash
npx -y octocode skill install octocode-architect
```

## Maintainer verification

```bash
node scripts/eval-architect.mjs --self-test
node scripts/eval-architect.mjs --json
```

Then run the `octocode-skills` review against this folder.

## Sources

Local sources behind these rules (repository-relative). Defect-prevalence citations live in `references/agent-defect-evidence.md`.

| Source | Used for |
|---|---|
| `packages/octocode-native/crates/runtime/src/tools/ast_graph/` | `astTopology` analyses, edge kinds, coverage and pagination signals, dead-code candidates, persisted-graph issue detectors |
| `packages/octocode-native/crates/cli/src/cli/graph.rs` | `octocode graph ingest` / `query` operations |
| `packages/octocode-native/crates/engine/src/graph/algorithms.rs` | SCC condensation, layers, transitive edges, dominators, and path primitives |
| `packages/octocode-native/docs/engine/SUPPORTED_LANGUAGES_AND_FEATURES.md` | AST, graph, and LSP capability boundaries |
| `skills/octocode-research/` | schema-first evidence collection and semantic proof lanes |
| `skills/octocode-skills/` | progressive disclosure, trigger tuning, cleanup, and review gates |
| Repository `AGENTS.md` and package architecture guides | local ownership, dependency direction, and verification contracts |

General architecture and algorithm practices stay hypotheses until the checked-out system supplies intent, mechanism, and impact.
