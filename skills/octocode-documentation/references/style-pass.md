# Style pass: defaults, review, and sources

Load for any wording, formatting, or terminology pass, a style review someone else acts on, a disputed or missing rule, or possible guide drift. Topic pages snapshot the [Google developer documentation style guide](https://developers.google.com/style) (CC BY 4.0); the live guide wins. Load only the owner page the `SKILL.md` map names.

## Defaults to apply without looking anything up

Before you cite a rule or override wording, open the owning reference.

Beyond the lobby defaults:

- Imperative steps, condition first; code font for code, bold for UI labels.
- No `currently/soon`, no superlatives, no pre-announcements.

## Deviations

A deliberate, consistent deviation is fine; name the overridden rule. Don't add a second scheme.

## Review order

For a style review someone else acts on (not a direct edit):

1. Run `scripts/style-lint.mjs <paths>`; its hits are the report spine (ERROR gates, WARN mechanical, INFO judgment). It reads Markdown only, one finding per rule per line; hand-check docstrings, HTML, and UI strings. Judge each hit with the reference its message names.
2. Read for structure: headings, one Diátaxis type, step integrity.
3. Read for prose: person, voice, tense, modal words, condition first.
4. Spot-check formatting, code font, links, notices; tables, figures, and procedures as a screen-reader user.

| Severity | Meaning | Examples |
|---|---|---|
| Blocker | misleads or blocks accessibility | missing alt text, "click here", unreleased-feature promise, unsupported performance claim, non-inclusive term |
| Major | costs comprehension or translation | passive instructions, future tense, condition after instruction, Title Case headings, required step in a notice |
| Minor | consistency | serial comma, number style, date format |

Blockers and majors get a rewrite; minors batch as one line per rule with counts.

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

- Quote the original and give the replacement; a rule name alone isn't actionable.
- A fix that depends on an unverified fact: mark "needs verification" and route to `octocode-research`.
- List unread sections under "Not checked".

## Check the live page

Pages live at `https://developers.google.com/style/<slug>`; the changelog is [What's new](https://developers.google.com/style/whats-new). Open the owning page when:

- Someone disputes a rule or cites one the pack doesn't carry.
- No reference owns the question (outside the 69 pages in `assets/google-style-pages.tsv`).
- The wording is risky: trademark, product name, legal or security claim, public API reference.
- You claim the guide changed (read `whats-new` first) or a word-list entry looks wrong (`scripts/refresh-word-list.mjs --dry-run`).

Then quote the guide's sentence with its URL. Live page contradicts a reference: fix the reference in the same turn and say which rule moved. New rule: add it to the owning reference, never a new file. Fetch fails: answer from the reference, marked "not verified against the live guide".

## Page ownership and drift

`assets/google-style-pages.tsv` maps all 69 guide pages (`slug`, `title`, `owner`, `url`); each reference cites one primary page. URLs resolved on 2026-08-18 (newest changelog entry July 7, 2026).

```bash
grep -P "^tables\t" assets/google-style-pages.tsv        # which reference owns a page
cut -f3 assets/google-style-pages.tsv | sort | uniq -c    # pages per reference
```

This page ends the style lookup; return to the `SKILL.md` flow.
