# Roast Playbook

Load when the target is clear; it runs the lobby flow phase by phase.

## 1. Target

Order: explicit files/directories/symbols/lines, then an explicitly requested diff/branch scope, then the whole repository on request.

## 2. Inspect

Cover structure, search, semantics, reachability, and affected scope.

## 3. Inventory

Rank with `references/sin-catalog.md`. If the inventory obscures the decision, show the highest-value findings by impact and confidence and summarize the overflow.

Finding shape:
```text
{severity}. {title} — `path:line`
Evidence/mechanism: {what the code does}
Impact: {observable consequence}
Confidence: {high|medium|low}
Repair: {smallest safe move}
Roast: {one evidence-specific line}
```

## 4. Autopsy

Choose the highest-impact offender, not the funniest one.
Break it into responsibilities or failure paths; quantify only measured properties.
Explain why it is risky, where contracts cross, and how the repair can be staged.

## 5. Checkpoint

Summarize the important findings and repair paths. A critique-only request ends with that result; continue to scoped repairs when the request already includes fixes.

Arrange the response around the highest-impact findings. Include an autopsy or debt inventory when it helps the reader; no fixed output order is required.

Next: on a selected repair, load `references/redemption-flow.md`.
