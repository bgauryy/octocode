Load when running a style pass, ruling on wording, structure, formatting, code, claims, or links, writing controlled English, drawing a diagram, or matching a reader. Each linked [Google style guide](https://developers.google.com/style) page holds the full rule.

# Style guide

## Review procedure

- Precedence: [SKILL.md](../SKILL.md#writing-rules).
- Check changed text for structure, claims, steps, terminology, links, and accessibility.
- Review comment (Conventional Comments) per material issue: label (issue, suggestion, question, nitpick, praise), blocking or not, location, rewrite, why; in a saved report, the label and blocking flag start the `Issue` cell. Group nitpicks: "`setup.md:14`: `The file is deleted by the job` → The job deletes the file."
- Fix depends on an uncertain fact: verify with `octocode-research` first.
- Disputed, possibly changed, or sensitive public-claim rule: check the live Google page; on conflict, follow the live page and report the discrepancy for this skill's maintainer.
- Claim only the pages you checked.

## Voice and tone ([tone](https://developers.google.com/style/tone), [voice](https://developers.google.com/style/voice))

- Second person, imperative, present tense, active voice; condition or goal first.
- Required = must or imperative; recommended = "we recommend"; optional = can. Not `should`, `may`.
- No filler: `please`, `just`, `simply`, `easy`, `of course`, exclamation marks.
- Software detects; it does not see, want, or know.
- One idea per sentence; at most five or six sentences per paragraph; no hard wraps.
- Translatable: one term per concept, keep articles and "that", no idioms, humor, or double negatives.

## Words ([word list](https://developers.google.com/style/word-list), [inclusive](https://developers.google.com/style/inclusive-documentation), [abbreviations](https://developers.google.com/style/abbreviations))

- Word in question: search the bundled [word list](../assets/google-word-list.tsv) (`grep -i '<word>' <skill-folder>/assets/google-word-list.tsv`; `localSearch` only when the skill folder is inside the allowed roots); use the matching entry in context.
- `allows you to` → lets you; `utilize`, `leverage` → use; `click on` → click; `abort`, `kill` → stop.
- Inclusive: `whitelist`/`blacklist` → allowlist/denylist; `master`/`slave` → primary/replica; `sanity check` → quick check; `guys` → everyone; generic `he` → they.
- Code-fixed terms stay in code font; prose uses the preferred word.
- Abbreviations: spell out on first use unless known (API, HTML). Never a verb: `ssh into` → use SSH to connect. `10x` → 10 times.
- Jargon: define recurring terms on first use; replace vague words (`solution`, `support`) with specifics.

## Structure ([headings](https://developers.google.com/style/headings), [lists](https://developers.google.com/style/lists), [procedures](https://developers.google.com/style/procedures), [notices](https://developers.google.com/style/notices), [tables](https://developers.google.com/style/tables))

- Headings: sentence case, no end period, one H1, no skipped levels; tasks are infinitives, concepts noun phrases.
- Lists: numbered for order, else bulleted; introduced by a sentence and colon; parallel, never one item; period only on sentences.
- Procedures: one imperative action per step, location first; prerequisites before step 1; mark "Optional:".
- Notices: `**Note:** ...` (Note, Caution, Warning, Success), sparingly; never a required step.
- Tables: three or more related values per row; sentence-case cells; no merged or layout tables.

## Punctuation, numbers, dates ([commas](https://developers.google.com/style/commas), [hyphens](https://developers.google.com/style/hyphens), [numbers](https://developers.google.com/style/numbers), [dates](https://developers.google.com/style/dates-times), [units](https://developers.google.com/style/units-of-measure))

- Serial comma; semicolon or period before "however".
- Em dash, no spaces; hyphenate compound modifiers (read-only file), not after `-ly`.
- Straight quotes, commas and periods inside; no slashes: `and/or` → or both.
- Zero to nine spelled out, numerals from 10; versions, sizes, ports, percentages always numerals.
- Dates: January 19, 2017, or ISO `2017-01-19` when machine-readable; never `01/19/17`. Times: 3 PM.
- Units: `64 GB` with a nonbreaking space; 64-bit system; `-40 °C to 85 °C`.

## Formatting and UI ([text formatting](https://developers.google.com/style/text-formatting), [UI elements](https://developers.google.com/style/ui-elements), [images](https://developers.google.com/style/images))

- Bold UI labels, italic defined terms (sparingly), code font for code, plain product names.
- UI verbs: click a button; select or clear a checkbox; turn on a toggle; press a key; enter text. Never `hit`, `toggle`, `deselect`, or hover (hold the pointer over).
- Name the goal, not the widget. Menus: **File > Open**. Keys as `<kbd>`: **Control+S** (**Command+S** on macOS).
- No direction or color (`the button above`); use preceding and following.
- Images only when words fail; alt text ≤155 characters, or `alt=""` if decorative; no images of text or code; crop screenshots and cover personal data.
- File names: lowercase, hyphens, ASCII (`set-up-billing.md`); in prose, the `main.tf` file.

## Code ([code in text](https://developers.google.com/style/code-in-text), [samples](https://developers.google.com/style/code-samples), [syntax](https://developers.google.com/style/code-syntax), [placeholders](https://developers.google.com/style/placeholders), [API reference](https://developers.google.com/style/api-reference-comments))

- Code font for identifiers, paths, commands, values, env vars, HTTP verbs, status codes; never inflected: `POST`ed → send a `POST` request.
- Samples: introduced; two-space indent; wrap at 80; omissions as a comment, not `...`.
- Syntax: `[OPTIONAL]`, `{a|b}`, `ARG...`; none in click-to-copy blocks.
- Placeholders: `PROJECT_ID`, never `MY_ID`, explained once.
- Output in its own block: "The output is similar to the following:".
- API summaries: third person, verb first, no period inside the first sentence. Booleans: "True if the list is sorted; false otherwise." Deprecations lead with the replacement: Deprecated. Use `listTasks` instead.

## Claims, examples, links ([timeless](https://developers.google.com/style/timeless-documentation), [examples](https://developers.google.com/style/examples), [link text](https://developers.google.com/style/link-text))

- Timeless: cut `currently`, `new`, `soon`, `latest`; name a release when a date matters.
- No unreleased features or superlatives (`best`, `fastest`); cite performance and cost claims or cut them. Security: helps prevent, not prevents.
- Products: official name and case, never a verb or plural; "the" before tool and API names. Link others' docs, never copy.
- Example values: `example.com`; 800-555-0100 to 0199; IPs `192.0.2.0/24`, `2001:db8::/32`; varied names (Dana, Kai); no real data, `foo`, or `bar`.
- Link text names the destination, never "click here"; say when a link downloads a file or leaves the site. Renaming a heading: update inbound anchors.

## Controlled English ([ASD-STE100](https://www.asd-ste100.org/))

Apply when the request asks for controlled technical English. This is guidance only; formal compliance needs the standard and its dictionary.

- Never invent a dictionary ruling; for an unclear word, use the source's shortest word.
- One word keeps one meaning and one part of speech.
- Unpack noun clusters: "the cache key for the request," not "request cache key hash value."
- Order as separate sentences, not -ing phrases: "Stop the server. Then delete the lock."

## Diagrams

- Draw order, branches, or component relations when easier to see than prose; split big ones into an overview and details; put conditions on edge labels.
- Draw only edges and orders the source states.
- One caption line; no prose repeat of edges. Text keeps thresholds, commands, IDs, owners, and reasons.
- Short linear chains stay inline: `build -> test -> publish`.

## Reader

Match the requested reader and task; use the [shared format chooser](report-output.md#choose-the-format) for Markdown, Mermaid and widgets. No audience classification is required.
