# Text formatting and capitalization

Load when choosing bold, italic, code font, or quotes, or when checking capitalization, filenames, or markup.

## Format map

| Item | Format |
|---|---|
| UI labels, run-in headings, notice labels | bold |
| Term you introduce and define, word as word, emphasis, book title, series title, math variable, version variable | italic |
| Code items, filenames, paths, output, placeholders, text the reader types | code font |
| Titles of short works (article, chapter, episode) | quotation marks |
| Link text | underline — reserve underlining for links |
| Product names, service names, domains, URLs the reader opens | no formatting |

- Use italics sparingly. Its jobs: the first mention of a term you define right there ("A *Clos network* is…"), a word discussed as a word ("use *and* instead of *&*"), emphasis, and titles of long works — never bold or quotes for those.
- Titles that are part of a link take link formatting instead.
- A UI element that also qualifies for code font gets both bold and code font (`references/style-ui.md`).
- Don't override font styles inline. Never let capitalization or formatting carry meaning that the sentence must state (`Pod` versus `pod`).
- Don't use an ampersand as a conjunction; the exception is a UI label or menu name that contains one.

## Capitalization

- Sentence case for headings, titles, navigation, list items, table headings, table cells, captions, and any label you author.
- Lowercase the first word after a colon unless it's a proper noun, a heading, a quotation, or a notice label.
- References to another title or heading use sentence case even when the original is title case; keep the original casing for works outside your documentation.
- When a hyphenated word starts a sentence or heading, capitalize only the first element ("Well-known limits").
- No ALL CAPS for emphasis; no camel case outside official names and code. Glossary and index terms are lowercase; their definitions are sentence case.
- Product names take the owner's official capitalization, spelled in full (`references/style-claims.md`).
- Code items keep their own case even at the start of a sentence; rewrite if that looks wrong.
- Avoid naming case styles ("camel case", "snake case"); show the requirement with an example.

## Filenames and file types

- Lowercase, hyphen-separated, ASCII: `set-up-billing.md`. Consistency with the directory you're adding to wins over the general rule. No generic names like `document1.html`.
- In prose, put the filename in code font and add the word "file": "the `main.tf` file". Keep the exact spelling.
- Name file types by format, not extension: "a PNG file", "a Bash file" (`.sh`), "a Terraform file" (`.tf`), "an executable file" (`.exe`), "a zip file", "a tar file", "a Wasm file" (`.wasm`).
- Don't use a file type as a verb: "extract a zip file", not `unzip it`.

## Markup

- Markdown by default; match whatever the repository already uses. Prefer `**` for bold and `_` for italics.
- Drop to HTML for what Markdown can't express: `<var>` placeholders, notices, `<kbd>`, table `scope`, `aria-label`, nonbreaking spaces, superscripts, and `<code>` when a code span needs special characters.
- Use elements semantically: `em`/`strong` for meaning, `i`/`b` for visual-only, `cite` for titles of standalone works, `br` only for breaks that are part of the content (a poem, an address), headings only for hierarchy, and CSS for both layout and spacing.
- Two-space indentation, spaces not tabs, lowercase elements, lowercase attributes, 80-character lines where the format allows, and no trailing whitespace — except the two trailing spaces Markdown uses for a line break.

Upstream: [Text-formatting summary](https://developers.google.com/style/text-formatting) · [Capitalization](https://developers.google.com/style/capitalization) · [Italics with terms](https://developers.google.com/style/italics-terms) · [Markdown versus HTML](https://developers.google.com/style/markdown) · [HTML and semantic tagging](https://developers.google.com/style/semantic-tagging) · [HTML formatting](https://developers.google.com/style/html-formatting) · [Filenames](https://developers.google.com/style/filenames). Verify a disputed or missing rule against the live page → `references/style-sources.md`.

Next: punctuation → `references/style-punctuation.md`; numbers, dates, units → `references/style-numbers.md`.
