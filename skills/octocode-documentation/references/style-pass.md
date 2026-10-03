# Style pass: defaults, review, and sources

Load for any wording, formatting, or terminology pass, for a style review someone else acts on, or when someone disputes a rule, the pack doesn't carry the rule you need, or the guide might have moved. Why: the topic pages restate a snapshot of the [Google developer documentation style guide](https://developers.google.com/style), and the live guide is the authority. The skill map in `SKILL.md` names the one topic page to load; load only that owner. STE-80 (ASD-STE100 rules) and diagrams for flows: `references/style-ste80.md`.

## Defaults to apply without looking anything up

Not the rule set: before you cite a rule or override wording, open the owning reference.

- Sentence case headings; second person; active voice; present tense; imperative steps with the condition before the instruction.
- Serial comma; descriptive link text; code font for code, bold for UI labels; alt text on every image.
- No `currently/soon`, no superlatives, no pre-announcements.

## Precedence

1. The project's own documented style guide, when it exists.
2. A convention this repository already applies consistently. Report the conflict; do not add a second scheme.
3. This guide.

Deviations are fine when deliberate and consistent; name the overridden rule. Each `style-*.md` ends with its guide-page links; the live pages outrank this pack (§ Check the live page). Next: run `scripts/style-lint.mjs <paths>` for the mechanical hits before hand-reading; interpret each finding with the reference named in its message.

## Review order

Use when the deliverable is a style review someone else acts on, not a direct edit.

1. Run `scripts/style-lint.mjs <paths>` and keep the machine hits as the spine of the report. Read the levels: ERROR gates, WARN is mechanical, INFO needs judgment (passive voice, serial comma, word list, sentence length). It reports one finding per rule per line and reads Markdown only — docstrings, HTML, and UI strings need a hand pass, and every finding is a candidate, not a verdict.
2. Read the page once for structure: heading case and hierarchy, one Diátaxis type, step integrity (`references/style-structure.md`).
3. Read again for prose: person, voice, tense, modal words, condition-first order (`references/style-prose.md`).
4. Spot-check formatting, code font, links, and notices only where the page uses them.
5. IF the page carries structure that matters (tables, figures, procedures) → THEN check it as a screen-reader user does: headings in order, alt text present, no direction-only instructions.
6. Stop at the first two rules a page violates repeatedly — a systemic fix beats 40 line notes.

| Severity | Meaning | Examples |
|---|---|---|
| Blocker | misleads the reader or blocks accessibility | missing alt text, "click here" as only link text, promise of an unreleased feature, unsupported performance claim, non-inclusive term |
| Major | costs comprehension or translation quality | passive instructions, future tense for current behavior, condition after instruction, Title Case headings, notice holding a required step |
| Minor | consistency | serial comma, number style, date format, code font on a product name |

Report blockers and majors with a rewrite; batch minors as one line per rule with counts. Report shape:

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

## Guide entry points

Every page sits at `https://developers.google.com/style/<slug>`; Google publishes the guide under CC BY 4.0, and these references restate it rather than copy it.

| Page | Open it for |
|---|---|
| [Google developer documentation style guide](https://developers.google.com/style) | The guide home and its own navigation |
| [Highlights](https://developers.google.com/style/highlights) | The short list of rules that cover most reviews |
| [What's new](https://developers.google.com/style/whats-new) | The changelog — read before calling a rule here stale |
| [Philosophy of this guide](https://developers.google.com/style/philosophy) | What the guide optimizes for when two rules pull apart |
| [Word list](https://developers.google.com/style/word-list) | The source of `assets/google-word-list.tsv` |
| [Google site policies](https://developers.google.com/site-policies) | The [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/) terms these references restate under |

## Check the live page

Open the owning page with whatever web tool the host provides — a fetch tool, a search tool, or `curl` — when any of these holds:

- Someone disputes a rule, or cites a rule the pack doesn't carry.
- The question falls outside the 69 pages in `assets/google-style-pages.tsv`, so no reference owns it.
- The wording carries risk: a trademark, a product name, a legal claim, a security claim, or a public API reference.
- You're about to tell someone the guide changed, or that a rule here is stale.
- A word-list entry looks wrong — run `scripts/refresh-word-list.mjs --dry-run` first, since it reads the live page for you.

Then: 1. Quote the guide's own sentence and give the page URL; a rule name alone isn't evidence. 2. IF the live page contradicts a reference → THEN the page wins: fix the reference in the same turn and say which rule moved. 3. IF the live page carries a rule no reference has → THEN add it to the owning reference, never to a new file. 4. IF the fetch fails → THEN answer from the reference and mark it "not verified against the live guide".

## Page ownership and drift

`assets/google-style-pages.tsv` maps all 69 guide pages (`slug`, `title`, `owner`, `url`); each reference also links its own pages in its Upstream line. Every URL resolved on 2026-08-18, when the newest changelog entry was July 7, 2026.

```bash
grep -P "^tables\t" assets/google-style-pages.tsv        # which reference owns a page
cut -f3 assets/google-style-pages.tsv | sort | uniq -c    # pages per reference
```

`whats-new` is the guide's own changelog; read it before arguing that a rule here is stale. The guide ships changes several times a year and has already moved rules these references depend on: temperature spacing, checkbox state wording, heading-anchor markup, code font for IP addresses and port numbers. Word-list entries keep the guide's own guidance in `assets/google-word-list.tsv`; `scripts/refresh-word-list.mjs --dry-run` reports drift, and `scripts/style-lint.mjs` reads the file so every flagged word cites the guide's wording.

Upstream: [Highlights](https://developers.google.com/style/highlights) · [Google developer documentation style guide](https://developers.google.com/style/)
