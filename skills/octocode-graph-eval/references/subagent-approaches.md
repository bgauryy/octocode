# Subagent Approaches
Load when choosing a multi-agent topology for an evaluated experiment. Why: pattern choice changes which KPIs and checks matter.

## Common approaches → when → measure
| Approach | When | Primary focus | Extra checks |
|---|---|---|---|
| **Solo / ReAct** | Default; one context fits | Node case score | Do not spawn |
| **Parallel fan-out** | Independent probes | e2e + merge quality | Barrier; collisions=0 |
| **Supervisor + specialists** | Multi-turn; parent keeps user | e2e + per-specialist leading | Packet completeness |
| **Plan → execute** | Planning is the bottleneck | Plan acceptance → exec pass | Plan held-out; no plan rewrite mid-run |
| **Verifier-critic** | Quality bottleneck | Precision/recall of accepts | **Fresh-context** verifier |
| **Sequential pipeline** | Each stage needs prior artifact | Stage sensors + e2e | Real edges only |
| **Router → one specialist** | Clear verticals | Router accuracy + specialist e2e | Misroute rate guardrail |
| **Handoff** | Specialist owns next user turns | Task completion + return rule | Terminal/return enforced |
| **A2A remote peers** | Independent remote agents | e2e + auth/gate compliance | No auto-continue past gates |

## Best approaches (defaults)
1. **Earn spawn** — solo unless isolation, parallel width, or specialty changes the outcome KPI.
2. **Strengthen verifier before adding a node** — more workers ≠ more truth.
3. **One objective per worker** — broad packets destroy attribution.
4. **Outcomes over paths** — grade results/anchors, not exact tool sequences (unless policy order is the contract → strict trajectory).
5. **Freeze harness** during the experiment; evolve topology only between experiments.
6. **Anchor first** — tests/build/types before narrative critics.
7. **Parent synthesizes** — workers never silently become the user-facing voice.

## Anti-patterns
- Fake parallelism on a true chain (coordination tax, zero speedup).
- Shared-context “verifier” (graph agreeing with itself).
- Unbounded swarm for production coding.
- Optimizing spawn count or latency while e2e quality drops (Goodhart).

Spawn/pattern catalog detail → `octocode-subagent` `patterns.md` / `spawn-gate.md`. Graph measurement → `references/graph-of-loops.md`.

Next: protocol → `references/subagent-protocol.md`; KPIs → `references/subagent-kpis.md`.
