# Inclusive Terminology

Load when a term may exclude, stereotype, or read as violent — and before touching a replacement table row.

## The root rule

Most of this is one principle: drop idiomatic, figurative, and metaphorical language and use literal, precise terms in their primary sense. Figurative phrasing is what turns ableist, violent, or graphic. Don't build documentation on a metaphor — no "pets versus cattle".

## Replace, but rewrite verbs

| Don't use | Use instead |
|---|---|
| `blacklist` | denylist, excludelist, blocklist |
| `whitelist` | allowlist, trustlist, safelist |
| `graylist` | provisional list |
| `master` with `slave` | primary/secondary, primary/replica, controller/worker, leader/follower, active/standby — and never the pair `master`/`slave` in any context |
| `sanity check` | quick check, confidence check, preliminary check, coherence check |
| `dummy value` | placeholder |
| `dumb down` | simplify, remove technical jargon |
| `crazy`, `insane`, `lunatic`, `bonkers` | complicated, complex, baffling, unexpected — and only for inanimate things |
| `blind to`, `blind write`, `blind change` | unaware of; a write without a read; change without confirming the value |
| `cripple` | slow down, degrade |
| `man hours`, `manpower`, `mankind` | person-hours, staff or workforce, humanity |
| `guys`, `you guys` | everyone, folks |
| `he/she` generically | singular "they" |
| `grandfathered` | legacy, exempt |
| `ninja`, `guru`, `rockstar` | expert |
| `mom test`, `grandmother test`, `grandma test`, `girlfriend test` | beginner user test, novice user test |
| `female adapter`, `male adapter` | socket, plug |
| `STONITH` and other graphic terms | the literal action ("fence failed nodes") |

- Check that a replacement is technically accurate for your context — and that a list is even involved.
- Don't swap a non-inclusive **verb** for an inclusive one; rewrite the sentence. "You can allow requests from a range of IP addresses", not "You can allowlist a range".
- IF replacing an established term would confuse readers → THEN name it once in parentheses and use the replacement after: "add them to an allowlist (sometimes called a `whitelist`)".
- IF the term is fixed in code, a flag, or an API → THEN keep it in code font, use it as little as possible, and use the preferred term in prose.
- IF a graphic term must appear → THEN mention it once and phrase the rest to de-emphasize it.

## Disability, age, and people

- "person with disabilities", or the community's identity-first term (Deaf, autistic, blind). Never "the disabled" or "a quadriplegic" — say "people with disabilities", "a quadriplegic person".
- Don't call people without disabilities `normal` or `healthy`; use nondisabled, sighted, hearing, or neurotypical person.
- No euphemisms: not `physically challenged`, `special`, `differently abled`, or `handi-capable`. No "suffers from", "victim of", "wheelchair-bound".
- "older adults", not "seniors" or cute phrasing; "aging population" works for the group.
- Avoid framing people as "native speakers" versus "non-native speakers".

Next: translation-safe sentence shape → `references/style-global.md`; the full word list → `references/style-words.md`.
