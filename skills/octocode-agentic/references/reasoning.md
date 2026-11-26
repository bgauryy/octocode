Load when an agent must break a problem down or reason through a hard step, or when choosing a reasoning technique: chain of thought, self-consistency, Tree of Thoughts, decomposition or prompt chaining.

# Break down problems and reason

Start with the model's own reasoning (thinking enabled; raise the effort setting before adding a technique) and a clear outcome. Add a technique only for a failure reproduced on a case set ([evaluation](evaluation.md)), and compare it at equal token cost: most published gains shrink or vanish when the baseline gets the same budget.

| Technique | Use when | Measured | Limit |
|---|---|---|---|
| Chain of thought ("think step by step") | A non-reasoning model, or thinking off, on math, logic or symbolic work | +14, +12 and +7 points on symbolic, math and logic tasks; under 1 point elsewhere | On reasoning models: marginal gain, 20–80% more time |
| Self-consistency (sample several answers, take the majority) | The answer is a number or label that agreement can check | +4 to +18 points on 2022 models; saturates at 5–10 samples | One current-model study: +0.4% to +1.6% at near-linear cost |
| Tree of Thoughts (branch, score, backtrack) | A search problem whose partial states can be scored | Game of 24: 4% → 74% on GPT-4 | About 100 times the tokens of one pass; behind self-consistency on weaker models; no cost-matched test on current models |
| Decomposition (least-to-most, plan-and-solve) | A compositional task whose early answers feed later ones | SCAN: 16% → 99% on a 2022 model | At equal budget most of the gain goes; it diminishes on frontier models |
| Prompt chain with gates | You must inspect an intermediate output or enforce a pipeline | First-party guidance, no measurement | Current models do most multistep work in one call |
| Solver or code for the exact step | Arithmetic, parsing, search, constraint solving | Plan plus a symbolic solver beat chain of thought for both steps | Needs a formal statement of the step ([scripts first](patchy-behavior.md#script-first-agent-second)) |

Subtasks unknown in advance and separable go to an orchestrator with workers ([delegation](delegation.md)).

## Break a task down

- Hand the agent a plan only when it was checked: every step names a file, command or interface that exists now, or the plan already succeeded in a run. Over 21,120 coding-agent runs a standard plan raised issue resolution, a subpar plan did worse than no plan, and extra early phases lowered results. Otherwise state the outcome and let the model plan ([guiding agents](guiding-agents.md)).
- Cut where one piece's output can be checked before the next piece uses it: a test, a schema, a verifier. A stage without a check only adds a place to drift.
- Give each piece one owner, its input and a checkable output ([parallel or in sequence](delegation.md#delegate-only-separable-work)).
- Remind the agent of the plan as [steer during a long run](guiding-agents.md#steer-during-a-long-run) describes: periodic reminders reduced plan violations in the same study.
- Plan files are needed only across context windows ([compaction](compaction.md#checkpoint-template)): within one session the newest models track multistep work without a todo list (Anthropic).

## Stated reasoning is not evidence

A model's visible reasoning often leaves out what drove its answer: in hint experiments models mentioned the deciding hint in under 20% to 39% of cases. It is more faithful when the task cannot be done without the reasoning. Verify the result against a test or source, and never cite the explanation as proof of why the answer came out.

## Sources

- Chain of thought: [Sprague et al.](https://arxiv.org/abs/2409.12183), [Meincke et al.](https://arxiv.org/abs/2506.07142), [OpenAI: reasoning best practices](https://developers.openai.com/api/docs/guides/reasoning-best-practices)
- [Self-consistency](https://arxiv.org/abs/2203.11171), [Tree of Thoughts](https://arxiv.org/abs/2305.10601), [least-to-most](https://arxiv.org/abs/2205.10625); equal budget: [Wang et al.](https://arxiv.org/abs/2406.06461), [Loo](https://arxiv.org/abs/2511.00751), [Madhwal et al.](https://arxiv.org/abs/2602.04853)
- Plans: [Liu et al.](https://arxiv.org/abs/2604.12147), [Anthropic: building effective agents](https://www.anthropic.com/research/building-effective-agents), [todo tracking](https://code.claude.com/docs/en/agent-sdk/todo-tracking)
- Faithfulness: [Chen et al.](https://arxiv.org/abs/2505.05410), [Emmons et al.](https://arxiv.org/abs/2507.05246)
