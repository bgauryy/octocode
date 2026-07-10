# Recovery

Load when a worker stalls, fails, or conflicts. Why: retrying the same packet wastes budget.

## Escalation
1. **Retry** — same agent, tighter acceptance, once.
2. **Replan** — rewrite the brief from the failure reason.
3. **Decompose further** — split the failed unit; do not enlarge the swarm blindly.
4. **Escalate model** — one tier up (`model-routing.md`).
5. **Kill + parent** — after one steer/replan failure, finish in parent.

## Synthesis / verifier
- Barrier + merge: `synthesize.md`.
- Verifier starts from anchors + acceptance — never from the worker’s unverified prose.
- Partial/blocked stay labeled; do not echo shared errors into higher confidence.

## Hygiene
- `abort` interrupts a turn; `kill` ends the process.
- Always `list` before claiming the campaign done.
- Preserve useful partial output when killing.
- Empty final / missing return shape → failed handback.

## Watch (MAST-style)
Task derailment · fail-to-clarify · info withholding · context poisoning · conversation reset · reasoning/action mismatch.

## ASI boundary
Bounded harness improvement (`improve-loop.md`) only. Reject unbounded recursive self-modification of models/weights/policies.

Next: `synthesize.md` · `awareness.md`.
