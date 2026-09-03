# Trajectory Grading
Load when evaluating an agent's sequence of steps or tool calls. Why: final-answer grading misses whether the agent took the right path.

## Reference-free vs. reference-based
- **Reference-based**: compare against a golden trajectory — use when you know the right tool sequence and can maintain it.
- **Reference-free** (LLM judge): ask "was this trajectory reasonable?" — use when golden trajectories are expensive or ambiguous.

Start reference-free to discover failure patterns; add golden trajectories once failure categories stabilize.

## Trajectory match modes
| Mode | Checks | Use when |
|---|---|---|
| **strict** | Same tools, same order, same args | Ordering is a business requirement (policy: step A before B) |
| **unordered** | Same tools, any order | All required tools must be called, order is irrelevant | <!-- style-lint: ignore-line passive-voice -->
| **subset** | Agent called subset of reference | Agent must not call extra tools beyond the reference |
| **superset** | Agent called superset of reference | Key tools required; extra tools acceptable |

Default to **unordered** unless ordering is a stated requirement. Use **strict** only when you can articulate why sequence matters. Grade outcomes, not exact sequences, where possible.

## Tool args match modes
By default: exact match on every argument. Override per tool when strict equality breaks on semantically equivalent inputs:
- `exact` — all arguments must match
- `ignore` — any call to this tool counts; arguments irrelevant
- `subset/superset` — argument subset/superset of reference
- `custom comparator` — per-tool function (handles `"SF" == "San Francisco"`)

Rule: use a custom comparator before failing a match on semantically equivalent args. Strict arg matching is the single largest source of false negatives in trajectory evals.

## Graph trajectory (node steps vs. messages)
For graph-based agents, the trajectory is node steps, not messages:
```
steps: [["__start__", "agent", "tools", "__interrupt__"], ["agent"]]
```
`__interrupt__` = human-in-the-loop gate. Grade whether the agent visited the right nodes across turns, not what it said. Use strict match for deterministic step sequences; LLM judge for flexible ones.

## Multiturn simulation — trajectory generation, not evaluation
To test multi-turn agents without real users: simulate a user (LLM or scripted fixed responses), run app ↔ user for N turns or until a stopping condition, then evaluate the final trajectory.

**Separation rule:** simulation generates trajectories; evaluators run at the end on the frozen trajectory. Never mix trajectory generation with grading — it collapses the harness.

## Evaluator interface contract
All evaluators share one interface: `(inputs, outputs, reference_outputs?)` → `{key, score: bool|float, comment}`.
- `key` — metric name
- `score` — binary (true/false) or continuous (0.0–1.0)
- `comment` — justification (required for LLM judges; None for deterministic)

Stick to this interface for composability across any runner (pytest, CI, LangSmith, manual).

Next: grader mix and G-Eval/DAG → `eval-techniques.md`; graph attribution → `graph-of-loops.md`.
