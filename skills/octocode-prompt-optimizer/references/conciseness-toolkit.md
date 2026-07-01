# Conciseness & Clarity Toolkit

Load during FIX when a line is wordy, indirect, or over about 20-25 words. Goal: fewer tokens, identical logic. Grounded in Anthropic agent guidance, plain-language rules, and psycholinguistics.

**Frame — density over length:** minimal is not the same as short. Cut tokens that carry no signal; keep every token that changes behavior. Aim for high signal-per-token, not raw brevity.

## Compression moves (shorten and keep logic)
| Move | Before → After | Why it helps an agent |
|------|----------------|-----------------------|
| De-nominalize | "make a decision" → "decide" | Verb carries the action; drops the empty light-verb |
| Active voice, name the actor | "it must be done" → "you must do it" | Shorter; assigns responsibility unambiguously |
| Cut expletive openers | "there is a check that runs" → "a check runs" | Removes placeholder plus relative pronoun |
| Strong verb over periphrasis | "provide validation of" → "validate" | One precise verb replaces a weak-verb chunk |
| Positive over negative | "do not omit the flag" → "include the flag" | Negation adds processing cost; flip double negatives |
| One instruction per sentence | split compound commands; cap about 20 words | Prevents dropped steps and costly re-parsing |
| Front-load | known/context first, new/emphasis last | Reader links back before absorbing the new claim |
| Parallel structure | match grammatical form across list items | Structural priming speeds reading and comparison |
| Prose → table / bullets / snippet | dense paragraph → structured rows | One real snippet beats three prose paragraphs |
| Strip non-meaningful markdown | decorative `**bold**`, a header over one line, a 2-row table → plain text | Markdown syntax is tokens; keep it ONLY when it aids structure or scanning |
| Offload | move conditional detail to `references/` | Keeps the always-loaded core lean (progressive disclosure) |
| Consistent vocabulary | one term per concept, throughout | Removes synonym-driven ambiguity; lowers decode load |

## Guardrails — do not over-compress
- **Keep signal:** never cut exact commands, versions, flags, or trigger phrases to save length.
- **Keep structure:** never drop subject/verb/article — ellipsis creates garden-path ambiguity that costs more than it saves.
- **Keep hard boundaries strict:** prefer positive framing, but keep strict prohibitions for destructive, fragile, or order-dependent rules (three tiers: always-allowed, ask-first, never).
- **Match style to output:** a terse prompt biases terse agent output; a verbose prompt biases verbose output.
- **Markdown only if meaningful:** emphasis, headers, tables, and fences cost tokens. Use them only when they aid parsing or scanning — a real matrix, a section, a code block; strip decorative formatting.
- **Table = genuine matrix:** this bounds the "Prose → table" move above — reach for a table on a real matrix, not on two items.
