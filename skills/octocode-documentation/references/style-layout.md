Load when ruling on headings, lists, procedures, notices, tables, punctuation, numbers, dates, units, text formatting, UI terms or images, or drawing a diagram. The [style guide](style-guide.md#review-procedure) holds the review procedure; the Google style guide pages linked from each heading hold the full rules.

# Style: structure and formatting

## Structure ([headings](https://developers.google.com/style/headings), [lists](https://developers.google.com/style/lists), [procedures](https://developers.google.com/style/procedures), [notices](https://developers.google.com/style/notices), [tables](https://developers.google.com/style/tables))

- Headings: sentence case, no end period, one H1, no skipped levels; tasks are infinitives, concepts noun phrases.
- Lists: numbered for order, else bulleted; introduced by a sentence and colon; parallel, never one item; period only on sentences.
- Procedures: one imperative action per step, location first; prerequisites before step 1; mark "Optional:".
- Notices: `**Note:** ...` (Note, Caution, Warning, Success), sparingly; never a required step.
- Tables: compare related values; sentence-case cells; no merged or layout tables.

## Punctuation, numbers, dates ([commas](https://developers.google.com/style/commas), [hyphens](https://developers.google.com/style/hyphens), [numbers](https://developers.google.com/style/numbers), [dates](https://developers.google.com/style/dates-times), [units](https://developers.google.com/style/units-of-measure))

- Serial comma; semicolon or period before "however".
- [Em dash](https://developers.google.com/style/dashes), no spaces, at most one dashed aside per sentence; prefer a comma, colon or parentheses. Hyphenate compound modifiers (read-only file), not after `-ly`.
- Straight quotes, commas and periods inside; no slashes: `and/or` → or both.
- Zero to nine spelled out, numerals from 10; versions, sizes, ports, percentages always numerals.
- Dates: January 19, 2017, or ISO `2017-01-19` when machine-readable; never `01/19/17`. Times: 3 PM.
- Units: `64 GB` with a nonbreaking space; 64-bit system; `-40 °C to 85 °C`.

## Formatting and UI ([text formatting](https://developers.google.com/style/text-formatting), [UI elements](https://developers.google.com/style/ui-elements), [images](https://developers.google.com/style/images))

- Bold UI labels, italic defined terms (sparingly), code font for code, plain product names.
- UI verbs: click a button; select or clear a checkbox; turn on a toggle; press a key; enter text. Never `hit`, `toggle`, `deselect`, or `hover` (hold the pointer over); `hit`, `deselect` and `hover` are [word list](https://developers.google.com/style/word-list) entries.
- Name the goal, not the widget. Menus: **File > Open**. Keys as `<kbd>`: **Control+S** (**Command+S** on macOS).
- No direction or color (`the button above`); use preceding and following.
- Use images when they clarify the task; concise meaningful alt text, or `alt=""` if decorative. Keep text and code selectable; crop screenshots and cover personal data.
- [File names](https://developers.google.com/style/filenames): lowercase, hyphens, ASCII (`set-up-billing.md`); in prose, the `main.tf` file.

## Diagrams

- Draw order, branches, or component relations when easier to see than prose; split big ones into an overview and details; put conditions on edge labels.
- Draw only edges and orders the source states.
- One caption line; no prose repeat of edges. Text keeps thresholds, commands, IDs, owners, and reasons.
- Short linear chains stay inline: `build -> test -> publish`.
