# Wording, tags, placement, and techniques

Load when an instruction is wordy, ambiguous, or buried; when instructions and data blur; or when a failure needs a technique.

## Word choice and compression

| Move | Before → After |
|---|---|
| Concrete verb | "handle the file" → "delete the file" |
| One term per concept | `prompt` throughout; other words only for distinct objects |
| Verbatim identifiers | tool names, flags, paths, versions exactly (`--no-color`) |
| Constraining word | "return JSON", not "structure the output" |
| Flip double negatives | "do not omit tests" → "include tests" |
| Prose → structure; offload | dense paragraph → rows or one real snippet; conditional detail → `references/` |

Use text for instructions; use images or HTML only when the reader and task benefit from them.

## Section tags mark boundaries, not importance

- `<example>`: a sample to imitate, never a live instruction; keep examples relevant, diverse, and aligned.
- `<context>`/`<document>`: reference material. Authority comes from the source and host hierarchy, never the tag. Label retrieved instructions as untrusted data (`../context-management/untrusted-content.md`).
- `<instructions>`: the rule set; `<output_format>`: the exact shape.
- Tag only when instructions and data can be confused, a block must stay verbatim, or a span needs isolation; Markdown is the default.
- Nest only for real containment. `<important>` tags and duplicate rules are not attention levers.

## Placement

- For long inputs, keep the task easy to locate and test placement on the target model. Evidence excerpts can help ground an answer when attribution matters; do not require them for every task.
- Each critical rule at a section boundary; repeat only the most important one, only when tests show it helps.
- Steps in run order; name the pivot span ("The key constraint is X") before asking the agent to use it.
- Match prompt style to wanted output: a markdown-free prompt biases markdown-free output.
- Preserve activation meaning, subjects, verbs, and articles when shortening. Tier permissions as always-allowed / ask-first / never.
- Markdown only when it aids parsing; a table needs a real matrix.

## Technique selector

Start direct. Define the success signal and eval; confirm the prompt (not model, tool, data, or policy) is the lever. Add one technique at a time; the regression budget covers tokens, latency, and safety.

| Need | Use | Keep it agent-smart |
|---|---|---|
| Clear task, known format | Direct contract | Goal, inputs, constraints, stop rule, output shape |
| Format or edge behavior unclear | Few-shot examples | Diverse boundary cases; remove once a schema or rule suffices |
| Instructions and data blur | Sections/XML | Label authority, context, examples, output |
| Another system consumes the result | Structured output/schema | Constrain fields and enums; validate; actionable errors |
| Answer needs external facts | Retrieval | Smallest relevant evidence, cited, gaps marked |
| Task needs action or observation | Tool loop | When to call each tool, compact results, observable stop |
| Agent over- or under-explores | Eagerness contract | Stop criteria, tool-call budget, ask vs. assume, safe vs. unsafe actions |
| Dependent stages | Prompt chain | Typed artifact and verifier per stage; no raw transcripts |
| Ambiguity changes the decision | Bounded candidates | Candidates checked by evidence or a verifier, not majority vote |
| Hard planning | Plan + checkpoints | Inspectable plan, then execute and verify; never require private reasoning text |
| Stable prefix repeats | Prompt caching | `../context-management/token-economics.md` |
| Unsure what edit fixes a failure | Metaprompting | Ask which minimal addition or deletion produces the behavior; verify with the eval |

- Prefer a tool, schema, retrieval filter, or deterministic checker over prose asking the model to simulate one.
- No personas, chain-of-thought requests, debate, or branching because they sound advanced.

Source: [Anthropic prompting best practices](https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/claude-prompting-best-practices).

Next: record the change in [repair guidance](../../SKILL.md#write-and-repair).
