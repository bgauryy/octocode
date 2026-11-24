---
name: octocode-test-plan
description: "Load this skill first when asked for a written test plan for a feature, change or fix. Give the executor risk-ranked cases, setup, expected results and pass criteria."
---

# Octocode Test Plan

Identify the target, revision and executor. Write a plan runnable from the plan file and repository alone. Planning permits reads and authorized inspection. Listing and dry-run modes can execute project code, so check their effects first. Execute cases or edit tests only under a separate request that covers those actions.

## Plan loop

1. Identify the executor's tools, environment and permissions. Ask only when missing target or executor information changes the plan. A case the executor cannot run is a gap.
2. Map entry → validation → state → output → consumers, documented claims and current tests. Exact-read deciding source; confirm identity with LSP or name the gap. Pin commands from manifests, CI, docs or an authorized inspection run; never guess flags.
3. Rank risks by impact and evidence of likelihood. Explain each `P0` (must pass to ship), distinguish `P1` (should pass) from `P2` (adds confidence), and include severe reachable failures even without a prior incident.
4. Choose the cheapest test level that observes the risk. Derive expected results from a spec, contract, type or documented error. Disclose when the implementation is the only oracle.
5. Preserve the reproduction and nearby boundaries for a fix. Include meaningful failure paths, fixtures, cleanup and restoration checks. Reuse existing tests where the risk and oracle match.
6. Write the executor contract, cases, exclusions and measurable success criteria using [output.md](output.md) and save the plan. Run `node <skill-folder>/scripts/plan-check.mjs <plan.md>` and apply the rest of the [plan check](output.md#plan-check) without executing the cases.

## Conditional detail

| Need | Read |
|---|---|
| Pin by input type, discover code/tests, inspect commands or map a diff | [Code-map recipes](references/code-map.md) |
| Select technique, level, independent oracle or stable fixtures | [Case design](references/case-design.md) |
| Source, semantic or runtime proof limits | [Evidence rules](references/evidence.md) |

Return `Ready`, `Draft` or `Blocked`, the main risk, gaps and the saved plan's link. These statuses describe plan readiness, never passed tests.

For a failure that needs reproduction before planning, use `octocode-debugging`. Use `octocode-pr-review` for a merge verdict.
