# Roast Playbook

Load for a complete critique. Why: enforce scope, evidence, triage, autopsy, and the pre-fix checkpoint.
Flow matches the lobby: `TARGET → INSPECT → INVENTORY → AUTOPSY → CHECKPOINT → REDEEM`.

## 1. Target
Use explicit files/directories/symbols/lines first, then an explicitly requested diff/branch scope; inspect the whole repository only when requested.
Do not widen a provided target. Stop if it resolves to no files.

## 2. Inspect
Use `octocode-research` for structure, search, semantics, reachability, and affected scope; otherwise mark reduced coverage.
Pattern matches are leads. Upgrade every cited finding with exact anchor, mechanism, impact, confidence, and repair move.
Never reveal credential values or infer compromise from a literal alone.

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
Explain why it is risky, where contracts cross, and how the repair can be staged. <!-- style-lint: ignore-line passive-voice -->

## 5. Checkpoint
Summarize important versus redundant findings, then stop.
Load `redemption-flow.md` only when you selects a repair path.

Output: top roast, important findings, autopsy, repair paths, and fix checkpoint. Include redundant/low-value findings only when useful.

Next: rank the inventory with `references/sin-catalog.md` (ecosystem leads: `references/language-sins.md`); delegate evidence with `references/octocode.md`; split a monorepo pass with `references/parallel-roasting.md`; on a selected repair load `references/redemption-flow.md`.
