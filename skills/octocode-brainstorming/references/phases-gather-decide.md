Load for steps 6–9 of deep or delegated research: gathering with workers, combining and deepening, checking and deciding. Steps 2–5, checkpoints and recovery are in [phases](phases.md).

# Brainstorming phases: gather, combine, check and decide

## Gather: leads, then findings

Step 6. Delegate only independent groups whose answers can change the decision, each with the [gatherer brief](brief-gatherer.md#gatherer-brief), its plan rows and its own ledger path `<run>/ledgers/<worker>.jsonl`. Small scopes and a fast model suit gathering; you keep judgment. Keep keys out of briefs. A gatherer copies each deciding quote from `read-page.mjs` into its [ledger](claim-ledger.md) with `decides` set to the claim id, and runs `verify-claims.mjs` on that ledger. The orchestrator owns deciding-file downloads and their provenance: [verification and assets](verification.md#when-a-deciding-source-is-a-file).

## Combine and deepen

Step 7. Rerun [verify-claims.mjs](../scripts/verify-claims.mjs) on every worker ledger before counting it ([why](workers.md)), once per ledger because `--out` takes one: `node <skill-folder>/scripts/verify-claims.mjs --ledger <run>/ledgers/<worker>.jsonl --out <run>/ledgers/<worker>.verified.jsonl` (`--reuse` that output on reruns, `--since` for the window; [statuses and provider copies](verification.md#run-the-verifier)). Repeated `--ledger` without `--out` counts each ledger and the total. Join the verified copies to the plan with `research-tree.mjs --plan <run>/plan.jsonl --verified <run>/ledgers/<worker>.verified.jsonl`, one `--verified` per worker; its `deepen` list is the next round, briefed with what each claim still lacks, crossing web and GitHub. `unplanned` rows belong to no claim: map them to one or drop them. [Combine and deepen](research-github.md#combine-and-deepen) says what each round reads next and when to stop a claim.

## Check

Step 8. Apply every [validated-source check](verification.md#validated-sources) to each deciding row. Run the [contradiction pass](proof-bar.md#contradict-and-prove) on every deciding claim, judging it against its supporting and disconfirming rows. Mark the status on its ledger rows (`decides`, `direction`, `claimStatus`) and rerun the verifier on all ledgers (repeated `--ledger`, no `--out`, `--reuse` a [concatenated verified copy](verification.md#run-the-verifier) so no page is fetched twice): its `decidingClaims` gives each claim's support, refutation, sources and conflicting statuses. Dispatch an [independent critic](briefs.md#critic-brief) when its check can change the decision. Debate only when the [settling-test gate](judging.md#debate-when-and-how) holds. Verify new evidence before it counts.

## Decide

Step 9. Build the option table from `decidingClaims` and compare options on the same criteria, one at a time. Apply the [bias checks](judging.md#judging-without-bias) and [claim checks](judging.md#checks-for-critics-and-the-judge) to the verified quotes. Close each claim with its status, [recompose](critical-thinking.md#decompose-then-recompose) the sub-answers and say what changed from the expected answer. Give [calibrated confidence](proof-bar.md#calibrated-confidence), the smallest next action and the check most likely to change the recommendation. Stop when further research cannot change it.
