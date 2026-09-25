# Graph failure modes
Load when evaluating a multi-agent graph for structural failure risks. Why: topology alone doesn't buy correctness.

## 1. Shared context
A verifier receiving the executor's conversation is not independent and might repeat the same failure.

**Sensor:** does the judge start fresh with task + rubric + sealed artifact/evidence? A recorded trajectory may be evidence, but inherited executor instructions are not judge authority. Check filesystem/tool isolation with `clean-lab.md` and accuracy with `llm-judge.md`.

## 2. Race conditions
Agents writing to shared state (file, git workspace, API resource) overwrite each other. This is an operational failure — prompting cannot fix it. Before fanning out, answer:
1. Where does each agent work? (isolated directory, worktree, resource?)
2. How do results merge? (who owns the merge step?)
3. What happens when two agents disagree?

If you cannot answer all three, fix isolation first.

## 3. Goodhart's Law
A loop with one metric can hit that metric while the real goal degrades (support bot: resolution rate up, satisfaction down). The loop cannot see outside its own metric.

**Protection:** for every primary KPI, name a counter-metric guardrail the agent cannot tune. Primary improving + guardrail degrading → stop and reframe the goal, not the loop.

## 4. Missing anchors
Anchors prevent graph drift. Use direct outcome evidence where available, such as executed tests or inspected state; use calibrated judgment for properties that cannot be checked deterministically.

**Useful evidence:**
- Tests that ran with exit codes
- A verifier on deterministic evidence (not LLM opinion of LLM output)
- Frozen evaluation criteria that the subject cannot change

Other modes: opaque state (no typed snapshot) · no checkpoint/resume · unbounded tool permissions · missing human gates. Add suite cases on first trace appearance.

Next: KPI placement and attribution → `graph-of-loops.md`; inner loop sensors → `agent-loop.md`.
