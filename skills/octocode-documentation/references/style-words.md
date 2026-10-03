# Word choice, abbreviations, and inclusive terms

Load when a specific word is in question, when introducing a short form or a specialist term, or when a term might exclude, stereotype, or read as violent — and before touching a replacement table row. The guide's word list decides, not preference. `assets/google-word-list.tsv` carries all 597 entries as `term`, `verdict` (`dont-use`, `avoid`, `caution`, `usage`), and the guide's own guidance. Look the term up, then quote it:

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
| `legacy`, `anti-pattern` | plain description, or define on first use |
| `blast radius`; `shifting left` | affected area; moving earlier in the process — or define on first use |
| `off-the-shelf`; `back-of-the-envelope`; `cold standby` | ready-made or prebuilt; informal estimate; backup system |
| `foo`, `bar`, `baz` | meaningful placeholder names | <!-- style-lint: ignore-line metasyntactic-name -->
| `doc`, `repo`, `k8s`, `cell phone`, `mobile` (alone) | documentation, repository, Kubernetes, mobile device |
| `and/or` | "or", or "A, B, or both" — acceptable only where space is tight |
| `postmortem` | retrospective |
| `as of this writing` | delete it |

`ingest` is conditional: use import, load, or copy for plain data movement, and `ingest` only when the step does significant processing. Keep these spellings: `on-premises`, `OAuth 2.0`, plugin (noun), plug-in (adjective), plug in (verb), allowlist and denylist as nouns.

## Abbreviations

- Spell out on first use with the abbreviation in parentheses, and italicize both: "*Border Gateway Protocol* (*BGP*)". Lowercase the spelled-out form unless it's a proper noun: "data manipulation language (DML)", not "Data Manipulation Language (DML)".
- Skip the expansion for terms the audience already knows (API, HTML, PDF, AI).
- IF the first mention falls in a heading → THEN use the abbreviation there and spell it out in the first paragraph that follows.
- IF you use the abbreviation only once → THEN include it only if it's as familiar as the spelled-out term; otherwise leave it out.
- Don't abbreviate terms unrelated to the document's topic — spell out "low Earth orbit" instead of introducing `LEO`.
- Acronyms take no periods (API, NASA); shortened words do (Dr.) — except date and time abbreviations, country abbreviations, US state abbreviations (DC), and shortenings read as words (app, sync, demo).
- Never use an abbreviation as a verb: "use SSH to connect", not "ssh into". Choose "a" or "an" by how the abbreviation sounds aloud ("a SQL query", "an SAP system").
- Spell out symbol substitutions: "10 times faster", not "10x faster"; "approximately", not "approx.".
- No internet slang: no `tl;dr`, `ymmv`, `RTFM`. Write what you mean, literally.
- Put the abbreviation inside the link text with its long form (`references/style-claims.md`).

## Jargon

- First choice: write around the term. Second: replace it with specific language (the plain-language swaps the guide names are in § Replace on sight).
- IF the term appears once → THEN describe it in plain language with the term in parentheses, or link a trusted definition. IF the term recurs throughout → THEN describe it briefly in parentheses on first reference.
- Jargon is worth keeping when readers search for it — SEO is a legitimate reason, a definition is still required.
- Vague, overloaded words count as jargon too: `solution`, `support`, `workload`. Say which one you mean.
- Jargon that is a code item stays in code font (`references/style-code.md`).

## Inclusive terms

Most of this is one principle: drop idiomatic, figurative, and metaphorical language; use literal, precise terms in their primary sense. Figurative phrasing is what turns ableist, violent, or graphic. Don't build documentation on a metaphor — no "pets versus cattle".

| Don't use | Use instead |
|---|---|
| `blacklist`; `whitelist`; `graylist` | denylist, excludelist, blocklist; allowlist, trustlist, safelist; provisional list |
| `master` with `slave` | primary/secondary, primary/replica, controller/worker, leader/follower, active/standby — and never the pair `master`/`slave` in any context |
| `sanity check` | quick check, confidence check, preliminary check, coherence check |
| `dummy value`; `dumb down` | placeholder; simplify, remove technical jargon |
| `crazy`, `insane`, `lunatic`, `bonkers` | complicated, complex, baffling, unexpected — and only for inanimate things |
| `blind to`, `blind write`, `blind change` | unaware of; a write without a read; change without confirming the value |
| `cripple` | slow down, degrade |
| `man hours`, `manpower`, `mankind` | person-hours; staff or workforce; humanity |
| `guys`, `you guys`; `he/she` generically | everyone, folks; singular "they" |
| `grandfathered`; `ninja`, `guru`, `rockstar` | legacy, exempt; expert |
| `mom test`, `grandmother test`, `grandma test`, `girlfriend test` | beginner user test, novice user test |
| `female adapter`, `male adapter` | socket, plug |
| `STONITH` and other graphic terms | the literal action ("fence failed nodes") |

- Check that a replacement is technically accurate for your context — and that a list is even involved.
- Don't swap a non-inclusive **verb** for an inclusive one; rewrite the sentence. "You can allow requests from a range of IP addresses", not "You can allowlist a range".
- IF replacing an established term risks confusing readers → THEN name it once in parentheses and use the replacement after: "add them to an allowlist (sometimes called a `whitelist`)".
- IF code, a flag, or an API fixes the term → THEN keep it in code font, use it as little as possible, and use the preferred term in prose. IF a graphic term must appear → THEN mention it once and phrase the rest to de-emphasize it.
- "person with disabilities", or the community's identity-first term (Deaf, autistic, blind). Never "the disabled" or "a quadriplegic" — say "people with disabilities", "a quadriplegic person". Don't call people without disabilities `normal` or `healthy`; use nondisabled, sighted, hearing, or neurotypical person.
- No euphemisms: not `physically challenged`, `special`, `differently abled`, or `handi-capable`. No "suffers from", "victim of", "wheelchair-bound".
- "older adults", not "seniors" or cute phrasing; "aging population" works for the group. Avoid framing people as "native speakers" versus "non-native speakers".

Upstream: [Word list](https://developers.google.com/style/word-list) · [Abbreviations](https://developers.google.com/style/abbreviations) · [Jargon](https://developers.google.com/style/jargon) · [Inclusive language](https://developers.google.com/style/inclusive-documentation). Verify a disputed or missing rule against the live page → `references/style-pass.md`.
