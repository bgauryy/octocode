# Subagent Communication
Load when defining how parent ↔ workers talk during an evaluated run. Why: bad communication creates false certainty and unattributable failures.

## Contract (eval-facing)
| Channel | Rule |
|---|---|
| **Downlink (parent→worker)** | Sealed packet only — goal, anchors, scope, acceptance, return shape. No parent chat dump. |
| **Uplink (worker→parent)** | Result packet — status, result, ≤8 evidence anchors, verification, confidence, next. |
| **Lateral (worker↔worker)** | Default **off**. Parent is the bus; peer chat hides conflicts from the barrier. |
| **User** | Parent owns you unless a handoff packet explicitly transfers. |

## Barrier communication checklist
1. `list` live workers before any final answer.
2. `wait` / poll until needed workers are idle or terminal.
3. Do not synthesize while status is starting/running.
4. Label `partial` / `blocked` — never average into `complete`.
5. Re-check load-bearing anchors in the **parent** context.
6. Verifier/critic gets **anchors + acceptance**, not the first worker’s prose.

## Message hygiene
- Steer once on derailment; else stop and replan — do not replay the same packet.
- Idle ≠ done — check acceptance criteria.
- After session reload, spawn fresh ids — stale registries lie.
- Map host `input-required` / auth gates to parent/user — never auto-continue past policy.

## Eval sensors on communication
| Sensor | Fail if |
|---|---|
| Return-shape rate | Missing `status`/`evidence`/`verification` |
| Conflict visibility | Merge hides disagreement |
| Context poison | Unverified worker claim fed as fact into next spawn |
| Barrier breach | Answer emitted while workers still running |

Mechanics map to host APIs through `octocode-subagent` `coordinate.md` / `packets.md` / `synthesize.md`.

Next: approaches → `references/subagent-approaches.md`; protocol → `references/subagent-protocol.md`.
