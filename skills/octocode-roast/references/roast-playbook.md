# Roast Playbook

Load for a complete critique. Why: enforce scope, evidence, triage, autopsy, and the pre-fix checkpoint.
Flow matches the lobby: `TARGET → INSPECT → INVENTORY → AUTOPSY → CHECKPOINT → REDEEM`.

## 1. Target
Use explicit files/directories/symbols/lines first, then an explicitly requested diff/branch scope; inspect the whole repository only when requested.
Do not widen a provided target. Stop if it resolves to no files.

## 2. Inspect
Use `octocode-research` for structure, search, semantics, reachability, and blast radius; otherwise mark reduced coverage.
Pattern matches are leads. Upgrade every cited finding with exact anchor, mechanism, impact, confidence, and repair move.
Never reveal credential values or infer compromise from a literal alone.

## 3. Inventory
Group by severity from the lobby:
- Capital: confirmed security, data loss/corruption, auth bypass, or exploitable correctness.
- Felony: high-impact performance, concurrency, safety, coupling, or change-blocking design.
- Crime: real type/error/test/maintainability defects.
- Slop/Misdemeanor: noisy structure, naming, residue, or taste.
At 20+ findings, show the top ten by impact and confidence and state the overflow count.

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
Summarize important versus redundant findings, then stop.
Load `redemption-flow.md` only when the user selects a repair path.

Output: `Top roast`, `Important findings`, `Redundant / low-value`, `Autopsy`, `Redemption paths`, `Fix checkpoint`.
