# Parallel roasting — Multi-Agent sin hunting

Load when independent modules or risk categories make delegation worthwhile: divide inspection without duplicating or dropping findings. If the host lacks workers, run the same domains sequentially.

## Route

Use the host's worker tools only when delegation is authorized. Otherwise inspect the domains sequentially.
1. Split only independent inspection/inventory domains with no shared mutable state, such as security, architecture, or performance.
2. Give each worker a bounded scope and evidence contract.
3. Deduplicate after every required worker returns or is marked partial.

Each worker returns this compact contract:

| Field | Requirement |
|---|---|
| Scope | Exact files/directories inspected |
| Findings | `file:line`, evidence, impact, confidence, repair move |
| Non-findings | High-risk patterns checked but not found |
| Limits | Missing tools, unreadable paths, or reduced coverage |

Keep target selection, final prioritization, autopsy, and repairs with the parent. The parent verifies load-bearing claims before the roast.

Next: rank the combined inventory with `references/sin-catalog.md` and resume the sequential phases in `references/roast-playbook.md` § 4 Autopsy.
