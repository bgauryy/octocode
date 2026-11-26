Load when dispatching any worker (diverger, gatherer, critic, debater or judge): who sees what, what each writes and what you recheck.

# Workers

You are the orchestrator: you frame, plan, pool, judge and decide; workers only generate, gather or check. Each worker gets a brief ([diverger and critic](briefs.md), [gatherer](brief-gatherer.md), [debaters and judge](briefs-debate.md)) and sees nothing else of the conversation.

| Role | Dispatch when | Sees | Writes | You then |
|---|---|---|---|---|
| Diverger | Step 3, one per pass that can change the decision | The frame and its own lens, persona or constraint; never another pass | `<run>/passes/<pass>.jsonl` | Pool with `pool-options.mjs`, judge `candidates`, keep origins |
| Gatherer | Step 6, one per worker group of the plan; again in step 7 for `deepen` claims | Its plan rows | `<run>/ledgers/<worker>.jsonl` | Rerun `verify-claims.mjs` on the ledger, join it with `research-tree.mjs` and apply the [validated-source checks](verification.md#validated-sources) |
| Critic | Step 8, when its check can change the decision | The conclusion and verified ledger, before others' verdicts | Issues with repairs | Verify new evidence before it counts |
| Debaters, judge | Only when the [debate gate](judging.md#debate-when-and-how) holds | The resolution and verified ledger | Cases; a verdict | Recheck a close verdict in reverse order |

- Dispatch independent workers in one batch; a worker never reads another's output before committing its own.
- A worker's rating, ranking, confidence or "verified" is a claim, not a result: count only what you re-verified ([evidence](method-evidence-judging.md#self-evaluation)).
- Use a different model for a diverger or critic when the host offers one: pooling across vendors widens options ([evidence](method-evidence.md#independent-passes)); for critics the gain is untested ([evidence](method-evidence-judging.md#model-heterogeneity)).
- Workers never spawn actors, edit another worker's file or see keys. A worker that returns nothing usable gets one re-brief naming the gap; after that its claims are named gaps.
