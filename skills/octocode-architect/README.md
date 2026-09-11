# Octocode Architect

Review and improve software architecture from exact code, representative flows, algorithmic invariants, dependency topology, runtime evidence, and measurable checks.

## Use when

- A change affects algorithms, boundaries, contracts, external/internal data or control flow, persisted state, or several consumers.
- You need dependency, cycle, reachability, dead-code, coupling, or blast-radius analysis.
- A maintainability or performance problem may justify a safe refactor.

## Capabilities

- Separates architecture intent, observed structure, and hypotheses.
- Checks algorithm correctness, edge cases, termination, complexity, numerical behavior, and concurrency.
- Checks provider/consumer contracts across runtime validation, errors, compatibility, and representative valid/invalid paths.
- Traces external ingress, internal transformation and persistence, and external egress across trust and ownership boundaries.
- Attributes hot paths with representative profiles, traces, and comparable end-to-end measurements before recommending optimization.
- Uses graph topology only for file relationships, then upgrades claims with exact code, AST/LSP identity, flow traces, tests, and measurements.
- Refactors through reversible vertical slices that preserve explicit contracts.
- Keeps ordinary work concise through conditional reference loading.

## Workflow

```text
FRAME → MODEL → PROVE → CHANGE → VERIFY
```

Review-only tasks stop at evidence-backed findings. Authorized implementation tasks include the smallest verified refactor needed for the named quality goal.

## Maintainer verification

```bash
node scripts/eval-architect.mjs --self-test
node scripts/eval-architect.mjs --json
```

Then run the `octocode-skills` review against this folder.

## Install

```bash
npx -y octocode skill install octocode-architect
```
