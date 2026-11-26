Load when running a style pass, ruling on voice or wording, or writing controlled English. For structure, punctuation, formatting, UI terms or a diagram, read [structure and formatting](style-layout.md); for code, claims, examples or links, read [code, claims and links](style-code-claims.md). Each linked [Google style guide](https://developers.google.com/style) page holds the full rule.

# Style guide

## Review procedure

- Precedence: [SKILL.md](../SKILL.md#writing-rules).
- Check changed text for structure, claims, steps, terminology, links, and accessibility.
- Run the [doc check](../SKILL.md#check-before-delivery) first. It reports the mechanical part of these rules: word-list terms, filler, `should` and `may`, the fixed replacements under Words, curly quotes, numeric dates and times, vague link text, placeholder and example names in code, heading case and end punctuation, the H1 count and skipped levels. Judge each hit in context. Read for the rest: voice, tense, list grammar, numbers, notices, alt text and claims.
- Review comment (Conventional Comments) per material issue: label (issue, suggestion, question, nitpick, praise), blocking or not, location, rewrite, why; in a saved report, the label and blocking flag start the `Issue` cell. Group nitpicks: "`setup.md:14`: `The file is deleted by the job` → The job deletes the file."
- Fix depends on an uncertain fact: verify with `octocode-research` first.
- Disputed, possibly changed, or sensitive public-claim rule: check the live Google page; on conflict, follow the live page and report the discrepancy for this skill's maintainer.
- Claim only the pages you checked.

## Voice and tone ([tone](https://developers.google.com/style/tone), [voice](https://developers.google.com/style/voice))

- Second person, imperative, present tense, active voice; condition or goal first.
- Required = must or imperative; recommended = "we recommend"; optional = can. Not `should`, `may`.
- No filler: `please`, `just`, `simply`, `easy`, `of course`, exclamation marks.
- No AI-writing tells: [writing tells](writing-tells.md).
- Software detects; it does not see, want, or know.
- Keep sentences focused and paragraphs short enough to scan; follow the document's wrapping convention.
- Translatable: one term per concept, keep articles and "that", no idioms, humor, or double negatives.

## Words ([word list](https://developers.google.com/style/word-list), [inclusive](https://developers.google.com/style/inclusive-documentation), [abbreviations](https://developers.google.com/style/abbreviations))

- Word in question: `node <skill-folder>/scripts/doc-check.mjs --lookup <word>` prints each matching entry of the bundled [word list](../assets/google-word-list.tsv) with its verdict and guidance; `scope: sense` means the entry bans one usage of the word, not the word. Use the matching entry in context; no entry means the list has no ruling.
- `allows you to` → lets you; `utilize`, `leverage` → use; `click on` → click; `abort`, `kill` → stop.
- Inclusive: `whitelist`/`blacklist` → allowlist/denylist; `master`/`slave` → primary/replica; `sanity check` → quick check; `guys` → everyone; generic `he` → they.
- Code-fixed terms stay in code font; prose uses the preferred word.
- Abbreviations: spell out on first use unless known (API, HTML). Never a verb: `ssh into` → use SSH to connect. `10x` → 10 times.
- Jargon: define recurring terms on first use; replace vague words (`solution`, `support`) with specifics.

## Controlled English ([ASD-STE100](https://www.asd-ste100.org/))

Apply when the request asks for controlled technical English. This is guidance only; formal compliance needs the standard and its dictionary.

- Never invent a dictionary ruling; for an unclear word, use the source's shortest word.
- One word keeps one meaning and one part of speech.
- Unpack noun clusters: "the cache key for the request," not "request cache key hash value."
- Order as separate sentences, not -ing phrases: "Stop the server. Then delete the lock."
