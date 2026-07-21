# Subagent Rubber-Duck Review

Use this when a hard explanation, risky decision, recurring weakness, or important reflection benefits from a real second agent.
Skip it when an internal role pass or direct test is cheaper; `reflect record --duo` returns prompts but launches no subagent. If the host cannot spawn one, say so; never label a fallback as subagent review.

Duck mode is always read-only. It may inspect source and Awareness leads, but it must not edit, claim files, or write durable rows.

## Loop

```text
FRAME -> EXPLAIN -> DUCK RESTATES -> CHALLENGE -> REVISE -> VERIFY -> CAPTURE
```

1. **Frame** one question, decision, or failed assumption; include acceptance criteria and relevant files/row IDs.
2. **Explain locally** the facts, hypothesis, confusion, and proposed action; keep the final recommendation out of the duck's first message.
3. **Dispatch** one subagent with the question, criteria, source packet, read-only constraint, and output contract.
4. **Restate first**: the duck independently describes the problem and expected behavior from evidence.
5. **Challenge**: after the restatement, send the current hypothesis/recommendation; the duck checks assumptions, contradictions, missing evidence, edge cases, and falsification options.
6. **Revise**: the main agent compares both models, preserves dissent, and chooses one decision or hypothesis.
7. **Verify** with one concrete source/test/command check. If unavailable or inconclusive, preserve dissent and make no verified capture; route pending work only when it has an owner.
8. **Capture** only verified synthesis as none, memory, refinement, or signal; then close via `references/learning-loop.md`. Agreement is never the check.

## Checks

| Check | Pass condition |
|---|---|
| Independence | Duck inspected the evidence and restated the problem before critique. |
| Scope | One question; one duck by default; second specialist only for a distinct risk. |
| Evidence | Claims cite current files, commands, tests, or row IDs. |
| Dissent | Unresolved objections remain visible instead of forced consensus. |
| Verification | Main agent names and runs the decisive check. |
| Write safety | Duck stays read-only for the entire review. |
| Stop | End after one pass; repeat only when new evidence changes the model. |

## Output Contract

```text
Restatement: <problem + expected behavior>
Assumptions challenged: <bullets>
Evidence checked: <files/commands/row IDs>
Dissent or missing evidence: <bullets>
Best falsification check: <one check>
Recommendation: <decision or revised hypothesis>
Capture candidate: none | memory | refinement | signal
```

Same-turn: return the packet directly; never signal just to mirror chat. Async/cross-run: use a signal or refinement.
After the loop, edits require user/task authorization and a separate workflow: distinct agent id, disjoint locks, own verification, and handoff.
Locks coordinate ownership; they never authorize edits.
