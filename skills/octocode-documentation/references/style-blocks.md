# Notices And Tables

Load when the page uses callouts or tabular data.

## Notices

Four notice types are in common use — anything else needs a house convention:

| Type | Use for |
|---|---|
| Note | useful aside or tip; not required for success |
| Caution | proceed carefully |
| Warning | don't do this: the outcome may be irreversible — data loss, lost money, lost work, a security breach |
| Success | a completed action or clean state; interactive content only |

- Start the notice with the bolded label: `**Note:** …`.
- Don't put required information, prerequisites, earlier steps, procedure steps, or cross-references in a notice — readers skip notices, and that content belongs in the flow.
- IF you can't tell whether something is a notice → THEN write it as regular text first.
- Use notices sparingly and never stack two; if two land together, restructure the section.
- HTML fallback when the site has no component: `<aside class="note"><b>Note:</b> …</aside>`.

## Tables

- A table earns its place at three or more pieces of related data per row; two-part pairs are a description list (sometimes a table); one dimension is a list.
- Never use a table for layout, for a single column, for code blocks, to spread a one-dimensional list across columns, or in the middle of a sentence. One row of data usually isn't a table either — reference entries are the exception.
- Avoid tables inside a numbered procedure; a long or complicated table is often two tables.
- Introduce every table with a complete sentence and refer to its position: "the following table", "the preceding table" — because not all screen readers announce tables. Use a colon when the table follows immediately, a period when other material intervenes.
- Refer back to a table by number ("table 2"), never by direction ("the table below"). Avoid linking to tables. Don't capitalize "table" unless it starts a sentence.
- Caption when more than one table appears: `**Table 1.** Supported regions` — sentence case, no period.
- Header row and header column only; sentence case; concise; no end punctuation in headings; no merged cells (`colspan`, `rowspan`).
- Sort rows logically or alphabetically; keep cell contents parallel; multi-paragraph cells use paragraph elements, not line breaks.
- IF the table needs footnotes → THEN place them immediately after the table.
- Accessibility: real `th` with `scope`; alt text for any image or symbol inside a cell; make the table responsive to viewport width.

## Footnotes

Avoid them — hard to reach with a screen reader, awkward to localize. Use a cross-reference, a note, or a parenthetical. IF a footnote is unavoidable → THEN use a superscript number and put the text at the bottom of the page.

Next: figures and alt text → `references/style-images.md`; code and command formatting → `references/style-code.md`.
