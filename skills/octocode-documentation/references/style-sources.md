# Google Style Sources

Load when a rule is challenged, or to check whether a rule has moved. All URLs are under `https://developers.google.com/style/<page>`; guide content is published under CC BY 4.0 and restated here, not copied.

Every page of the guide maps to one owning reference:

| Guide pages | Owned by |
|---|---|
| `highlights`, `philosophy` | `references/style-index.md` |
| `tone`, `person`, `voice`, `tense`, `anthropomorphism`, `prescriptive-documentation`, `sentence-structure`, `paragraph-structure` | `references/style-voice.md` |
| `articles`, `pronouns`, `possessives`, `pluralization`, `prepositions`, `contractions` | `references/style-grammar.md` |
| `word-list` (also served at `spelling`) | `references/style-words.md` |
| `abbreviations`, `jargon` | `references/style-abbreviations.md` |
| `translation`, `accessibility` | `references/style-global.md` |
| `inclusive-documentation` | `references/style-inclusive.md` |
| `headings`, `lists` | `references/style-structure.md` |
| `procedures` | `references/style-procedures.md` |
| `notices`, `tables`, `footnotes` | `references/style-blocks.md` |
| `images` | `references/style-images.md` |
| `text-formatting`, `capitalization`, `italics-terms`, `markdown`, `semantic-tagging`, `html-formatting`, `filenames` | `references/style-format.md` |
| `colons`, `commas`, `dashes`, `ellipses`, `hyphens`, `parentheses`, `periods`, `quotation-marks`, `semicolons`, `slashes`, `format-examples` | `references/style-punctuation.md` |
| `numbers`, `dates-times`, `units-of-measure`, `phone-numbers`, `mathematical-notation` | `references/style-numbers.md` |
| `code-in-text`, `code-samples` | `references/style-code.md` |
| `code-syntax`, `placeholders` | `references/style-cli.md` |
| `examples` | `references/style-examples.md` |
| `ui-elements` | `references/style-ui.md` |
| `cross-references`, `headings-targets` | `references/style-links.md` |
| `timeless-documentation`, `future`, `excessive-claims`, `other-sources`, `trademarks`, `product-names` | `references/style-claims.md` |
| `api-reference-comments`, `reference-verbs` | `references/style-api.md` |
| `whats-new` | staleness check — see below |

## Checking for drift

`whats-new` is the guide's own changelog. IF a rule here looks stale, or a reviewer cites something these references don't carry → THEN read `https://developers.google.com/style/whats-new` before arguing; the guide ships changes several times a year and has moved rules these references depend on (temperature spacing, checkbox state wording, heading-anchor markup).

Word-list entries keep the guide's own guidance text in `assets/google-word-list.tsv` (term, verdict, guidance); `scripts/refresh-word-list.mjs --dry-run` reports drift, and `scripts/style-lint.mjs` reads the file so every flagged word cites the guide's wording.

Next: back to the topic map → `references/style-index.md`.
