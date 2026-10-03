# Formatting, UI text, and images

Load when choosing emphasis, capitalization, filenames, or markup, writing UI instructions, or adding an image.

## Format map

| Item | Format |
|---|---|
| UI labels, run-in headings, notice labels | bold |
| Term you define, word as word, emphasis, book or series title, math or version variable | italic |
| Code items, filenames, paths, output, placeholders, text the reader types | code font |
| Titles of short works (article, chapter, episode) | quotation marks |
| Link text | underline; underline only links |
| Product and service names, domains, URLs the reader opens | no formatting |

- Italics sparingly: a term defined at that point ("A *Clos network* is…"), a word as word, emphasis, long-work titles; never bold or quotes for those. Titles inside a link take link formatting.
- Don't override font styles inline. Never let case or formatting carry meaning the sentence must state (`Pod` versus `pod`). No ampersand as a conjunction, except in a UI label.

## Capitalization

- Sentence case for headings, titles, navigation, list items, table headings and cells, captions, and labels you author.
- References to your own titles use sentence case even if the original is title case; keep the original case for outside works. A hyphenated first word: capitalize only its first part ("Well-known limits").
- No ALL CAPS for emphasis; no camel case outside official names and code. Glossary and index terms lowercase; definitions sentence case.
- Show a case requirement with an example, not a name ("snake case"). Product and code-item case: `references/style-claims.md`, `references/style-code.md`.

## Filenames and markup

- Filenames: lowercase, hyphens, ASCII (`set-up-billing.md`); match the directory's existing pattern first. No generic names (`document1.html`).
- In prose: code font plus "file" ("the `main.tf` file"), exact spelling.
- Name file types by format: "a PNG file", "a Bash file", "a Terraform file", "an executable file", "a zip file", "a tar file", "a Wasm file". No file type as a verb ("extract a zip file", not `unzip it`).
- Markdown by default; match the repository. `**` for bold, `_` for italics.
- HTML only for what Markdown can't express: `<var>`, notices, `<kbd>`, table `scope`, `aria-label`, nonbreaking spaces, superscripts, `<code>` with special characters.
- Semantic elements: `em`/`strong` for meaning, `i`/`b` for visual-only, `cite` for standalone-work titles, `br` only for content breaks (a poem, an address), headings only for hierarchy, CSS for layout and spacing.
- Two-space indent, spaces not tabs, lowercase elements and attributes, 80-character lines where possible, no trailing whitespace except Markdown's two-space line break.

## UI elements

State the goal, not the widget ("Expand the **Advanced options** section"); name elements only when teaching the page itself.

| Element | Use |
|---|---|
| Web page, console subpage | page |
| Smaller window for one interaction | dialog, not `pop-up window` |
| Region inside a window | pane or panel; never window, section, area, column |
| Region inside a pane | section; never area or column |
| Item in a menu | command; "menu item" only for interface-building docs |
| Text entry | box ("the **Name** box"); Google Cloud and Workspace use field |
| Expandable region | expander arrow, expandable section; never "expando", "zippy" |
| Navigation menu | navigation menu; never `navigation bar`, `pane`, `panel`, `window` |
| Slang | never `hamburger icon` or `kebab menu` |

## UI labels

- Labels are bold: "Click **Save**". A label that is also code takes both: "In the **`Network`** list, select **`my-net-2`**". Typed text is code font: "In the **Name** field, enter `wsfc-1`."
- Keep the label's case; if it is all caps or a label set is inconsistent, use sentence case. Drop a trailing ellipsis (`Save ...` → **Save**). Bold a product or feature name only when it is the literal label. Never quote a label.
- Icons: icon, then the tooltip name ("click ⋮ **Settings and utilities**"). Never describe the shape; don't append "icon". No tooltip: check `aria-label`, `aria-labelledby`, `title`, `placeholder`, and file a bug for a tooltip.
- Menu chains: one bold span, nonbreaking space before each angle bracket ("Click **File > Open**"), separator labeled for screen readers (`aria-label="and then"`). Menu commands only.

## UI verbs

| Target | Verb |
|---|---|
| Button, link, icon | click (tap on touch devices) |
| Checkbox | select / clear; state "selected" or "not selected" |
| Radio button, menu command, list option | select or choose |
| Toggle, switch | turn on / turn off |
| Key | press |
| Box or field | enter, type |
| Page, tab, section | go to, open, expand |
| Pointer | drag; hold the pointer over (never `hover`) |
| Never | a label as a verb ("**Save** the file"), `toggle`, `deselect`, `hit` |

## Keys and location

- `<kbd>`, spelled-out modifiers, uppercase letters, `MODIFIER+Shift+KEY` (`Control+S`). macOS variant in parentheses: "press **Control+C** (or **Command+C** on macOS)". Spell out confusable characters (comma, hyphen, period, plus). A key typed as a literal value is code font. Say keyboard shortcut or key combination.
- "in" a dialog, field, menu, window, pane; "on" a page, tab, toolbar. Location first: "In the **Query** pane, click **Run**." Outside a procedure, give enough context to find the element.

## Images

- Use an image only when words explain badly. No new information only in an image; no image of text, code, or terminal output.
- Flows and decisions: Mermaid per `references/style-ste80.md`.
- Introduce an image like a list; skip that for a screenshot right after the text it shows.
- Every `img` has `alt`. Informative: what it conveys here, at most 155 characters, a sentence or noun phrase with punctuation; no "Image of…", no all caps.
- Decorative `alt=""`: ornament, UI icons, a screenshot that only repeats the text.
- More than 155 characters of information: put it in body text or a figure description. Same alt text for repeats. Introduce a diagram in the text, not alt text; a caption never replaces alt text.
- Caption: `**Figure 1.** Request flow through the proxy.`, a full sentence with end punctuation; wrap `img` and `figcaption` in `figure`. Don't fold the caption into the referring sentence.
- Refer to figures by number, never `the image above`; without numbers, repeat the figure where needed.
- Numbered callouts explained in prose, not dense in-graphic annotations; graphic text short and sentence case.
- Files: SVG for diagrams, PNG fallback, MP4 not animated GIF; within column width; no image maps or transparent backgrounds; descriptive filenames.
- High-DPI: 1x in `src`, 2x in `srcset` as `BASENAME_2x.EXTENSION` at exactly double size, never upscaled. Don't center images or nest `img` in `p`.
- Screenshots: crop to the relevant UI, consistent OS and treatment, opaque blocks (not blur) over personal data, flattened layers.

Source: [Text-formatting summary](https://developers.google.com/style/text-formatting).
