Load when a model writes, repairs, migrates or optimizes a prompt, or when a request says "meta prompting".

# Meta prompting

The term names three techniques. Settle which one is meant from the request's wording before acting; ask the user only when two meanings would lead to different work:

| Meaning | What happens | Go to |
|---|---|---|
| A model writes or revises a prompt (OpenAI's use) | The prompt is the output | This page |
| A conductor over experts (Suzgun and Kalai) | One model splits the task, queries fresh instances and integrates their answers | [Delegation](delegation.md) |
| A structure template (Zhang et al.) | An example-free prompt fixes the reasoning and output schema for a family of tasks | [Protocols](prompt-protocols.md) |

## Repair a failing prompt

1. Confirm the prompt is the lever ([techniques](prompt-techniques.md#techniques)).
2. Give the model the prompt, the desired behavior and the observed behavior: real failing cases, each rated good or bad with a specific critique.
3. Ask for the cause and the minimal additions or deletions that keep the rest intact. Take a full rewrite only for a simple prompt or a migration: it changes rules nobody tested.
4. Check the edit before running it: every rule survives ([rules vs presentation](prompt-protocols.md#rules-vs-presentation)) and no case-specific answer or identifier entered the instructions ([roles and access](eval-isolation.md#roles-and-access)).
5. Keep it only if sealed-test cases improve ([eval loop](eval-loop.md)); a revision that reads better proves nothing.

## Use cases and limits

| Use | Do | Measured limit |
|---|---|---|
| Draft from a task description | Treat the result as a first draft; add the project's identifiers, permissions and checks yourself | A generated prompt knows only what the description said |
| Migrate to another model | Reread the target model's guide ([sources](prompt-techniques.md#sources)), apply this repair loop to the failures and rerun the evals | A prompt tuned for one model often does worse on another |
| Generate test cases | Grow them from a baseline of real cases and review each; test a model grader on known cases before scaling it | The model that wrote the prompt shares its blind spots (inferred, not measured) |
| Optimize automatically (GEPA, MIPRO, OPRO, TextGrad) | Needs a metric and separate development, validation and sealed-test sets. Read what the optimizer found and write that rule plainly | OPRO's training accuracy ran 5–20% above test; gains often fail to transfer across benchmarks and models; one study reproduced most of the gain with a hand-written version of the discovered rule |
| An agent updates its own instructions or memory | Propose the edit for review, scope it to the task that taught it, validate before writing ([memory](memory.md)) | Self-edits degraded earlier capabilities in two studies; generated repository instruction files raised cost without raising success ([context budget](context-budget.md#context-budget)) |

Provider optimizers come and go (OpenAI scheduled its dataset-backed optimizer to shut down on 2026-11-30): keep the cases and the metric in your own files.

## Sources

- [OpenAI: GPT-5 prompting guide](https://developers.openai.com/cookbook/examples/gpt-5/gpt-5_prompting_guide) (minimal-edit template), [prompt generation](https://developers.openai.com/api/docs/guides/prompt-generation.md), [prompt optimizer](https://developers.openai.com/api/docs/guides/prompt-optimizer.md)
- [Anthropic: prompt improver](https://claude.com/resources/articles/prompt-improver)
- Meanings: [Suzgun and Kalai](https://arxiv.org/abs/2401.12954), [Zhang et al.](https://arxiv.org/abs/2311.11482)
- Optimizers: [GEPA](https://arxiv.org/abs/2507.19457), [OPRO](https://arxiv.org/abs/2309.03409), [MIPRO](https://arxiv.org/abs/2406.11695); transfer failures: [Gong and Wen](https://arxiv.org/abs/2605.26655), [PromptBridge](https://arxiv.org/abs/2512.01420), [Lumer et al.](https://arxiv.org/abs/2610.12601)
- Self-editing: [Yu et al.](https://arxiv.org/abs/2605.09315), [Chen et al.](https://arxiv.org/abs/2610.25858), [Gloaguen et al.](https://arxiv.org/html/2602.11988v3)
