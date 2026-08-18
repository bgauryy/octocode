# Google Style Index

Load for any wording, formatting, or terminology question. This maps every topic in the Google developer documentation style guide (`https://developers.google.com/style`) to the reference that owns its rules, so the guide works as a lookup instead of a read-through.

New to the guide: skim `style-voice.md` (voice and tone) and `style-format.md` (text-formatting summary) once. After that, look things up — most questions are word-list questions (`style-words.md`).

| Ask | Load |
|---|---|
| Tone, "you" versus "we", active voice, present tense, must/can/recommend, sentence and paragraph shape | `references/style-voice.md` |
| Articles, pronouns, that/which, possessives, plurals, prepositions, contractions | `references/style-grammar.md` |
| One specific word; the full 597-entry word list | `references/style-words.md` |
| Abbreviations, acronyms, jargon | `references/style-abbreviations.md` |
| Translation-safe phrasing, accessible language | `references/style-global.md` |
| Inclusive terminology, disability and people | `references/style-inclusive.md` |
| Headings, titles, lists, description lists, run-in headings | `references/style-structure.md` |
| Numbered steps, task instructions | `references/style-procedures.md` |
| Notices, tables, footnotes | `references/style-blocks.md` |
| Screenshots, diagrams, alt text, captions | `references/style-images.md` |
| Bold/italic/code/quote choice, capitalization, filenames, Markdown versus HTML | `references/style-format.md` |
| Commas, colons, dashes, hyphens, quotation marks, slashes, ellipses, parentheses | `references/style-punctuation.md` |
| Numbers, dates, times, units, phone numbers, math notation | `references/style-numbers.md` |
| What takes code font, HTTP status codes, code samples | `references/style-code.md` |
| Command syntax, prompts, placeholders, click-to-copy, output blocks | `references/style-cli.md` |
| Example domains, names, addresses, IDs | `references/style-examples.md` |
| UI labels, element terminology, interaction verbs, keys | `references/style-ui.md` |
| Link text, cross-references, heading anchors | `references/style-links.md` |
| "currently/new/soon", roadmap talk, superlatives, product names, trademarks, third-party text | `references/style-claims.md` |
| Docstrings, method summaries, parameters, returns, exceptions | `references/style-api.md` |
| Producing a style review someone else acts on | `references/style-review.md` |
| Which guide page backs a rule, or whether a rule is stale | `references/style-sources.md` |

## Defaults to apply without looking anything up

Sentence case headings; second person; active voice; present tense; imperative steps; condition before instruction; serial comma; descriptive link text; code font for code and bold for UI labels; alt text on every image; no "currently/soon", no superlatives, no pre-announcements.

## Precedence

1. The project's own documented style guide, when it exists.
2. A convention this repository already applies consistently — report the conflict, don't add a second scheme.
3. This guide.

Deviations are fine when they are deliberate and consistent; say which rule you overrode and why.

Next: run `scripts/style-lint.mjs <paths>` for the mechanical hits before hand-reading; interpret each finding with the reference named in its message.
