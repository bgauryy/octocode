# Formatting, UI text, and images

Load when choosing bold, italic, code font, or quotes, or when checking capitalization, filenames, or markup; when the text tells a reader to operate an interface; or when a page carries a screenshot, diagram, or any other image.

## Format map

| Item | Format |
|---|---|
| UI labels, run-in headings, notice labels | bold |
| Term you introduce and define, word as word, emphasis, book title, series title, math variable, version variable | italic |
| Code items, filenames, paths, output, placeholders, text the reader types | code font |
| Titles of short works (article, chapter, episode) | quotation marks |
| Link text | underline — reserve underlining for links |
| Product names, service names, domains, URLs the reader opens | no formatting |

- Use italics sparingly. Its jobs: the first mention of a term you define right there ("A *Clos network* is…"), a word discussed as a word ("use *and* instead of *&*"), emphasis, and titles of long works — never bold or quotes for those. Titles that are part of a link take link formatting instead.
- A UI element that also qualifies for code font gets both bold and code font (§ UI labels).
- Don't override font styles inline. Never let capitalization or formatting carry meaning that the sentence must state (`Pod` versus `pod`). Don't use an ampersand as a conjunction; the exception is a UI label or menu name that contains one.

## Capitalization

- Sentence case for headings, titles, navigation, list items, table headings, table cells, captions, and any label you author. Lowercase the first word after a colon (`references/style-punctuation.md`).
- References to another title or heading use sentence case even when the original is title case; keep the original casing for works outside your documentation. When a hyphenated word starts a sentence or heading, capitalize only the first element ("Well-known limits").
- No ALL CAPS for emphasis; no camel case outside official names and code. Glossary and index terms are lowercase; their definitions are sentence case.
- Product names take the owner's official capitalization, spelled in full (`references/style-claims.md`). Code items keep their own case even at the start of a sentence; rewrite if that looks wrong. Avoid naming case styles ("camel case", "snake case"); show the requirement with an example.

## Filenames and markup

- Filenames: lowercase, hyphen-separated, ASCII: `set-up-billing.md`. Consistency with the directory you're adding to wins over the general rule. No generic names like `document1.html`.
- In prose, put the filename in code font and add the word "file": "the `main.tf` file". Keep the exact spelling.
- Name file types by format, not extension: "a PNG file", "a Bash file" (`.sh`), "a Terraform file" (`.tf`), "an executable file" (`.exe`), "a zip file", "a tar file", "a Wasm file" (`.wasm`). Don't use a file type as a verb: "extract a zip file", not `unzip it`.
- Markup: Markdown by default; match whatever the repository already uses. Prefer `**` for bold and `_` for italics.
- Drop to HTML for what Markdown can't express: `<var>` placeholders, notices, `<kbd>`, table `scope`, `aria-label`, nonbreaking spaces, superscripts, and `<code>` when a code span needs special characters.
- Use elements semantically: `em`/`strong` for meaning, `i`/`b` for visual-only, `cite` for titles of standalone works, `br` only for breaks that are part of the content (a poem, an address), headings only for hierarchy, and CSS for both layout and spacing.
- Two-space indentation, spaces not tabs, lowercase elements, lowercase attributes, 80-character lines where the format allows, and no trailing whitespace — except the two trailing spaces Markdown uses for a line break.

## UI elements

State what the reader accomplishes, not which widget they poke: "Refresh the page", "Expand the **Advanced options** section". It survives redesigns. IF the point of the procedure is to walk the reader through the page itself → THEN name the elements.

| Element | Use |
|---|---|
| Web page, console subpage | page — the preferred general term |
| Smaller window for one interaction | dialog, not `pop-up window` |
| Distinct region inside a window | pane or panel — never window, section, area, or column |
| Region inside a pane | section — never area or column |
| Item in a menu | command; "menu item" only when documenting how to build an interface |
| Text entry | box, as "the **Name** box"; Google Cloud and Workspace docs use field |
| Expandable region | expander arrow, expandable section — never "expando" or "zippy" |
| Navigation menu | navigation menu — never `navigation bar`, `pane`, `panel`, or `window` |
| Slang | never `hamburger icon` or `kebab menu` |

## UI labels

- UI labels are bold: "Click **Save**". A label that also qualifies for code font takes both: "In the **`Network`** list, select **`my-net-2`**". Text the reader types is code font: "In the **Name** field, enter `wsfc-1`."
- Follow the label's own capitalization, but IF a label is all uppercase or a set of labels is inconsistently cased → THEN use sentence case: "Click **Refresh**". Drop a trailing ellipsis: document `Save ...` as "click **Save**". Don't bold a product or feature name unless it is literally the label on screen, and never quote a label.
- Icons: put the icon before the name from its tooltip — "click ⋮ **Settings and utilities**". Never describe an icon by shape, and don't append the word "icon" to a label. IF no tooltip exists → THEN check `aria-label`, `aria-labelledby`, `title`, or `placeholder`, and file a bug asking for a tooltip.
- Menu chains use a single bold span with a nonbreaking space before each angle bracket: "Click **File > Open**"; label the separator for screen readers (`aria-label="and then"`). The notation is for menu commands only — don't chain unrelated element types.

## UI verbs

| Target | Verb |
|---|---|
| Button, link, icon | click (tap on touch devices) |
| Checkbox | select / clear; state it as "selected" or "not selected" |
| Radio button, menu command, list option | select or choose |
| Toggle, switch | turn on / turn off |
| Key | press |
| Box or field | enter, type |
| Page, tab, section | go to, open, expand |
| Pointer | drag; hold the pointer over (never `hover`) |
| Never | a label as a verb ("click **Save**", not "**Save** the file"), `toggle`, `deselect`, `hit` |

## Keys and location

`<kbd>` markup, spelled-out modifiers, uppercase letters, `MODIFIER+Shift+KEY`: `Control+S`. Put the macOS variant in parentheses: "press **Control+C** (or **Command+C** on macOS)". Spell out confusable characters (comma, hyphen, period, plus). A key typed for its literal value is code font, not `<kbd>`. Call it a keyboard shortcut or key combination — and don't make a shortcut the documented way to complete a task (`references/style-structure.md`).
- "in" a dialog, field, menu, window, pane; "on" a page, tab, toolbar. Location first: "In the **Query** pane, click **Run**." Outside a numbered procedure, give the element enough context that the reader knows where it lives.
- No directional references. IF an element is genuinely hard to find → THEN provide a screenshot (§ Images).

## Images

- Use an image only when it explains something words handle badly. Never carry new information in an image — images aren't translated and aren't readable by everyone. Never use an image of text, a code sample, or terminal output. Use real text.
- A stated flow, order, or decision is a Mermaid diagram, not a dense paragraph. Draw only edges the source states; thresholds, commands, and IDs stay in text (`references/style-ste80.md`).
- Introduce most images with a complete sentence: colon when the image follows immediately, period when other material intervenes. IF the image is a screenshot right after the procedural text that describes that UI → THEN skip the introduction.
- Alt text: every `img` needs an `alt` attribute. Omitting it makes assistive technology read the filename aloud. Informative images: describe what the image conveys in this context, using at most 155 characters. Write a full sentence or a noun phrase, with punctuation so screen readers pause. No "Image of…" prefix, no all caps.
- Decorative images take `alt=""`: purely ornamental art, UI icons, and a screenshot that only repeats what the text already says (for example, a screenshot showing which fields to fill in).
- IF the image carries more than 155 characters of information → THEN put the detail in the body text or a figure description and keep alt text short. Use the same alt text for repeated instances of the same image. Introduce a diagram in the text, not in the alt text, and never let a caption substitute for alt text.
- Captions, numbers, descriptions: three distinct elements: alt text (short, for assistive technology), caption (optional label), figure description (longer explanation in the text).
- Caption format: `**Figure 1.** Request flow through the proxy.` — complete sentence, end punctuation. Wrap `img` and `figcaption` in `figure`. Don't lowercase-cap "figure" mid-sentence and don't fold the caption into the referring sentence.
- Refer to figures by number, never `the image above`. IF figure numbers aren't available → THEN repeat the figure where the reader needs it.
- Use numbered callouts explained in prose instead of dense annotations inside the graphic. If text must appear in a graphic, keep it short, in sentence case.
- Files: SVG for diagrams, PNG as fallback, MP4 instead of animated GIF; keep the image within the column width; no image maps, no transparent backgrounds; descriptive filenames.
- High-DPI: the 1x file goes in `src`, the 2x file in `srcset`, named `BASENAME_2x.EXTENSION` at exactly double the dimensions — never an upscaled 1x. Don't center images, and don't nest an `img` inside a `p`.
- Screenshots: crop to the relevant UI, stay consistent across operating systems and visual treatments, and cover personal data with solid opaque blocks — not blur. Flatten layered exports so the covered data is really gone.

Upstream: [Text-formatting summary](https://developers.google.com/style/text-formatting) · [Capitalization](https://developers.google.com/style/capitalization) · [Italics with terms](https://developers.google.com/style/italics-terms) · [Markdown versus HTML](https://developers.google.com/style/markdown) · [HTML and semantic tagging](https://developers.google.com/style/semantic-tagging) · [HTML formatting](https://developers.google.com/style/html-formatting) · [Filenames](https://developers.google.com/style/filenames) · [UI elements and interaction](https://developers.google.com/style/ui-elements) · [Figures and other images](https://developers.google.com/style/images). Verify a disputed or missing rule against the live page → `references/style-pass.md`.
