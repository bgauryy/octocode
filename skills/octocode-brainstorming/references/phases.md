# Brainstorming phases

Load for deep, delegated or resumable research. The ordinary loop is the steps in `SKILL.md`; this page adds orchestration, checkpoints and recovery to them.

## Frame and diverge

Steps 2–4. Reuse supplied or known prior work when applicable; use [report discovery](../output.md#saved-report) only when locating a saved result. Record each option with its origin (user, yours, found), then its claim, strongest rival and refuting observation ([hypotheses](critical-thinking.md#hypotheses-before-targeted-search)).

| Trigger | Action | Why |
|---|---|---|
| Work may be interrupted or handed off | Keep a [checkpoint](../output.md#checkpoints) with deciding evidence and the next action. | Recovery, not routine reporting. |
| Requested steering or missing scope, spend or authority decision | Pause dependent work. | Respect the decision boundary. |
| Resume | Continue the unfinished phase; reopen evidence only if identity, applicability or claim changed. | Avoid repeated research. |
| Stopped | Deliver verified interim and what the next phase adds. | Preserve a usable handoff. |

## Research and validate

Steps 5–6.

- Write one [research question](briefs.md#research-question) per claim or sub-question. Delegate only independent questions whose answers can change the decision, each with the [gatherer brief](briefs.md#gatherer-brief) and its own ledger path. A fast model can gather; workers return evidence without spawning actors. The orchestrator owns judgment and reports material limits on independence.
- Search the [relevant surfaces](research.md) with host tools or [search.mjs](../scripts/search.mjs) run from the workspace root. Keep keys out of briefs.
- Rerun [verify-claims.mjs](../scripts/verify-claims.mjs) on every worker ledger before counting it: `node <skill-folder>/scripts/verify-claims.mjs --ledger <worker-1.jsonl> --ledger <worker-2.jsonl>` counts each ledger and the total. A worker's "verified" is a claim, not a result. Apply the [validated-source checks](verification.md#validated-sources) to each deciding row.
- The orchestrator owns deciding-file downloads and their provenance: [verification and assets](verification.md#when-a-deciding-source-is-a-file).

## Contradict

Step 7. Run the [contradiction pass](critical-thinking.md#contradict-and-prove) on every deciding claim. Dispatch an [independent critic](briefs.md#critic-brief) when its check can change the decision; it may cover several directions. Debate only when the [settling-test gate](critical-thinking.md#debate-when-and-how) holds. Verify new evidence before it counts.

## Decide

Step 8. Apply the [bias checks](critical-thinking.md#judging-without-bias) and [claim checks](critical-thinking.md#checks-for-critics-and-the-judge) to the verified quotes. Close each hypothesis with its status, recompose the sub-answers and say what changed from the expected answer. Give [calibrated confidence](critical-thinking.md#calibrated-confidence), the smallest next action and the check most likely to change the recommendation. Stop when further research cannot change it.

## Scripts

Run `node <skill-folder>/scripts/<name>.mjs --help` for options and `node --test '<skill-folder>/scripts/tests/*.test.mjs'` for tests. Scripts share the [network guard](../scripts/lib/net-guard.mjs) and [JSON errors](../scripts/lib/fail.mjs).
