# Headings And Lists

Load when checking page skeleton: titles, headings, paragraph flow, lists.

## Headings and titles

- Sentence case everywhere: capitalize the first word and proper nouns only. No end period. Contractions and articles follow the same rules as body text.
- Title the document by its primary purpose; one unique H1 per page, used once; never skip levels (H2 → H4); every heading is followed by content.
- Task sections take the bare infinitive: "Create an instance", not "Creating an instance". Concept sections take a noun phrase: "Migration to Cloud Run". Both styles can appear in one document.
- Avoid an `-ing` form as the first word, but keep it when no better alternative exists ("Billing", "Pricing"), and it's fine later in a heading.
- Don't number headings to signal sequence, don't link inside a heading, and don't use heading tags for visual styling.
- Avoid code items in headings; if you must, pair them with a descriptive noun ("The `Delimiter` class").
- You can define an abbreviation in a heading when the added length pays for itself; otherwise define it in the first paragraph. Only use the abbreviation if it's the better-known form.
- Don't repeat the exact page title as a heading on the page. Optional sections start with "Optional:".
- Frequently linked headings deserve stable anchors (`references/style-links.md`).

## Paragraphs and flow

- Lead with the point; one topic per paragraph; sentence and paragraph limits live in `references/style-voice.md`.
- Introduce a group of subsections with "the following sections" — not "this section" or "these sections".
- Transitions carry the logic; don't rely on the reader inferring order.

## Lists

- Four types: numbered for any sequence-significant order, bulleted for unordered items, description lists for term/definition pairs, and description lists with bulleted run-in headings.
- Introduce a list with a complete sentence — colon when the list follows immediately, period when other material (a note, a paragraph) intervenes. IF the preceding heading already gives all the context → THEN no introduction is needed. Never let list items complete a fragment.
- Bulleted lists must make clear whether every item is required.
- Capitalize the first word of each item unless case carries meaning (a code item, a flag, a glossary term).
- End punctuation: period on items that are sentences or contain a verb; none on single words, verbless fragments, code-only items, or items that are entirely link text or a document title.
- IF punctuation ends up mixed → THEN rewrite for parallel construction, or punctuate every item.
- Parallel structure across items; don't attach an explanatory phrase to one item only — use a description list instead.
- No single-item lists; set a lone item off with other formatting.
- Nest with lowercase letters, then lowercase Roman numerals. Multiple paragraphs in one item use real paragraphs, not line breaks.
- Three or more properties per item belongs in a table (`references/style-blocks.md`).

## Description lists and run-in headings

- Start each term with a capital letter and don't end it with a period.
- A run-in heading is bold, starts capitalized, and ends with a period or a colon — consistently within the list.
- Text after a period starts capitalized; text after a colon starts lowercase.
- End the description with a period when it contains a verb or stands as a thought; leave it off for short verbless phrases.
- Separate a term from its description with a colon, a period, or a description list — never a dash (`references/style-punctuation.md`).

Next: numbered steps → `references/style-procedures.md`; notices, tables → `references/style-blocks.md`; images → `references/style-images.md`.
