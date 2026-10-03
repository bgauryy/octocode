# Parallel roasting — Multi-Agent sin hunting

Load when independent modules or risk categories make delegation worthwhile. Why: divide inspection without duplicating or dropping findings. If the host lacks workers, run the same domains sequentially.

## Route
1. Identify independent domains whose inspection can run without shared mutable state.
2. Spawn only when parallelism, specialist context, or isolation repays coordination cost.
3. Give each worker a bounded scope and evidence contract.
4. Merge, deduplicate, and rank after every required worker returns or is marked partial. <!-- style-lint: ignore-line passive-voice -->

Each worker returns this compact contract:

| Field | Requirement |
|---|---|
| Scope | Exact files/directories inspected |
| Findings | `file:line`, evidence, impact, confidence, repair move |
| Non-findings | High-risk patterns checked but not found |
| Limits | Missing tools, unreadable paths, or reduced coverage |

Keep target selection, final prioritization, autopsy, and repairs with the parent. Split only independent inspection/inventory domains, such as security, architecture, or performance. Each research worker uses `octocode-research` when installed; otherwise it reports reduced coverage.

Avoid workers for cheap inspection, tightly dependent scopes, or repairs that share files. The parent verifies load-bearing claims before the roast.

Next: merge the worker contracts, then rank the combined inventory with `references/sin-catalog.md` and resume the sequential phases in `references/roast-playbook.md` § 4 Autopsy.
