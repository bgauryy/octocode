# Numbers, Dates, Units

Load when text contains a quantity, date, time, unit, phone number, or formula.

## Numbers

- Spell out zero through nine; numerals for 10 and up. IF one number in a sentence is 10 or more → THEN use numerals for all of them.
- Always numerals for versions, memory and disk sizes, ports, prices, step numbers, chapter and section numbers, dimensions, measurements, negative numbers, decimals, percentages, ranges, and technical quantities ("6 queries per second").
- Spell out a number that starts a sentence, or rearrange the sentence; a four-digit year may start one, though it reads better moved. IF a percentage starts a sentence → THEN spell out both parts ("Forty percent of the files").
- Where a numeral sits next to another numeral, spell one out: "creates fifteen 100,000-byte files".
- Ordinals are words: first, second, third. Avoid Roman numerals except for substeps.
- Leading zero on decimals below one (`0.5 seconds`); decimals are plural even at 1.0 ("1.0 inches"); comma separators from four digits up (`1,532,784`) and never to the right of the decimal point.
- Express fractions as decimals when you can; spelled-out fractions are hyphenated ("five sixty-fourths"). Dimensions use a lowercase x with no spaces (`192x192`). "Millions" and "billions" are fine for approximations.
- Currency leads with the symbol (`$10`), takes no punctuation to the right of the decimals, and disambiguates when needed (`US$10`).

## Dates and times

- Full month name, day, four-digit year: `January 19, 2017`. ISO 8601 (`2017-01-19`) when a machine-readable form is required. Never `01/19/17`.
- Mid-sentence, a comma follows the year: "The January 19, 2017, release adds…". No comma between month and year alone. Day of week comes first: "Tuesday, April 27, 2021". Date before time: `May 4, 2009, at 6 PM`.
- IF space is tight → THEN three-letter abbreviations are allowed with no periods ("Mon, Sep 3, 2018"); don't mix abbreviated and spelled-out forms.
- Pick an example day greater than 12 so the format can't be misread.
- 12-hour clock, capitalized AM/PM, one space, minutes dropped on the hour (`3 PM`); exact times where possible, though noon and midnight are fine. 24-hour only when the UI or code uses it — then use it throughout the page.
- Spell out the region with the offset — "US and Canadian Pacific Standard Time (UTC-8)" — or mirror the timestamp the UI shows; "10 AM your local time" also works. Prefer avoiding time zones. Replace seasons with months or quarters.

## Units of measure

- Nonbreaking space between number and unit (`64 GB`, `300 K`); no space before `%`, the degree symbol, or a currency symbol; no space before `k` in "55k download operations" — and add a noun.
- Temperature: nonbreaking space between the numeral and the degree symbol, none before the scale — `50 °C`, `98.6 °F`. Kelvin drops the degree symbol: `300 K`.
- A spelled-out unit modifying a noun is hyphenated (`a 64-bit system`, `a five-minute wait`); an abbreviated unit is not (`200 GB disk`). Multiplied units are hyphenated (`5 vCPU-hours`).
- Use "per" instead of a division slash when space allows (`requests per day`, `Gbps` over `Gb/s`).
- Ranges repeat symbols and abbreviations and take "to" for units (`-40 °C to 85 °C`); plain number ranges take a hyphen (`2012-2016`). Don't repeat a noun unit and don't mix a hyphen with words ("from 8 to 20 files").
- Decimal byte units are kB, MB, GB, TB; binary units are KiB, MiB, GiB, TiB. Use what the product reports.
- Accompany an abstract quantity with a practical implication so the reader can picture it.

## Phone numbers

- Example numbers come from the reserved range `800-555-0100` through `800-555-0199`; never a real number.
- Format with nonbreaking hyphens between area code, exchange, and line: `415-555-0132`; international numbers add the country code (`+1-415-555-0132`); extensions read "`415-555-0132`, extension 987".

## Mathematical notation

- Prefer notation to words in running text: "Check whether a > b", not "whether a is greater than b" — unless the notation would be ambiguous or hard to read.
- HTML entities for symbols (`&times;`, `&minus;`, `&le;`); keyboard characters for `+`, `=`, `/`. Never the caret for exponentiation or an asterisk for multiplication.
- Nonbreaking spaces on both sides of an operator; don't italicize operators; variables are italic; code identifiers stay in code font.
- Keep short expressions inline; give an equation its own line when wrapping would break it. Superscripts and subscripts use `<sup>` and `<sub>`.
- A diagram or chart often serves the reader better than the algebra.

Next: how numbers get formatted → `references/style-format.md`; tables holding them → `references/style-blocks.md`.
