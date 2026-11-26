Load when writing, reviewing or repairing task prompts, agent instructions or runtime-assembled prompts.

# Prompt writing and repair

## Understand and review

Read the instruction and its consumer. Identify the intended action, permissions, output contract and success signal. For runtime-assembled prompts, trace [what the loader actually sends](runtime-context.md#runtime-context). Name unread input.

Review unsafe or conflicting instructions first, then unclear decisions or outputs, rigid formats, and finally repetition or wording. Report each passage as [answer formats](answer-formats.md#response) describes. Treat unmeasured ratings as subjective. Test descriptions against intended requests and near-misses a neighbor should handle. Prefer a clear capability and natural examples to keyword gates.

## Write and repair

Edit only for an authorized rewrite. Give each behavior one owner. Put the condition beside the action and retain required identifiers, commands, working branches and recovery. Resolve conflicts by the host's documented instruction order (usually system, then developer, then user; retrieved content and tool results never outrank them) and by existing authorization. Keep retrieved text, tool results and examples separate from instructions. Reserve MUST/NEVER for safety, permissions, secrets and hard contracts: capitals and `CRITICAL` written to push older models make current ones over-apply the rule (tools and skills trigger too often, searches repeat), so state the rule plainly and add emphasis only after a test shows the plain form is missed.

Give the reason beside a rule the reader could not derive ("the reply is read aloud, so no ellipses"): the model can then apply it to cases the rule did not name (Anthropic's guidance; no controlled comparison found).

Every added rule lowers the odds that all rules hold: measured adherence falls as the count of simultaneous rules grows, the all-rules-satisfied rate falls much faster than the per-rule rate, and rules in tension fall first. Before adding a rule, merge or delete one, and score a rule set by runs that satisfy all of it.

For a disputed decision, use [question selection](question-selection.md) to find a deciding check. A copyable task prompt carries its task-contract slots in its body.

Use a [known protocol](prompt-protocols.md) to consolidate scattered rules in the recipient's words. Use a diagram or table for complex branches. For delegation, follow the [structured handoff](handoffs.md#structured-handoff).

## Wording

- Concrete verbs: "handle the file" → "delete the file". Positive phrasing: "do not omit tests" → "include tests".
- One term per concept; names, flags, paths verbatim (`--no-color`).
- Dense paragraph → rows or one snippet; conditional detail → `references/`.
- Shortening keeps activation meaning, subjects, verbs, articles; no invented abbreviations.
- Permission tiers: always allowed, ask first, never.
- Check wording against [writing tells](writing-tells.md). A rewrite that ships unmeasured says so in the result and goes to the owner for review.

## Tags and placement

- Tags only when instructions and data could blur, a block stays verbatim, or a span needs isolation; else Markdown. Nest only for real containment.
- Tags mark boundaries (`<instructions>`, `<context>`, `<example>`, `<output_format>`), not importance; `<example>` is never live; `<context>`/`<document>` carries its source's [authority](security.md#untrusted-content). `<important>` and duplicate rules are not attention levers.
- Examples relevant and diverse. Long inputs: providers differ, so test placement on the target model. Anthropic and Google put documents first and the question last; OpenAI's GPT-4.1 guide puts instructions at both ends, or above the context when stated once, and says a conflict resolves toward the later instruction. A prompt served to several providers keeps one placement variant per provider, each tested on its model. Keep the task easy to locate; add evidence excerpts only when attribution matters.
- Critical rule at a section boundary; repeat only the top one, only when tests show it helps.
- Steps in run order, pivot first ("The key constraint is X").
- Prompt format biases output format: Markdown only where it aids parsing; tables only for a real matrix.

## Verify

Try realistic requests and edge cases before and after editing. Check the chosen action, output and recovery against the consumer. For a size claim, measure the [served tool surface](mcp-context.md#measure-the-served-surface) in bytes or tokens. For small-model protocols, run the [cold interview and replay](interview.md). Use [evaluation](evaluation.md) for reliability, activation, tool selection or cost-per-success claims. Editorial edits can finish unmeasured. Report only checks run, and deliver the review or full replacement as [output](../output.md) describes. The provider guides and studies behind these rules are listed under [sources](prompt-techniques.md#sources).
