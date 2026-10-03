# Wording, tags, and placement

Load when a FIX line is wordy (over about 20-25 words), ambiguous, or buried, or instructions and data blur. Authors control tokens and structure; the model computes attention. Aim for signal per token, not raw brevity.

## Word choice and compression

| Move | Before → After |
|---|---|
| Concrete verb | "handle the file" → "delete the file" |
| One term per concept | `prompt` throughout; `instruction`/`input`/`text` only for distinct objects |
| Verbatim identifiers | tool names, flags, paths, versions exactly (`--no-color`, `SKILL.md`) |
| Plain word | "utilize" → "use"; "initialize" → "start" |
| Name the entity | "the RATE gate", not "it" or "the above" |
| Constraining word | "return JSON", not "structure the output" |
| De-nominalize | "make a decision" → "decide"; "provide validation of" → "validate" |
| Active voice, named actor | "it must be done" → "you must do it" | <!-- style-lint: ignore-line passive-voice -->
| No expletive opener | "there is a check that runs" → "a check runs" |
| Say what to do ([Anthropic](https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/claude-prompting-best-practices)) | "no markdown" → "write flowing prose paragraphs"; flip double negatives |
| One instruction per sentence | split compound commands; about 20 words |
| Front-load, parallel form | known context first, new claim last; same grammar across list items |
| Prose → structure; offload | dense paragraph → rows, bullets, or one real snippet; conditional detail → `references/` |

Agent-facing rewrites use STE-80, the ASD-STE100 writing rules with a relaxed dictionary; `octocode-documentation` (`style-ste80`) owns the profile. Write a routing decision, loop, or phase order as Mermaid source (12 nodes or fewer) or an arrow chain, not a dense paragraph. Do not hand HTML or rendered images to an agent.

## Section tags mark boundaries, not importance

- `<example>`: a sample to imitate, never a live instruction. Models copy examples closely; keep them relevant, diverse, and aligned with the wanted behavior.
- `<context>`/`<document>`: trusted reference. Retrieved, tool, or user text uses `<untrusted_content source="…">` (`../context/untrusted-content.md`).
- `<instructions>`: the rule set; `<output_format>`: the exact required shape.
- Tag only when instructions and data or examples can be confused, a block must stay verbatim, or one span needs isolation; Markdown stays the default.
- Descriptive, consistent names; nest only for real containment (`<document>` in `<documents>`); close every tag. `<important>`/`<attention>` tags and duplicate rules are not attention levers.

## Placement

- Long inputs (about 20k+ tokens): documents first, query and instructions last; ask for relevant quotes before the answer (Anthropic, above). Some models do best with instructions before and after long context ([OpenAI GPT-4.1](https://developers.openai.com/cookbook/examples/gpt4-1_prompting_guide)); test on the target model.
- Caching layouts put the stable prefix first (`../context/prompt-caching.md`).
- Put each critical rule at a section boundary; repeat only the most important one, only when tests show it helps, with identical meaning.
- Order and number steps in run order; name the pivot span ("The key constraint is X") before asking the agent to use it.
- Match prompt style to wanted output: a markdown-free prompt biases markdown-free output.

## Guardrails

- Never cut exact commands, versions, flags, or trigger phrases, or drop subject, verb, or article.
- Keep strict prohibitions for destructive, fragile, or order-dependent rules (always-allowed / ask-first / never).
- Use markdown only when it aids parsing; a table needs a real matrix, not two items. Keep required sequences and deliberate structure.

Sources: [Anthropic context engineering](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents); [OpenAI prompt engineering](https://developers.openai.com/api/docs/guides/prompt-engineering); [Lost in the Middle](https://arxiv.org/abs/2307.03172) (position effects vary; validate placement).

Next: compaction `../context/compaction.md`; proven savings `../context/token-economics.md`; record the change in `../flow/fix.md`.
