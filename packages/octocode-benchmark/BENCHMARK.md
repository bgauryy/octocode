# Benchmark design

This package measures repository research and code understanding through competing CLIs. It does not measure patching or test execution.

**Each question is worked by three separate people/agents, each working alone:** Runner A (baseline CLI), Runner B (Octocode CLI), and the Grader. The two runners get the same question, budget, and frozen refs — only the assigned CLI differs — and neither can see the other or the grader. Both answers are finished before the grader (who never saw either runner work) starts, researches independently, grades each on its own, then compares them.

```text
runner A (baseline CLI)  ─┐
                           ├─ two answers, tool names hidden ─→ grader ─→ scored comparison
runner B (Octocode CLI)  ─┘        (three separate people/agents per question)
```

Keeping the roles separate and blind is what makes the numbers trustworthy: don't reuse one person/agent across roles. Questions contain no answer key — the grader establishes ground truth by its own research, so no one is grading against a supplied answer.

Questions live only as markdown in each comparison's `questions/` folder. The Octocode arm is always `npx octocode tools …`. Every question is worked; contaminated or unresolved ones are reported in a separate diagnostic slice, not dropped. A single pass is a snapshot — repeat it for a stable claim.
