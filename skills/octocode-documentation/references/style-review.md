# Review Pass

Load when the deliverable is a style review someone else acts on, not a direct edit.

## Order

1. Run `scripts/style-lint.mjs <paths>` and keep the machine hits as the spine of the report. Read the levels: ERROR gates, WARN is mechanical, INFO needs judgment (passive voice, serial comma, word list, sentence length). It reports one finding per rule per line and reads Markdown only — docstrings, HTML, and UI strings need a hand pass, and every finding is a candidate, not a verdict.
2. Read the page once for structure: heading case and hierarchy, one Diátaxis type, step integrity (`references/style-structure.md`).
3. Read again for prose: person, voice, tense, modal words, condition-first order (`references/style-voice.md`).
4. Spot-check formatting, code font, links, and notices only where the page actually uses them.
5. IF the page carries structure that matters (tables, figures, procedures) → THEN check it the way a screen-reader user would: headings in order, alt text present, no direction-only instructions.
6. Stop at the first two rules a page violates repeatedly — a systemic fix beats 40 line notes.

## Severity

| Level | Meaning | Examples |
|---|---|---|
| Blocker | misleads the reader or blocks accessibility | missing alt text, "click here" as only link text, promise of an unreleased feature, unsupported performance claim, non-inclusive term |
| Major | costs comprehension or translation quality | passive instructions, future tense for current behavior, condition after instruction, Title Case headings, notice holding a required step |
| Minor | consistency | serial comma, number style, date format, code font on a product name |

Report blockers and majors with a rewrite; batch minors as one line per rule with counts.

## Report shape

```text
Style review: <paths>  (<blockers> blocker, <majors> major, <minors> minor)

Systemic
- <rule> — <count> hits, one fix: <what to change globally>

Line findings
- <file>:<line> [<severity>] <rule> — <quote>
  → <rewrite>

Consistent deviations kept
- <repo convention that overrides the guide, and where it is documented>

Not checked
- <sections skipped and why>
```

- Quote the original and give the replacement text; a rule name alone is not actionable.
- IF a fix depends on an unverified fact → THEN mark it "needs verification" and route to `octocode-research`.
- Never report a clean bill of health for sections you didn't read; list them under "Not checked".

Next: topic map for any rule you need to cite → `references/style-index.md`; guide page behind a rule → `references/style-sources.md`.
