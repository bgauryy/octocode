# Punctuation, numbers, dates, and units

Load when checking any punctuation mark, or when text holds a quantity, date, time, unit, phone number, or formula.

## Commas

- Serial comma before the final "and" or "or": "zones, regions, and multi-regions".
- Comma after an introductory phrase, and before a conjunction that joins two independent clauses unless both are very short.
- Comma between an independent and a dependent clause only to prevent a misread.
- Comma before nonrestrictive "which"; none before "because" unless nonrestrictive. Semicolon, period, or dash before "however", "therefore", "otherwise"; comma after.

## Colons and semicolons

- Text before a list colon is a full sentence: "The fields are as follows:", not "The fields are:". Run-in labels ("**Note:**") are fine.
- Lowercase after a colon unless a proper noun, heading, quotation, or notice label follows.
- Avoid semicolons except for two tightly linked independent clauses, before a conjunctive adverb or "that is", or between long items that hold commas.

## Dashes and hyphens

- Em dash (`—`) for a break, no spaces. No en dash or hyphen in its place.
- Hyphenate a compound modifier before a noun ("read-only file"), usually not after a verb. Always hyphenated: `on-premises`, `add-on`, `cloud-based`, `customer-facing`, `user-friendly`.
- No hyphen after an `-ly` adverb. After "more" or "most" only to prevent a misread ("more-reliable links").
- Rewrite three-word modifiers; if kept, hyphenate each word (`cross-data-center replication`).
- Hyphenate after `self-` and `cross-`, before a capital or number, when the base has hyphens or spaces (`un-Google-like`), for hard `non-` compounds (`non-KSA-based`), and for consistency (`pre-processing`). Other compound nouns are closed; the word list decides.
- No spaces around hyphens; a suspended hyphen takes a space after it ("2- to 3-minute delay").

## Quotation marks and apostrophes

- Straight quotes and apostrophes, never curly.
- Double quotes for short-work titles, quoted text, an unlinked section reference, a metaphor, or exact words. Single quotes only in code or nested quotes.
- Commas and periods inside the closing quote, except after an exact literal string: then other punctuation goes outside.

## Parentheses, periods, ellipses, slashes

- Keep important information out of parentheses; keep a mid-sentence parenthetical short.
- A standalone parenthetical sentence keeps its period inside; otherwise the period goes outside.
- No period on headings, titles, or short list items. Don't end a sentence with a URL where the period looks like part of it.
- No exclamation point in concept or reference docs; fine in code (`!=`), quoted literals, and rare tutorial milestones.
- Avoid ellipses. In quotes they mark an omission: three periods, a space each side, no space before following punctuation, four dots across a sentence boundary.
- Avoid slashes: write "or", "and", or "or both"; `and/or` only where space is tight. No `w/`, `c/o`, slash dates, or slash fractions (`0.75`, `75%`, `¾`). Slashes stay in paths, URLs, and code; Windows paths use backslashes; break a long URL after a slash.

## Example introductions

At sentence end: "such as", "like", or an em dash before "for example" ("…instances—for example, CPU utilization"). Mid-sentence: a short parenthetical ("(for example, `228B22`)"). Never commas around a sentence-final "for example"; never a semicolon before an example.

## Numbers

- Spell out zero through nine; numerals from 10. If one number in a sentence is 10 or more, use numerals for all.
- Always numerals: versions, memory and disk sizes, ports, prices, step, chapter, and section numbers, dimensions, measurements, negatives, decimals, percentages, ranges, technical quantities ("6 queries per second").
- Spell out or move a number that starts a sentence; a four-digit year may start one. A leading percentage spells out both parts ("Forty percent").
- Adjacent numerals: spell one out ("fifteen 100,000-byte files").
- Ordinals are words (first, second). Roman numerals only for substeps.
- Leading zero below one (`0.5`); decimals are plural even at 1.0 ("1.0 inches"); commas from four digits (`1,532,784`), never right of the decimal point.
- Fractions as decimals when possible; hyphenate spelled-out fractions ("five sixty-fourths"). Dimensions: lowercase x, no spaces (`192x192`).
- Currency: symbol first (`$10`), nothing after the decimals, disambiguate when needed (`US$10`).

## Dates and times

- `January 19, 2017`; ISO 8601 (`2017-01-19`) when machine-readable. Never `MM/DD/YY`.
- Mid-sentence, a comma follows the year ("The January 19, 2017, release"). No comma between month and year alone. Weekday first: "Tuesday, April 27, 2021". Date before time: `May 4, 2009, at 6 PM`.
- Tight space: three-letter abbreviations, no periods ("Mon, Sep 3, 2018"); don't mix forms.
- Example days greater than 12.
- 12-hour clock, capital AM/PM, one space, no minutes on the hour (`3 PM`). 24-hour only when the UI or code uses it, then throughout the page.
- Time zones: avoid them; else spell out the region with the offset ("US and Canadian Pacific Standard Time (UTC-8)"), mirror the UI timestamp, or say "your local time". Months or quarters, not seasons.

## Units of measure

- Nonbreaking space between number and unit (`64 GB`); no space before `%`, a degree symbol, or a currency symbol, or before `k` ("55k download operations", with a noun).
- Temperature: nonbreaking space before the degree symbol, none before the scale (`50 °C`); Kelvin has no degree symbol (`300 K`).
- Hyphenate a spelled-out unit before a noun (`64-bit system`, `five-minute wait`); abbreviated units stay open (`200 GB disk`). Hyphenate multiplied units (`5 vCPU-hours`).
- "per" over a division slash when space allows (`Gbps` over `Gb/s`).
- Unit ranges repeat symbols and use "to" (`-40 °C to 85 °C`); plain number ranges use a hyphen (`2012-2016`). Don't repeat a noun unit or mix a hyphen with words ("from 8 to 20 files").
- Decimal bytes kB, MB, GB, TB; binary KiB, MiB, GiB, TiB. Use what the product reports.
- Give an abstract quantity a practical implication.

## Phone numbers

- Nonbreaking hyphens: `415-555-0132`; international `+1-415-555-0132`; extension "`415-555-0132`, extension 987".

## Mathematical notation

- Notation over words ("whether a > b") unless ambiguous.
- HTML entities for symbols (`&times;`, `&minus;`, `&le;`); keyboard `+`, `=`, `/`. No caret for exponents, no asterisk for multiplication.
- Nonbreaking spaces around operators; operators upright, variables italic, identifiers in code font.
- Short expressions inline; an equation that wraps gets its own line. `<sup>` and `<sub>` for scripts.

Source: [Commas](https://developers.google.com/style/commas).

Next: return to the `SKILL.md` flow.
