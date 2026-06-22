# Advocate vs Critic — the debate

Load at the converge step (Workflow step 5), before running the debate. The goal is **not** to collect a pro list and a con list; it is to make the two agents *reason against each other* so only claims that survive scrutiny reach the verdict — then keep the **best of both**. Every claim must carry its reasoning *and* a citation; an assertion with neither is dropped before it counts.

## Round 1 — opening cases (same evidence, dispatch together)

> **ADVOCATE** for "<idea>" — strongest case FOR. Each claim: assertion → *because* (reasoning) → citation (repo/package/web/local). Bull case only.

> **CRITIC** of "<idea>" — strongest case AGAINST: crowded competitors, abandoned repos, complaints, unsolved problems. Each claim: assertion → *because* → citation. Bear case only.

## Round 2 — rebuttal (each agent receives the *other's* Round-1 case)

Rebut specific claims with evidence, **concede** what you cannot refute (say so explicitly), and attack the weakest-supported claim. New citations only — no repeating Round 1.

> **ADVOCATE rebuttal** — answer the Critic's strongest points; concede what holds.

> **CRITIC rebuttal** — answer the Advocate's strongest points; concede what holds.

## Referee / best-of-both (main agent)

Keep every claim that *survived* rebuttal (→ high-confidence), drop every claim that was *conceded*, and mark every claim that stayed *contested* as a decision point. The verdict is the strongest defensible position assembled from **both** sides — not whoever shouted louder.

Record the **decision delta**: which claims flipped, which were conceded, which stayed contested, and who had the better evidence.

## No delegation tool

Run the four passes sequentially with the labels above, feeding each agent the prior pass verbatim. **Budget:** the debate is ~4 worker dispatches — count it against the 5-worker ceiling (Hard Gate 4); if web slices already spent the budget, run a single rebuttal round (one Advocate-rebuts + one Critic-rebuts) and note "debate shortened (budget)".
