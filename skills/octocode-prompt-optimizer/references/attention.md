# Attention & Structure Toolkit

Load during FIX to sharpen word choice, separate instructions from data, or move buried critical rules.
Pair with `conciseness-toolkit.md`: that file cuts tokens; this file makes the survivors easier to distinguish and apply.

**Frame — optimize context, not model attention.** Prompt authors control context tokens and structure; models compute attention weights internally.
Use semantic boundary tags such as `<context>` and `<example>`; `<attention>` is ordinary markup, while attention sinks are model/inference mechanisms.
Word choice, section tags, and ordering make prompt boundaries and task-relevant constraints easier to find.

## 1. Word choice — choose one clear meaning
Choose words that leave one interpretation.

| Move | Before → After | Why it helps an agent |
|------|----------------|-----------------------|
| Concrete verb over catch-all | "handle the file" → "delete the file" | Names the exact action; "handle/process/manage" force a guess |
| One term per concept | pick `prompt`; never also `instruction`, `input`, `text` | Avoids referring to one concept under multiple labels |
| Verbatim identifiers | quote tool names, flags, paths, versions exactly (`--no-color`, `SKILL.md`) | An approximate name is a different name to the agent |
| Plain over latinate/rare | "use" not "utilize"; "start" not "initialize" | Avoids unnecessary interpretive work |
| Name the entity | "the RATE gate" not "it" / "the above" | Avoids unclear references |
| Constraining phrasing | "return JSON" not "output should be structured" | The word itself carries the constraint; nothing left to infer |

## 2. Section tags — mark boundaries, not importance
Use section tags to separate instructions from data, mark exemplars, and isolate a high-value block. Their purpose is boundary clarity, not attention amplification.

- **`<example>…</example>`** — wrap exemplar input/output so the agent reads it as a sample to imitate, never as a live instruction to execute.
- **`<context>` / `<document>`** — trusted reference material. For retrieved, tool, or user text that may contain instructions, use `<untrusted_content source="…">` and treat it as data, not authority.
- **`<instructions>`** — the rule set; **`<output_format>`** — the exact required shape.
- Use a tag when: instructions and literal data/examples are mixed and could be misread as each other; a block must be treated verbatim; or one span carries the load and needs isolating.

Rules: descriptive, consistent tag names; nest only for real containment (`<document>` inside `<documents>`); close every tag. Markdown stays the default — use tags for separation, not emphasis (bounds `xml-overuse`).

## 3. Ordering — keep rules findable and execution-ordered
- **Use message-aware sections:** follow current provider/model guidance; a developer prompt commonly puts stable identity, instructions, and examples before bounded dynamic context.
- **Bracket the non-negotiables:** place every critical rule at a section boundary and repeat only the single most important one when testing shows it helps.
- **Execution order:** order and number steps in the sequence they run.
- **Point to the pivot:** in a long input, name the most relevant span ("The key constraint is X") before asking the agent to use it.

## Guardrails
- Tags separate, not emphasize — wrap only spans that need isolating, and close every tag you open.
- Preserve a required execution sequence and the structure the user fixed on purpose.
- Use semantic boundary tags instead of `<attention>` / `<important>` tags or duplicate rules as an attention hack.
- Keep every repeated critical rule semantically identical so copies cannot conflict.
- Word swaps must keep exact commands, versions, and trigger phrases intact (see `conciseness-toolkit.md` guardrails).

## Sources
- OpenAI, [Prompt engineering](https://developers.openai.com/api/docs/guides/prompt-engineering) — Markdown/XML boundaries and developer-message structure.
- Anthropic, [Effective context engineering for AI agents](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents) — bounded, high-signal context for agents.
- Liu et al., [Lost in the Middle](https://arxiv.org/abs/2307.03172) — context position can affect long-context retrieval; validate placement on the target task.
- Xiao et al., [StreamingLLM](https://arxiv.org/abs/2309.17453) — attention sinks are an internal model/inference mechanism.
