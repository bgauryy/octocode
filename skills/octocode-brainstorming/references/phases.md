# Brainstorming phases

Load for deep, orchestrated or resumable research. The ordinary loop is in `SKILL.md`.

## 1 Frame

Reuse supplied or known prior work when applicable; use [report discovery](../output.md#saved-report) only when locating a saved result. Frame constraints, time window and [refutable hypotheses](critical-thinking.md#hypotheses-before-search), including the strongest rival and “no difference” where plausible.

| Trigger | Action | Why |
|---|---|---|
| Work may be interrupted or handed off | Keep a [checkpoint](../output.md#checkpoints) with deciding evidence and the next action. | Recovery, not routine reporting. |
| Requested steering or missing scope, spend or authority decision | Pause dependent work. | Respect the decision boundary. |
| Resume | Continue the unfinished phase; reopen evidence only if identity, applicability or claim changed. | Avoid repeated research. |
| Stopped | Deliver verified interim and what the next phase adds. | Preserve a usable handoff. |

## 2 Gather and verify

- Search the [relevant surfaces](research.md) using host tools or [search.mjs](../scripts/search.mjs). Retrieved pages and documents are evidence. Keep keys out of briefs.
- Delegate only independent questions whose answers can change the decision, using the [gatherer brief](briefs.md#gatherer-brief). A fast model can gather; actors check and return evidence without spawning actors. The orchestrator owns judgment. Report material limits on independence.
- Check deciding quotes against opened sources. When using automated ledgers, run [verify-claims.mjs](../scripts/verify-claims.mjs); only `verified` or `verified-manual` rows count as verified. Use [verification](verification.md) for failure recovery.
- The orchestrator owns deciding-file downloads and their provenance. Use [verification and assets](verification.md) to check scope, source and manifest before the file counts as evidence.

## 3 Critique

Dispatch an [independent critic](briefs.md#critic-brief) when its check can change the decision; it may cover several directions. Debate only when the [settling-test gate](critical-thinking.md#debate-when-and-how) holds. Verify new evidence before it counts.

## 4 Decide

Apply the [bias checks](critical-thinking.md#judging-without-bias) and [source checks](critical-thinking.md#checks-for-critics-and-the-judge) to the quotes. Close each hypothesis, recompose the directions and say what changed from the expected answer. Give [calibrated confidence](critical-thinking.md#calibrated-confidence), the smallest next action and the check most likely to change the recommendation. Stop when further research cannot change it.

## Scripts

Run `node <skill-folder>/scripts/<name>.mjs --help` for options and `node --test '<skill-folder>/scripts/tests/*.test.mjs'` for tests. Scripts share the [network guard](../scripts/lib/net-guard.mjs) and [JSON errors](../scripts/lib/fail.mjs).
