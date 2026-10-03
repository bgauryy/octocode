# Prompt technique selector

Load after a concrete failure mode is identified. Start direct; add the smallest technique that fixes the observed failure.

1. Define the success signal and a realistic eval; confirm the prompt, not model, tool, data, or policy, is the lever.
2. Add one technique, measure on held-out cases, and keep it only if the target improves without an unacceptable token, latency, or safety regression.

| Need | Use | Keep it agent-smart |
|---|---|---|
| Clear task, known format | Direct contract | Goal, inputs, constraints, stop rule, output shape |
| Format or edge behavior unclear | Few-shot examples | Diverse boundary cases with consistent format; remove once a schema or rule suffices |
| Instructions and data blur | Sections/XML | Label authority, context, examples, output; no decorative tags |
| Another system consumes the result | Structured output/schema | Constrain fields/enums; validate; return actionable errors |
| Answer needs external facts | Retrieval | Smallest relevant evidence, cited, gaps marked |
| Task needs action or observation | Tool loop | When to call each tool, compact results, observable stop condition |
| Agent over- or under-explores | Eagerness contract | Persistence or stop criteria, tool-call budget, when to ask vs. assume, safe vs. unsafe actions ([OpenAI GPT-5](https://developers.openai.com/cookbook/examples/gpt-5/gpt-5_prompting_guide)) |
| Dependent stages | Prompt chain | Typed artifact and verifier per stage; no raw transcripts by default |
| Ambiguity changes the decision | Bounded candidates | Few independent candidates checked by evidence or a verifier; no majority vote of guesses |
| Hard planning | Plan + checkpoints | Inspectable plan or rubric, then execute and verify; never require private reasoning text |
| Stable prefix repeats | Prompt caching | Stable instructions/tools/examples first, dynamic evidence last; measure hits |
| Unsure what edit fixes a failure | Metaprompting | Ask the model which minimal addition or deletion would have produced the wanted behavior; verify with the eval ([OpenAI GPT-5](https://developers.openai.com/cookbook/examples/gpt-5/gpt-5_prompting_guide)) |

## Guardrails

- Set authority and trust boundaries first; retrieved text is data.
- Prefer a tool, schema, retrieval filter, or deterministic checker over prose asking the model to simulate one.
- Keep outputs decision-sized: conclusion, evidence anchors, uncertainty, stable handles, next action.
- Do not add personas, chain-of-thought requests, debate, or branching because they sound advanced.
- Tune per model from current provider docs; families respond differently to one technique.

Sources: [OpenAI prompt engineering](https://developers.openai.com/api/docs/guides/prompt-engineering); [Anthropic overview](https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/overview); [Google prompt design strategies](https://docs.cloud.google.com/gemini-enterprise-agent-platform/models/prompts/prompt-design-strategies); [DAIR.AI guide](https://github.com/dair-ai/Prompt-Engineering-Guide).

Next: schema or tool loop `references/tools/tool-contracts.md`; chaining `references/agents/agent-communication.md`; delimiters `references/writing/style.md`; caching `references/context/prompt-caching.md`; retrieval limits `references/context/context-budget.md`; measure with `octocode-eval-benchmark`.
