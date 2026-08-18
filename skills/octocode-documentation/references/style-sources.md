# Google style sources

Load when someone disputes a rule, when the pack doesn't carry the rule you need, or to check whether the guide has moved. Why: these references are a snapshot, and the live guide is the authority. Every page sits at `https://developers.google.com/style/<slug>`; Google publishes the guide under CC BY 4.0, and these references restate it rather than copy it.

## Guide entry points

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

Then:

1. Quote the guide's own sentence and give the page URL. A rule name alone isn't evidence.
2. IF the live page contradicts a reference → THEN the page wins: fix the reference in the same turn and say which rule moved.
3. IF the live page carries a rule no reference has → THEN add it to the owning reference, never to a new file.
4. IF the fetch fails → THEN answer from the reference and mark it "not verified against the live guide".

## Page ownership

`assets/google-style-pages.tsv` maps all 69 guide pages (`slug`, `title`, `owner`, `url`); each reference also links its own pages in its Upstream line. Every URL resolved on 2026-08-18, when the newest changelog entry was July 7, 2026.

```bash
grep -P "^tables\t" assets/google-style-pages.tsv        # which reference owns a page
cut -f3 assets/google-style-pages.tsv | sort | uniq -c    # pages per reference
```

## Drift check

`whats-new` is the guide's own changelog; read it before arguing that a rule here is stale. The guide ships changes several times a year and has already moved rules these references depend on: temperature spacing, checkbox state wording, heading-anchor markup, code font for IP addresses and port numbers. Word-list entries keep the guide's own guidance in `assets/google-word-list.tsv`; `scripts/refresh-word-list.mjs --dry-run` reports drift, and `scripts/style-lint.mjs` reads the file so every flagged word cites the guide's wording.

Next: back to the topic map → `references/style-index.md`.
