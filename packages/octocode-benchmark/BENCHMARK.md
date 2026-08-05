# Benchmark design

This package measures repository research and code understanding through competing CLIs. It does not measure patching or test execution.

> **This is the canonical description of the run flow.** The README, `INSTRUCTIONS.md`, and the skill point here rather than restating it — edit the flow **once, here**.

**Each question is worked by three separate people/agents, each working alone:** Runner A (baseline CLI), Runner B (Octocode CLI), and the Grader. The two runners get the same question, budget, and frozen refs — only the assigned CLI differs — and neither can see the other or the grader. Both answers are finished before the grader (who never saw either runner work) starts, researches independently, grades each on its own, then compares them.

```text
runner A (baseline CLI)  ─┐
                           ├─ two answers, tool names hidden ─→ grader ─→ scored comparison
runner B (Octocode CLI)  ─┘        (three separate people/agents per question)
```

Keeping the roles separate and blind is what makes the numbers trustworthy: don't reuse one person/agent across roles. Questions contain no answer key — the grader establishes ground truth by its own research, so no one is grading against a supplied answer.

Questions live only as markdown under `compare/` — the GitHub matchups share one canonical set in `github-questions/`, and any corpus-local matchup keeps its own `questions/`. Before research, each runner receives the fixed assigned-arm primer from [`RUNNER_TOOL_CONTEXT.md`](RUNNER_TOOL_CONTEXT.md); this setup context is excluded from CLI-output totals, while runner-invoked help or schema calls are counted. The Octocode arm is always `npx octocode tools …`. Every question is worked; contaminated or unresolved ones are reported in a separate diagnostic slice, not dropped. A single pass is a snapshot — repeat it for a stable claim.

---

## Results

The current rollup is [`results/SUMMARY.md`](results/SUMMARY.md). It uses the
latest complete report for each matchup and does not pool invalid, incomplete,
or methodologically incompatible campaigns into one synthetic total.

Efficiency is the count of Unicode characters the CLI delivers into the agent's
context — the budget that funds reasoning and crowds out attention when spent.
It is a deterministic, tokenizer-independent unit, not a token, latency, or cost
estimate. **Correctness is graded first**: fewer characters break an essentially
equal-correctness tie, but cannot rescue a materially wrong answer.

Current evidence is mixed rather than universal:

- Octocode is smaller on aggregate in the latest complete `gh`, RTK, and
  Headroom campaigns.
- Correctness is near-parity and pass-dependent: in the latest two-pass RTK
  campaign Octocode was strictly more correct in one pass (Q13) and tied in the
  other, while staying far leaner throughout.
- In the latest strict three-pass Headroom campaign, Octocode wins 29–22 at
  higher correctness and uses 62.2% fewer delivered characters.

The repeated mechanism is task-shaped: targeted region reads help on multi-hop
file/diff research, while a small direct `gh` lookup can be leaner when the
location is already known. Treat this public 20-question suite as comparative
orientation, not a product shipping gate.
