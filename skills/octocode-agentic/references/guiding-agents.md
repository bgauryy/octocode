Load when writing the standing instructions that steer an agent: how specific to be, persistence and stop rules, reminders during a long run, or the brief for a subagent.

# Guide an agent through prompts

## Pick the specificity

- Aim between two failures: hardcoded branches and edge-case lists that break on the first new case, and vague guidance that assumes context the agent lacks. State the goal, the heuristics for judging, and what done means. Minimal means the smallest set that fully describes the expected behavior, which is not always short.
- Prescribe more for a routine with fixed steps or wording, a weaker or more literal model, or an order that guards an effect. Prescribe less for a strong reasoning model on open-ended work: guidance that helped an older model can overconstrain a newer one.
- No measurement compares the two styles, so try both on the target model with the same cases.

## What to state

| State | Evidence and limit |
|---|---|
| The outcome, who it is for, constraints, and what done includes (implement, run, inspect, fix) | Current guides keep completion criteria and persistence; without them agents stop early |
| When to act, when to ask, and which effects are allowed | The agent cannot infer authority |
| When each tool applies, in plain words | "If in doubt, use the tool" makes current models overtrigger |
| Not by default: a plan before acting | Raised GPT-4.1's pass rate 4%; OpenAI's coding-model guide says to remove upfront-plan prompts because the model can stop abruptly |
| Not by default: thoroughness ("be thorough", "always run tests") | Written for older models; on current ones it causes repeated searches and unneeded tests, so reduce it. A check belongs in the definition of done for this task ("done when the changed tests pass"), not in a standing habit |

One measurement exists: persistence, tool-use and planning reminders together raised GPT-4.1's internal SWE-bench Verified score by close to 20%. Newer guides soften or withdraw two of the three and publish no number, so rerun your own cases after a model change.

## Steer during a long run

- When behavior drifts, restate the goal and constraints or name the specific behavior. In 28-turn persona conversations re-injecting instructions cut drift 35–38% and a behavior-specific instruction 87%; none removed it, and a fixed schedule did as well as adaptive timing. This is not measured on tool-using runs.
- Drift follows the pattern of earlier turns more than distance from the instruction, and a strong model inherits drift from a weak transcript. When a reminder does not hold, restart from a [checkpoint](compaction.md#checkpoint-template) in a clean context.
- Let a reminder yield to newer user direction and expire when its task ends: one repeated every turn can outweigh what the user just said (a single user report).

## Brief a subagent

Give each subagent a [task contract](protocol-catalog.md#catalog) plus the tools and sources to use. With one-line briefs Anthropic saw subagents misread the task and repeat each other's searches. A brief still drops details that change interpretation, so add the decisions already made and references to the evidence behind them ([delegation](delegation.md), [handoffs](handoffs.md)).

## Sources

- Anthropic: [effective context engineering](https://www.anthropic.com/research/effective-context-engineering-for-ai-agents), [prompting best practices](https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/claude-prompting-best-practices), [multi-agent research system](https://www.anthropic.com/research/multi-agent-research-system)
- OpenAI: [GPT-4.1 guide](https://developers.openai.com/cookbook/examples/gpt4-1_prompting_guide), [GPT-5 guide](https://developers.openai.com/cookbook/examples/gpt-5/gpt-5_prompting_guide), [Codex guide](https://developers.openai.com/cookbook/examples/gpt-5/codex_prompting_guide), [rethinking skills and prompts](https://developers.openai.com/blog/rethinking-skills-and-prompts-for-gpt-6-astra)
- Drift: [arXiv 2609.24532](https://arxiv.org/abs/2609.24532), [arXiv 2505.02709](https://arxiv.org/abs/2505.02709), [arXiv 2603.03258](https://arxiv.org/abs/2603.03258)
