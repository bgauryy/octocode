# Page structure: headings, lists, procedures, notices, and tables

Load when checking a page skeleton, numbered steps, callouts, or tabular data.

## Headings and titles

- Sentence case: capitalize the first word and proper nouns only. No end period.
- Title by primary purpose; one H1 per page; never skip levels (H2 → H4); every heading has content.
- Task sections: bare infinitive ("Create an instance"). Concept sections: noun phrase ("Migration to Cloud Run"). Both can share a page.
- Avoid an `-ing` first word unless no better option exists ("Billing"); fine later in a heading.
- No numbered headings for sequence, no links in headings, no heading tags for styling.
- Avoid code items in headings; if needed, add a noun ("The `Delimiter` class").
- Don't repeat the page title as a heading. Optional sections start with "Optional:". Stable anchors: `references/style-claims.md`.
- Introduce subsections with "the following sections", not "this section".

## Lists

- Numbered for order that matters, bulleted for unordered items, description lists for term/definition pairs (optionally with bold run-in headings).
- Introduce with a full sentence: colon when the list follows, period when other material intervenes. Skip it when the heading gives all the context. Items never complete a fragment.
- Say whether every bulleted item is mandatory. Capitalize each item unless case carries meaning (code, flag, glossary term).
- Period on items that are sentences or contain a verb; none on single words, verbless fragments, code-only items, or link-only or title-only items. Mixed result: rewrite in parallel, or punctuate all.
- Parallel items; no explanation on one item only (use a description list). No single-item lists.
- Nest with lowercase letters, then lowercase Roman numerals. Real paragraphs, not line breaks, inside an item.
- Description terms: capitalized, no period. Run-in headings: bold, capitalized, a period or colon used the same way throughout; capital after a period, lowercase after a colon.
- Description ends with a period when it has a verb or is a full thought. Separate term and description with a colon, period, or description list, never a dash.

## Procedures

- Numbered steps; substeps a, b, c; then i, ii, iii. A one-step procedure is one bulleted item.
- Introduce with a full sentence ending in a colon ("do the following:"); drop it if it only repeats the heading.
- Prerequisites, permissions, and software go before step 1. Link an existing procedure; don't repeat it.
- Document one path that works for everyone: keyboard-accessible, shortest, most familiar. Never a keyboard shortcut as the documented way.
- Each step starts with an imperative; location and goal come first ("In the console, click **Create**"; "To enable billing, click **Enable**"). If "To …" might read as optional, name the outcome first: "Start a new document: click **File** > **New** > **Document**".
- One action per step; chain only trivial menu hops. Optional steps: "Optional: Enter a description.", not `(Optional)`. Say when to press Enter.
- Give a reason when it prevents a mistake ("You need it in the next step"). Keep a result in its step; no separate "the dialog appears" step.
- Bold only the UI elements the reader acts on. A step with substeps ends its text with a colon or period.
- Complex step order: action → command → placeholder explanations → what it does → sample output → result. Say what a command accomplishes, not "run the following command".
- Not in a procedure: directional language (hard-to-find element: screenshot or icon name, `references/style-format.md`), tables, notices that hold a required step.

## Notices

Four common types; any other needs a house convention:

| Type | Use for |
|---|---|
| Note | aside or tip, not needed for success |
| Caution | proceed carefully |
| Warning | the outcome might be irreversible: data, money, or work loss, security breach |
| Success | completed action or clean state; interactive content only |

- Start with the bold label: `**Note:** …`. HTML fallback: `<aside class="note"><b>Note:</b> …</aside>`.
- No required information, prerequisites, steps, or cross-references in a notice. Unsure: write it as regular text first.
- Use notices sparingly; never stack two; restructure instead.

## Tables

- Use a table for three or more related values per row; pairs are a description list; one dimension is a list.
- No table for layout, one column, code blocks, a split one-dimensional list, mid-sentence, or one data row (reference entries excepted).
- Split a long or complex table.
- Introduce like a list, naming its position ("the following table").
- Refer back by number ("table 2"), never by direction. Avoid links to tables. Lowercase "table" mid-sentence.
- Caption when there are several: `**Table 1.** Supported regions`, sentence case, no period.
- Header row and header column only; sentence case; concise; no end punctuation; no merged cells (`colspan`, `rowspan`).
- Sort rows logically or alphabetically; parallel cells; paragraph elements, not line breaks; footnotes right after the table.
- Accessibility: `th` with `scope`; alt text for images or symbols in cells; responsive width.
- Avoid footnotes; use a cross-reference, note, or parenthetical. Unavoidable: superscript number, text at page bottom.

Source: [Headings and titles](https://developers.google.com/style/headings).
