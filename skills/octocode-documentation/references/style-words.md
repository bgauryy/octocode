# Word choice, abbreviations, and inclusive terms

Load when a specific word, short form, specialist term, or possibly exclusive term is in question. The guide's word list decides, not preference: `assets/google-word-list.tsv` holds all 599 entries as `term`, `verdict` (`dont-use`, `avoid`, `caution`, `usage`), and guidance. Look the term up, then quote it:

```bash
grep -iP "^[^\t]*allows you to" assets/google-word-list.tsv
```

IF a word isn't in the list → THEN use Merriam-Webster's first spelling (`canceled`); for a technical term, use that technology's own docs.

## Replace on sight

The following examples help when no project convention decides the wording:

| Don't use | Use instead |
|---|---|
| `allows you to` | lets you |
| `via`, `leverage`, `utilize` | with, through, use |
| `just`, `simply`, `easy` | delete (`just` is fine in "or just example-kind") |
| `careful`, `carefully` | name the exact check |
| `proper`, `properly` | name the technical state |
| `please note`, `note that` | state the fact |
| `click on` | click; hyphenate `right-click`, `double-click`; Android uses tap |
| `abort`, `terminate`, `kill` | stop, exit, cancel, end |
| `hit` (a button); `deselect` | click; clear |
| `this article`, `this page`, `this topic` | this document |
| `first-class citizen` | the actual capability |
| `legacy` | plain description, or define on first use |
| `off-the-shelf`; `back-of-the-envelope`; `cold standby` | prebuilt; informal estimate; backup system |
| `doc` (alone) | documentation |

- `ingest`: use import, load, or copy for plain data movement; `ingest` only when the step does significant processing.
- Keep: `on-premises`, `OAuth 2.0`, plugin (noun), plug-in (adjective), plug in (verb), allowlist and denylist as nouns.

## Abbreviations

- First use: spell out, abbreviation in parentheses, both italic: "*Border Gateway Protocol* (*BGP*)". Lowercase the long form unless it is a proper noun.
- Skip the expansion for terms the audience knows (API, HTML, PDF, AI).
- First mention in a heading: define it there only when worth the length; else use the abbreviation only if it is the better-known form, and spell it out in the next paragraph.
- Used only once: include the abbreviation only if it is as familiar as the long form.
- Don't abbreviate terms outside the topic ("low Earth orbit", not `LEO`).
- Acronyms take no periods (API); shortened words do (Dr.), except date, time, country, and US state abbreviations (DC) and words such as app, sync, demo.
- Never use an abbreviation as a verb: "use SSH to connect", not "ssh into". Pick "a" or "an" by sound ("a SQL query", "an SAP system").
- Spell out symbol substitutions: "10 times faster", not "10x"; "approximately", not "approx.".

## Jargon

- First write around the term; else replace it with specific language (§ Replace on sight).
- Term used once: describe it in plain words with the term in parentheses, or link a definition. Recurring term: define it briefly in parentheses on first use.
- Keep jargon readers search for, but still define it.
- Vague words count as jargon: `solution`, `support`, `workload`. Say which one you mean.

## Inclusive terms

Use literal terms in their primary sense. Drop idioms, figures, and metaphors; they turn ableist, violent, or graphic. Don't build docs on a metaphor ("pets versus cattle").

| Don't use | Use instead |
|---|---|
| `blacklist`; `whitelist`; `graylist` | denylist, excludelist, blocklist; allowlist, trustlist, safelist; provisional list |
| `master` with `slave` | primary/secondary, primary/replica, controller/worker, leader/follower, active/standby; never the pair `master`/`slave` |
| `sanity check` | quick check, confidence check, preliminary check, coherence check |
| `dummy value`; `dumb down` | placeholder; simplify |
| `crazy`, `insane`, `lunatic`, `bonkers` | complicated, complex, baffling, unexpected (inanimate things only) |
| `blind to`, `blind write`, `blind change` | unaware of; a write without a read; change without confirming the value |
| `cripple` | slow down, degrade |
| `man hours`, `manpower`, `mankind` | person-hours; staff or workforce; humanity |
| `guys`, `you guys`; generic `he/she` | everyone, folks; singular "they" |
| `grandfathered`; `ninja`, `guru`, `rockstar` | legacy, exempt; expert |
| `mom test`, `grandma test`, `girlfriend test` | beginner user test, novice user test |
| `female adapter`, `male adapter` | socket, plug |
| `STONITH` and other graphic terms | the literal action ("fence failed nodes") |

- Check that the replacement is accurate in context.
- Don't swap a non-inclusive verb for an inclusive verb; rewrite: "You can allow requests from a range of IP addresses".
- Established term at risk of confusion: name it once in parentheses, then use the replacement ("an allowlist (sometimes called a `whitelist`)").
- Term fixed by code, a flag, or an API: keep it in code font, use it rarely, use the preferred term in prose. A required graphic term: mention it once, then de-emphasize.
- "person with disabilities", or the community's identity-first term (Deaf, autistic). Never "the disabled". Don't call others `normal` or `healthy`; use nondisabled, sighted, hearing, neurotypical.
- No euphemisms (`physically challenged`, `special`, `differently abled`, `handi-capable`) and no "suffers from", "victim of", "wheelchair-bound".
- "older adults" or "aging population", not "seniors". Avoid "native speaker" versus "non-native speaker".

Source: [Word list](https://developers.google.com/style/word-list).
