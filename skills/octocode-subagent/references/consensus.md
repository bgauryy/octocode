# Consensus
Load when one solve is ambiguous and independent retries can reduce noise. Why: self-consistency / majority over isolated rolls beats one confident wrong answer — still not proof without anchors.

## Variants
| Variant | When | Do |
|---|---|---|
| **Self-consistency** | Same hard question; stochastic answers | N independent workers, same sealed packet; parent majority / cluster |
| **Round-robin** | Need improvement trail | A proposes → B critiques → C revises (fresh each hop) |
| **Jury** | Binary ship/no-ship | Odd N voters; parent breaks ties with anchors |

## Packet rules
- Identical `goal` / `acceptance` / `return` across parallel voters.
- **No lateral chat** between voters (independence is the method).
- Round-robin: pass only artifact + critique summary forward — not full prior transcripts.

## Parent merge
1. Cluster answers; treat minority dissent as a finding.
2. Re-check load-bearing anchors yourself on the winning cluster.
3. If split is deep (no majority, conflicting anchors) → stop; interview or red-team the split — do not average prose.
4. Budget: default N=3; raise only when value pays (multi-agent token cost is high).

## Rules
1. Consensus without anchors is still a claim.
2. Do not use consensus to paper over missing tests.
3. Prefer verifier-with-anchors over large N when the check is deterministic.
4. Scout fan-out for research breadth ≠ consensus — scouts have **different** goals; voters have the **same**.

## When NOT
- Cheap deterministic check exists → run it.
- Workers would share mutable writes → `references/workspace.md` first.
- Single clear specialist task → one worker + barrier.

Next: techniques → `references/techniques.md`; barrier → `references/synthesize.md`.
