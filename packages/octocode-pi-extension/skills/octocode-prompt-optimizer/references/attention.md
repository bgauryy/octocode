# Attention & Structure Toolkit

Load during FIX to sharpen word choice, isolate a high-value span, or move buried critical rules. Companion to `conciseness-toolkit.md`: that one cuts tokens; this one places the survivors where attention lands. Grounded in Anthropic prompt guidance (XML tags, example separation) and long-context attention research (U-shaped primacy/recency bias).

**Frame — attention is U-shaped.** An agent reads with primacy + recency bias. The start and end of a prompt, and of each section, get the most attention; the middle is where instructions get dropped. Word choice, section tags, and ordering are the three levers over where attention lands.

## 1. Word choice — pick the term the agent resolves fastest
Most model errors trace to semantic misreading, not missing information. Choose words that leave one interpretation.

| Move | Before → After | Why it helps an agent |
|------|----------------|-----------------------|
| Concrete verb over catch-all | "handle the file" → "delete the file" | Names the exact action; "handle/process/manage" force a guess |
| One term per concept | pick `prompt`; never also `instruction`, `input`, `text` | Synonyms read as distinct entities; consistency lowers decode load |
| Verbatim identifiers | quote tool names, flags, paths, versions exactly (`--no-color`, `SKILL.md`) | An approximate name is a different name to the agent |
| Plain over latinate/rare | "use" not "utilize"; "start" not "initialize" | High-frequency words parse faster and less ambiguously |
| Name the entity | "the RATE gate" not "it" / "the above" | Kills referential ambiguity (see RATE `referential-ambiguity`) |
| Constraining phrasing | "return JSON" not "output should be structured" | The word itself carries the constraint; nothing left to infer |

## 2. Section tags — isolate the spans attention must land on
Claude is trained to recognize XML-style tags as structure boundaries. Use them to separate instructions from data, mark exemplars, and give a block focused attention — **not** as decoration.

- **`<example>…</example>`** — wrap exemplar input/output so the agent reads it as a sample to imitate, never as a live instruction to execute.
- **`<context>` / `<document>`** — reference material the agent should use but not follow as commands.
- **`<instructions>`** — the rule set; **`<output_format>`** — the exact required shape.
- Use a tag when: instructions and literal data/examples are mixed and could be misread as each other; a block must be treated verbatim; or one span carries the load and needs isolating.

Rules: descriptive, consistent tag names; nest only for real containment (`<document>` inside `<documents>`); close every tag. Markdown stays the default — reach for tags for separation or attention, not for emphasis (bounds `xml-overuse`).

## 3. Ordering — put critical rules where attention peaks
- **Front-load the frame:** role/context → task → instructions → output format. Context before the task so the agent has the frame before the ask.
- **Bracket the non-negotiables:** the middle is the low-attention trough, so place critical rules at the top of a section and restate the single most important one at the end. Never bury a MUST mid-block.
- **Given before new:** within a sentence, put known/context first and the new/emphasized claim last (recency) so the agent links back before absorbing it.
- **Execution order:** order and number steps in the sequence they run.
- **Point to the pivot:** in a long input, one line naming the most relevant span ("The key constraint is X") measurably lifts recall of mid-context detail.

## Guardrails
- Tags separate, not emphasize — wrap only spans that need isolating, and close every tag you open.
- Do not reorder when it breaks a required execution sequence or a structure the user fixed on purpose.
- Bracketing restates a rule for attention; it must not re-specify it differently, or the copies become a conflict.
- Word swaps must keep exact commands, versions, and trigger phrases intact (see `conciseness-toolkit.md` guardrails).
