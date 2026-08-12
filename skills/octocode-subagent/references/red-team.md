# Red Team
Load when a plan or design is “too clean” and needs attack before ship. Why: agreement and polite merge miss failure modes that an adversarial role surfaces early.

## Variants → when
| Variant | When | Ask the worker to |
|---|---|---|
| **Devil’s advocate** | Default adversarial pass | Argue the plan is wrong; list kill-shots |
| **Premortem** | High-stakes change | “It failed six months later — write the postmortem now” |
| **Steelman** | Contested decision | State the strongest opposing case before rebuttal |
| **Red team** | Security/abuse focus | Find exploit paths, abuse cases, privilege mistakes |

## Packet (fresh worker)
- `goal`: attack / premortem / steelman — do **not** implement the original task
- `context`: sealed plan or artifact + acceptance criteria (no author chat)
- `scope`: failure modes, kill-shots, unanswered risks; max N findings ranked
- `acceptance`: ranked risks with severity + what would falsify each
- `return`: `attacks` · `severity` · `falsifiers` · `keep_or_kill` · `next`

## Rules
1. Fresh context — no author chain-of-thought dump.
2. Parent must answer top kill-shots or explicitly defer before shipping.
3. “Looks fine” without attacks → failed red-team; re-ask with sharper scope.
4. Pair with anchors (tests/build) when attacks are technical — prose alone is weak.
5. Cap to one adversarial round unless new kill-shots appear.

## When NOT
- Need assumption surface only → `references/rubber-duck.md`.
- Need claim-by-claim falsification of another agent → `references/interview.md`.
- Idea-space lenses (architect/entrepreneur/product) → brainstorming debate.

Next: blind artifact check → `references/blind-review.md`; catalog → `references/techniques.md`.
