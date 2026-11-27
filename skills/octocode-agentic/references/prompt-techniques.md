Load when a prompt fails and you are choosing which technique to add.

# Prompt techniques

## Techniques

- On failure, start direct: define the success signal and eval, confirm the prompt (not model, tool, data, policy) is the lever, then add one technique per failure within the task's token, latency and safety limits (its [KPI contract](eval-kpi.md#kpi-contract), or the owner's stated budget):
  - Known format → direct contract: [task contract](handoffs.md#handoff-protocols).
  - Unclear edges → few-shot boundary cases; drop once a rule suffices.
  - Machine consumer → structured output: enums, validation, actionable errors.
  - External facts or actions → retrieval ([budget](context-budget.md#context-budget)) or a tool loop with a [stop condition](execution-control.md#stop-conditions).
  - Over- or under-exploring → Stop condition (Definition of Done as observable checks; escalate at the boundary) + Budget contract (call budget, behavior when spent); ask vs assume, safe vs unsafe.
  - Dependent stages → prompt chain: typed artifact + verifier per stage, no raw transcripts.
  - Decision-changing ambiguity → bounded candidates checked by evidence, not majority vote.
  - Hard planning → inspectable plan + checkpoints; no private reasoning ([reasoning techniques](reasoning.md)). Unsupported claims → [grounding](grounding.md#reduce-unsupported-claims).
  - Reasoning model, or thinking enabled → [what to state](guiding-agents.md#what-to-state); chain of thought only per [reasoning](reasoning.md).
  - Stable prefix → [caching](caching.md#prompt-caching). Unknown fix → [meta prompting](meta-prompting.md): minimal addition or deletion, verified by the eval.
- Use the real tool, schema, filter, or checker, never prose asking the model to simulate one.
- For multi-angle decisions, use [question selection](question-selection.md) to turn distinct perspectives into testable checks. Drop personas, debate or branches that add no decision-changing observation.

## Sources

- Provider guidance changes per model generation: reread the target model's guide before applying a rule from these pages.
- [Anthropic: prompting best practices](https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/claude-prompting-best-practices): emphasis, reasons, document placement, general instructions over prescriptive steps
- OpenAI: [GPT-4.1 prompting guide](https://developers.openai.com/cookbook/examples/gpt4-1_prompting_guide) (emphasis, placement), [reasoning best practices](https://developers.openai.com/api/docs/guides/reasoning-best-practices); [Google: prompt design strategies](https://aistudio.google.com/docs/prompting-strategies.md)
- Rule count: [IFScale](https://arxiv.org/abs/2507.11538) (best model 68% at 500 instructions; two reasoning models near-perfect to about 150), [arXiv 2608.12426](https://arxiv.org/abs/2608.12426) (all-constraints rate), [arXiv 2510.14842](https://arxiv.org/abs/2510.14842) (tension between rules)
