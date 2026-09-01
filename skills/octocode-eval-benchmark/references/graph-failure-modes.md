# Graph Failure Modes
Load when evaluating a multi-agent graph for structural failure risks. Why: topology alone doesn't buy correctness.

## 1. Shared context
A verifier receiving the executor's conversation is not independent and may repeat the same failure.

**Sensor:** does each verifier node start with fresh context, grading only the artifact? Require this before calling a result verified.

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
A graph without anchors will drift. Every graph needs at least one node whose output cannot be argued with: a test that actually ran, a build exit code, a type error.

**Anchors to require:**
- Tests that actually ran with exit codes
- A verifier on deterministic evidence (not LLM opinion of LLM output)
- At least one rule agents are never allowed to tune

Other modes: opaque state (no typed snapshot) · no checkpoint/resume · unbounded tool permissions · missing human gates. Add suite cases on first trace appearance.

Next: KPI placement and attribution → `graph-of-loops.md`; inner loop sensors → `feedback-loops.md`.
