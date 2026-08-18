# Word choice

Load when a specific word is in question — the guide's word list decides, not preference. `assets/google-word-list.tsv` carries all 597 entries as `term`, `verdict` (`dont-use`, `avoid`, `caution`, `usage`), and the guide's own guidance. Look the term up, then quote it:

```bash
grep -iP "^[^\t]*allows you to" assets/google-word-list.tsv
node scripts/style-lint.mjs docs/ --only word-list        # dont-use and avoid terms in prose
node scripts/refresh-word-list.mjs --dry-run              # when an entry looks stale
```

IF a word isn't in the list → THEN follow Merriam-Webster's first listed spelling (`canceled`, not `cancelled`); for a technical term, follow the authoritative documentation for that technology.

## Replace on sight

| Don't use | Use instead |
|---|---|
| `allows you to` | lets you |
| `e.g.`, `i.e.` | for example, that is |
| `via`, `leverage`, `utilize` | with, through, use |
| `just`, `simply`, `easy` | delete the word — though `just` is fine in a phrase like `or just example-kind` |
| `etc.`, `and so on` | finish the list or use "such as"; `etc.` is acceptable in a tight list |
| `currently`, `now`, `new`, `soon`, `latest`, `recently`, `eventually` | delete it, or name the release version and date |
| `please note`, `note that` | state the fact |
| `click here`, `read this document` | descriptive link text |
| `click on` | click — and hyphenate `right-click`, `double-click`; Android uses tap |
| `hover` | hold the pointer over |
| `check` (a checkbox) | select — and `deselect` is clear |
| `above`, `below` | earlier, preceding, later, following; for versions use `later`, `earlier` |
| `abort`, `terminate`, `kill` | stop, exit, cancel, end |
| `hang` | stops responding |
| `hit` (a button) | click |
| `we`, `our`, `us` (addressing the reader) | you |
| `this article`, `this page`, `this topic` | this document |
| `account name` | username |
| `disable` (for something broken) | inactive, unavailable, deactivate |
| `native` (feature) | built-in |
| `first-class citizen` | name the actual capability |
| `allowlist`, `denylist` as verbs | rewrite the sentence ("allow requests from…") |
| `legacy`, `anti-pattern`, `shift left`, `blast radius` | plain description, or define on first use |
| `foo`, `bar`, `baz` | meaningful placeholder names | <!-- style-lint: ignore-line metasyntactic-name -->
| `doc`, `repo`, `k8s`, `cell phone`, `mobile` (alone) | documentation, repository, Kubernetes, mobile device |
| `and/or` | "or", or "A, B, or both" — acceptable only where space is tight |
| `postmortem` | retrospective |
| `as of this writing` | delete it |

`ingest` is conditional: use import, load, or copy for plain data movement, and `ingest` only when the step does significant processing. Keep these spellings: `on-premises`, `OAuth 2.0`, plugin (noun), plug-in (adjective), plug in (verb), allowlist and denylist as nouns.

Upstream: [Word list](https://developers.google.com/style/word-list). Verify a disputed or missing rule against the live page → `references/style-sources.md`.

Next: abbreviations and jargon → `references/style-abbreviations.md`; inclusive terms → `references/style-inclusive.md`.
