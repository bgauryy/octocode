# Techniques
Load when quality risk needs a second mind without enlarging the swarm. Why: topology alone misses hidden assumptions, echo chambers, and weak verification (MAST FC2/FC3).

## Catalog → when → load
| Technique | When | Ref |
|---|---|---|
| **Rubber duck** | Stuck plan; surface assumptions cheaply | `references/rubber-duck.md` |
| **Interview** | Another agent’s claims need falsification | `references/interview.md` |
| **Mimic flow** | Worker should follow a playbook without full chat | `references/mimic-flow.md` |
| **Red team / devil’s advocate** | Plan feels “too clean”; attack before ship | `references/red-team.md` |
| **Premortem** | High-stakes change; imagine failure first | `references/red-team.md` |
| **Steelman** | Contested choice; strengthen the opposing case | `references/red-team.md` |
| **Blind review** | Critic must judge the artifact, not the author story | `references/blind-review.md` |
| **Citation scrub** | Claims need source locations before publish | `references/blind-review.md` |
| **Consensus / self-consistency** | Ambiguous solve; independent retries then majority | `references/consensus.md` |
| **Round-robin critique** | Need sequential propose→critique→revise | `references/consensus.md` |
| **Verifier-critic** | Quality bottleneck; independent check with anchors | `references/patterns.md` + fresh context |
| **Perspective debate** | Contested product/tech idea (not code claims) | `octocode-brainstorming` debate.md |
| **Scout fan-out** | Breadth-first research; parallel independent angles | `references/patterns.md` parallel fan-out |
| **Local Ollama offload** | Save tokens; tool-less one-shot (not a challenge technique) | `references/local-ollama.md` |

Common topologies (supervisor, plan-and-execute, router, handoffs, pipelines) live in `references/patterns.md` — not duplicated here.

## Hard rules
1. Fresh context for duck / interviewer / critic / red-team — never feed the first worker’s full transcript as “truth.”
2. Parent adjudicates; agreement is **not** proof.
3. One technique at a time unless independence requires parallel critics.
4. Measure usefulness with `octocode-eval-benchmark` when looping these into a harness.

## Earn the spawn
Prefer parent self-check → rubber duck → interview / red-team → blind review → verifier with anchors → consensus only if still ambiguous. Do not stack by default.

Next: pick one row; spawn → `references/spawn-gate.md` · packets → `references/packets.md`.
