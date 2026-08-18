# Grammar Mechanics

Load for article, pronoun, possessive, plural, preposition, and contraction questions.

## Articles

- Keep `a`, `an`, and `the` — including in headings and titles: "Create a VM instance", not "Create VM instance". Dropping articles hurts comprehension and translation.
- Choose `a` or `an` by the following word's sound, not its letter.
- Articles before product, tool, and API names are owned by `references/style-claims.md`.

## Pronouns

- Every pronoun needs an unambiguous antecedent. IF "it", "this", or "they" could point at two nouns → THEN name the noun.
- Follow a demonstrative with a noun even when only one candidate exists: "Set **this value** to `true`", not "Set this to `true`".
- Keep optional relative pronouns — they aid clarity, not only ambiguity: "Right-click the link **that** you want to open"; "the fields, **which are** described in the following section".
- "that" introduces a restrictive clause (no comma); "which" introduces a nonrestrictive one (comma). For people you can use "who"; "whose" works for people, animals, and things.
- Singular "they" is the gender-neutral pronoun; never `he/she`, `s/he`, or a generic `he`.
- Second person for the reader; first person only in an FAQ question or where a document's author comments.

## Possessives

- Singular nouns, including those ending in s: add `'s` ("the class's quota"). Plural nouns ending in s: apostrophe only ("the models' capabilities"). Plurals not ending in s: `'s` ("the children's records").
- Company names take `'s` ("Google's office"). Product names, feature names, and trademarks never do — regardless of who owns them, so no "AWS's throughput".
- Code items: add a noun after the identifier and inflect that noun — "the `wordCount` method's return value" — or rewrite with "of".
- Don't form a possessive from an abbreviation paired with its expansion: "the rule that the Federal Trade Commission (FTC) issued".
- IF the possessive reads awkwardly → THEN rewrite the sentence.

## Plurals

- Standard US English plurals; never `'s` for a plural. Abbreviations add `s`, or `es` after s, sh, ch, x (`OSes`).
- Never park an optional plural in parentheses: no "your API key(s)", no "the child(ren)". Pick one form, or write "one or more" when both genuinely matter.
- "one or more" takes a plural verb ("if one or more tests fail"); "more than one" takes a singular ("you can create more than one instance").
- A spelled-out term and its abbreviation agree in number: "virtual machines (VMs)".
- Units agree with the number and abbreviations don't pluralize: "1 degree", "0.5 degrees", "64 GB" — never "64 GBs".
- Don't pluralize a code item or a trademark; add a plural noun instead ("`Widget` objects").
- Match the verb to the real subject: "The efficiency of algorithms that process data sets depends on memory allocation."

## Prepositions

- Ending a sentence with a preposition is fine when it reads better: "the language you're working with".
- Include prepositions that add clarity and cut the ones that don't.
- UI prepositions: "in" a dialog, field, menu, window; "on" a page, tab, toolbar (`references/style-ui.md`).

## Contractions

- Common two-word contractions are welcome; negative contractions (`isn't`, `can't`, `don't`) are preferred because the negative is harder to miss.
- No nonstandard or three-word contractions ("mightn't've"), and none that could read as a possessive.
- IF a negative needs emphasis → THEN spell it out with formatting (`is <em>not</em>`), but most sentences don't need it.

Next: tone and voice → `references/style-voice.md`; specific words → `references/style-words.md`.
