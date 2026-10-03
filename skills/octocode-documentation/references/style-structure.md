# Page structure: headings, lists, procedures, notices, and tables

Load when checking page skeleton (titles, headings, paragraph flow, lists), when writing or reviewing numbered steps and task instructions, or when the page uses callouts or tabular data.

## Headings and titles

- Sentence case everywhere: capitalize the first word and proper nouns only. No end period. Contractions and articles follow the same rules as body text.
- Title the document by its primary purpose; one unique H1 per page, used once; never skip levels (H2 → H4); every heading carries content.
- Task sections take the bare infinitive: "Create an instance", not "Creating an instance". Concept sections take a noun phrase: "Migration to Cloud Run". Both styles can appear in one document.
- Avoid an `-ing` form as the first word, but keep it when no better alternative exists ("Billing", "Pricing"), and it's fine later in a heading.
- Don't number headings to signal sequence, don't link inside a heading, and don't use heading tags for visual styling.
- Avoid code items in headings; if you must, pair them with a descriptive noun ("The `Delimiter` class").
- You can define an abbreviation in a heading when the added length pays for itself; otherwise define it in the first paragraph. Only use the abbreviation if it's the better-known form.
- Don't repeat the exact page title as a heading on the page. Optional sections start with "Optional:". Frequently linked headings deserve stable anchors (`references/style-claims.md`).
- Paragraphs and flow: lead with the point; one topic per paragraph; sentence and paragraph limits live in `references/style-prose.md`. Introduce a group of subsections with "the following sections" — not "this section" or "these sections". Transitions carry the logic; don't rely on the reader inferring order.

## Lists

- Four types: numbered for any sequence-significant order, bulleted for unordered items, description lists for term/definition pairs, and description lists with bulleted run-in headings.
- Introduce a list with a complete sentence — colon when the list follows immediately, period when other material (a note, a paragraph) intervenes. IF the preceding heading already gives all the context → THEN skip the introduction. Never let list items complete a fragment.
- Bulleted lists must say whether every item is mandatory. Capitalize the first word of each item unless case carries meaning (a code item, a flag, a glossary term).
- End punctuation: period on items that are sentences or contain a verb; none on single words, verbless fragments, code-only items, or items that are entirely link text or a document title. IF punctuation ends up mixed → THEN rewrite for parallel construction, or punctuate every item.
- Parallel structure across items; don't attach an explanatory phrase to one item only — use a description list instead. No single-item lists; set a lone item off with other formatting.
- Nest with lowercase letters, then lowercase Roman numerals. Multiple paragraphs in one item use real paragraphs, not line breaks. Three or more properties per item belongs in a table (§ Tables).
- Description lists: start each term with a capital letter and don't end it with a period. A run-in heading is bold, starts capitalized, and ends with a period or a colon — consistently within the list. Text after a period starts capitalized; text after a colon starts lowercase.
- End the description with a period when it contains a verb or stands as a thought; leave it off for short verbless phrases. Separate a term from its description with a colon, a period, or a description list — never a dash (`references/style-punctuation.md`).

## Procedures

- Numbered steps for a sequence; substeps a, b, c; sub-substeps i, ii, iii. A one-step procedure is a single bulleted item, not a list of one.
- Introduce the procedure with a complete sentence, ending in a colon before the steps; "do the following:" is a good closer. IF the introduction only repeats the heading → THEN drop it.
- Prerequisites, permissions, and required software go before step 1. Don't repeat a procedure that already exists — link to it.
- Document one procedure that works for everyone: prefer the keyboard-accessible, shortest, most familiar path. Don't document keyboard shortcuts as the way to do a task.
- The first sentence of a step contains an imperative verb; location and goal come before the verb: "In the console, click **Create**"; "To enable billing, click **Enable**". IF the "To …" opener might read as optional → THEN name the outcome first: "Start a new document: click **File** > **New** > **Document**".
- One action per step. Chain only trivial menu hops in a single bold sequence. Optional steps read "Optional: Enter a description." — not `(Optional)`. IF the reader must press Enter → THEN say so inside the step.
- Give the reason when it prevents a mistake: "Store the key. You need it in the next step." Keep the result in the same step when it matters ("Click **Run**. The results appear in the console."), and don't split one action into an action step plus a "the dialog appears" step.
- Avoid bolding every UI element in sight; bold the ones the reader acts on. IF a step has substeps → THEN treat its text as an introduction and end it with a colon or a period.
- Complex steps: order the parts action → command → placeholder explanations → what the command does → sample output → result. Don't introduce a code block with "run the following command"; say what the command accomplishes.
- What doesn't belong: directional language ("the button below", "the left pane") — IF an element is genuinely hard to find → THEN show a screenshot or name it with its icon (`references/style-format.md`); tables in the middle of the procedure; notices carrying a required step — put it in the flow.

## Notices

Four notice types are in common use — anything else needs a house convention:

| Type | Use for |
|---|---|
| Note | useful aside or tip; not required for success |
| Caution | proceed carefully |
| Warning | don't do this: the outcome might be irreversible — data loss, lost money, lost work, a security breach |
| Success | a completed action or clean state; interactive content only |

- Start the notice with the bolded label: `**Note:** …`. HTML fallback when the site has no component: `<aside class="note"><b>Note:</b> …</aside>`.
- Don't put required information, prerequisites, earlier steps, procedure steps, or cross-references in a notice — readers skip notices, and that content belongs in the flow. IF you can't tell whether something is a notice → THEN write it as regular text first.
- Use notices sparingly and never stack two; if two land together, restructure the section.

## Tables

- A table earns its place at three or more pieces of related data per row; two-part pairs are a description list (sometimes a table); one dimension is a list.
- Never use a table for layout, for a single column, for code blocks, to spread a one-dimensional list across columns, or in the middle of a sentence. One row of data usually isn't a table either — reference entries are the exception.
- Avoid tables inside a numbered procedure; a long or complicated table is often two tables.
- Introduce every table with a complete sentence and refer to its position: "the following table", "the preceding table" — because not all screen readers announce tables. Use a colon when the table follows immediately, a period when other material intervenes.
- Refer back to a table by number ("table 2"), never by direction (`the table below`). Avoid linking to tables. Don't capitalize "table" unless it starts a sentence.
- Caption when more than one table appears: `**Table 1.** Supported regions` — sentence case, no period.
- Header row and header column only; sentence case; concise; no end punctuation in headings; no merged cells (`colspan`, `rowspan`).
- Sort rows logically or alphabetically; keep cell contents parallel; multi-paragraph cells use paragraph elements, not line breaks. IF the table needs footnotes → THEN place them immediately after the table.
- Accessibility: real `th` with `scope`; alt text for any image or symbol inside a cell; make the table responsive to viewport width.
- Footnotes: avoid them — hard to reach with a screen reader, awkward to localize. Use a cross-reference, a note, or a parenthetical. IF a footnote is unavoidable → THEN use a superscript number and put the text at the bottom of the page.

Upstream: [Headings and titles](https://developers.google.com/style/headings) · [Lists](https://developers.google.com/style/lists) · [Procedures](https://developers.google.com/style/procedures) · [Notes and other notices](https://developers.google.com/style/notices) · [Tables](https://developers.google.com/style/tables) · [Footnotes](https://developers.google.com/style/footnotes). Verify a disputed or missing rule against the live page → `references/style-pass.md`.
