# Punctuation, numbers, dates, and units

Load when checking commas, colons, dashes, quotes, or any other mark, or when text contains a quantity, date, time, unit, phone number, or formula.

## Commas

- Serial comma before the final "and" or "or": "zones, regions, and multi-regions".
- Comma after an introductory word or phrase; comma before a coordinating conjunction joining two independent clauses unless both are very short.
- Between an independent and a dependent clause, add a comma only when a reader might otherwise misread the sentence.
- Comma before nonrestrictive "which"; none before "because" unless the clause is nonrestrictive. Put a semicolon, period, or dash before "however", "therefore", or "otherwise", then a comma after it.

## Colons and semicolons

- When a colon introduces a list, the text before it must stand alone as a sentence: "The fields are as follows:", not "The fields are:". Run-in labels ("Tone:", "Optional:", "**Note:**") are fine.
- Lowercase the first word after a colon unless it's a proper noun, a heading, a quotation, or a notice label.
- Avoid semicolons. They earn their place in three cases: two tightly linked independent clauses; a conjunctive adverb or a phrase such as "that is"; long items that carry their own commas.

## Dashes and hyphens

- Em dash (`—`) marks a break in a sentence, with no spaces around it. Don't substitute an en dash or hyphen, and don't use a dash to separate a term from its description — use a colon, a period, or a description list.
- Hyphenate a compound modifier before a noun ("read-only file"); after a verb, usually don't ("the app is well designed", "written in real time"). Some compounds are always hyphenated: `on-premises`, `add-on`, `cloud-based`, `customer-facing`, `user-friendly`.
- Don't hyphenate an `-ly` adverb ("publicly available"). Hyphenate after "more" or "most" only to prevent a misread ("more-reliable links").
- Three-word modifiers are better rewritten; if you must keep one, hyphenate between each word (`cross-data-center replication`).
- Hyphenate after `self-` and `cross-`, before a capitalized word or a number, when the base term already contains hyphens or spaces (`un-Google-like`), for `non-` compounds that are hard to parse (`non-KSA-based`), and for consistency within a document (`pre-processing`, `post-processing`). Compound nouns are otherwise closed; the word list decides exceptions.
- No spaces around hyphens; a suspended hyphen takes a following space only ("2- to 3-minute delay").
- Number plus unit: see § Units of measure.

## Quotation marks and apostrophes

- Straight quotes and apostrophes only, never curly.
- Double quotes for titles of short works, quoted text, an unlinked reference to a document section, a metaphor, or a person's exact words; single quotes only inside code or nested in another quotation.
- Commas and periods go inside the closing quotation mark — but when the quotation marks fence an exact literal string, put other punctuation outside so nothing extra lands inside the string. Items in code font take no quotation marks at all unless the quotes are part of the code.
- Don't quote link text or UI labels; use the right formatting instead (`references/style-format.md`).

## Parentheses, periods, ellipses, slashes

- Readers skip parentheses, so keep important information out of them and keep a mid-sentence parenthetical short. Don't park an optional plural in parentheses (`key(s)`).
- A standalone sentence inside parentheses keeps its period inside; a parenthetical inside a larger sentence puts the period outside.
- End every complete sentence with a period, except headings, titles, and short list items. One space between sentences. Don't leave a URL at the end of a sentence where the period might look like part of the link. IF quoted material ends in a question mark → THEN don't add a period.
- Never use an exclamation point in concept or reference documentation. It's fine inside code (`!=`), in a quoted literal or error string, and occasionally to mark a milestone in a tutorial.
- Avoid ellipses. In quoted text they mark an internal omission: three periods with a space on each side, no space after when punctuation follows, and four dots when the omission spans a sentence boundary. Don't use them for omitted code — use a comment (`references/style-code.md`). Drop a trailing ellipsis from a UI label: "click **Save**".
- Avoid slashes: write "or", "and", or "or both"; `and/or` is acceptable only where space is tight, such as a table. No `w/`, `c/o`, or slash dates or fractions (`0.75`, `75%`, or `¾`). Keep slashes for paths, URLs, and code; Windows paths take backslashes; break a long URL after a slash.

## Example introductions

At the end of a sentence, use "such as" or "like", or an em dash before "for example" — "…for your managed instances—for example, CPU utilization". Mid-sentence, a short parenthetical works ("(for example, `228B22`)"). Never fence "for example," with commas at the end of a sentence, and never introduce an example with a semicolon.

## Numbers

- Spell out zero through nine; numerals for 10 and up. IF one number in a sentence is 10 or more → THEN use numerals for all of them.
- Always numerals for versions, memory sizes, disk sizes, ports, prices, step numbers, chapter numbers, section numbers, dimensions, measurements, negative numbers, decimals, percentages, ranges, and technical quantities ("6 queries per second").
- Spell out a number that starts a sentence, or rearrange the sentence; a four-digit year can start one, though it reads better moved. IF a percentage starts a sentence → THEN spell out both parts ("Forty percent of the files").
- Where a numeral sits next to another numeral, spell one out: "creates fifteen 100,000-byte files".
- Ordinals are words: first, second, third. Avoid Roman numerals except for substeps.
- Leading zero on decimals below one (`0.5 seconds`); decimals are plural even at 1.0 ("1.0 inches"); comma separators from four digits up (`1,532,784`) and never to the right of the decimal point.
- Express fractions as decimals when you can; hyphenate spelled-out fractions ("five sixty-fourths"). Dimensions use a lowercase x with no spaces (`192x192`). "Millions" and "billions" are fine for approximations.
- Currency leads with the symbol (`$10`), takes no punctuation to the right of the decimals, and disambiguates when needed (`US$10`).

## Dates and times

- Full month name, day, four-digit year: `January 19, 2017`. Use ISO 8601 (`2017-01-19`) when the form must be machine-readable. Never the all-numeric `MM/DD/YY`.
- Mid-sentence, a comma follows the year: "The January 19, 2017, release adds…". No comma between month and year alone. Day of week comes first: "Tuesday, April 27, 2021". Date before time: `May 4, 2009, at 6 PM`.
- IF space is tight → THEN use three-letter abbreviations with no periods ("Mon, Sep 3, 2018"); don't mix abbreviated and spelled-out forms.
- Pick an example day greater than 12 so the format can't be misread.
- 12-hour clock, capitalized AM/PM, one space, minutes dropped on the hour (`3 PM`); use exact times where possible. Noon and midnight are fine. 24-hour only when the UI or code uses it — then use it throughout the page.
- Spell out the region with the offset — "US and Canadian Pacific Standard Time (UTC-8)" — or mirror the timestamp the UI shows; "10 AM your local time" also works. Prefer avoiding time zones. Replace seasons with months or quarters.

## Units of measure

- Nonbreaking space between number and unit (`64 GB`, `300 K`); no space before `%`, the degree symbol, or a currency symbol; no space before `k` in "55k download operations" — and add a noun.
- Temperature: nonbreaking space between the numeral and the degree symbol, none before the scale — `50 °C`, `98.6 °F`. Kelvin drops the degree symbol: `300 K`.
- Hyphenate a spelled-out unit that modifies a noun (`a 64-bit system`, `a five-minute wait`); leave an abbreviated unit open (`200 GB disk`). Hyphenate multiplied units (`5 vCPU-hours`).
- Use "per" instead of a division slash when space allows (`requests per day`, `Gbps` over `Gb/s`).
- Ranges repeat symbols and abbreviations and take "to" for units (`-40 °C to 85 °C`); plain number ranges take a hyphen (`2012-2016`). Don't repeat a noun unit and don't mix a hyphen with words ("from 8 to 20 files").
- Decimal byte units are kB, MB, GB, TB; binary units are KiB, MiB, GiB, TiB. Use what the product reports.
- Accompany an abstract quantity with a practical implication so the reader can picture it.

## Phone numbers

- Example numbers come from the reserved range `800-555-0100` through `800-555-0199`; never a real number.
- Format with nonbreaking hyphens between area code, exchange, and line: `415-555-0132`; international numbers add the country code (`+1-415-555-0132`); extensions read "`415-555-0132`, extension 987".

## Mathematical notation

- Prefer notation to words in running text: "Check whether a > b", not "whether a is greater than b" — unless the notation is ambiguous or hard to read.
- HTML entities for symbols (`&times;`, `&minus;`, `&le;`); keyboard characters for `+`, `=`, `/`. Never the caret for exponentiation or an asterisk for multiplication.
- Nonbreaking spaces on both sides of an operator; don't italicize operators; variables are italic; code identifiers stay in code font.
- Keep short expressions inline; give an equation its own line when wrapping breaks it. Superscripts and subscripts use `<sup>` and `<sub>`.
- A diagram or chart often serves the reader better than the algebra.

Upstream: [Colons](https://developers.google.com/style/colons) · [Commas](https://developers.google.com/style/commas) · [Dashes](https://developers.google.com/style/dashes) · [Ellipses](https://developers.google.com/style/ellipses) · [Hyphens](https://developers.google.com/style/hyphens) · [Parentheses](https://developers.google.com/style/parentheses) · [Periods and end punctuation](https://developers.google.com/style/periods) · [Quotation marks](https://developers.google.com/style/quotation-marks) · [Semicolons](https://developers.google.com/style/semicolons) · [Slashes](https://developers.google.com/style/slashes) · [Examples](https://developers.google.com/style/format-examples) · [Numbers](https://developers.google.com/style/numbers) · [Dates and times](https://developers.google.com/style/dates-times) · [Units of measurement](https://developers.google.com/style/units-of-measure) · [Phone numbers](https://developers.google.com/style/phone-numbers) · [Mathematical notation](https://developers.google.com/style/mathematical-notation). Verify a disputed or missing rule against the live page → `references/style-pass.md`.
