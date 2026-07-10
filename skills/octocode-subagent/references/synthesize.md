# Synthesize

Load before the parent answers the user. Why: fan-out without a barrier creates false certainty (misalignment, echo, withheld info).

## Barrier
1. List every live worker — note starting / running / idle.
2. Wait or poll until each relevant worker is idle or terminal.
3. Stop+remove workers you will not continue.
4. Do **not** synthesize while needed workers are still starting/running.

## Merge (reducer)
1. Collect result packets; keep `partial` / `blocked` labeled — never average into “complete.”
2. Hunt **conflicts** first; disagreement is a finding.
3. Re-check every load-bearing anchor in the **parent**.
4. Verifier/critic workers start from anchors + acceptance — not the first worker’s prose.
5. One parent answer: conclusion, evidence, gaps, next — no worker transcripts.

## Failure modes to watch
- Empty / missing `return` shape → failed handback; re-ask or replan.
- Context poisoning — do not feed unverified worker claims into the next spawn as facts.
- Task derailment / info withholding — compare packet `goal` vs returned `result`.
- Coordination tax — if merge needs another research campaign, the cut was wrong.

Next: `recovery.md` if blocked; `packets.md` for required shapes.
