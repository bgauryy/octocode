# Voice and tone

Load when judging prose: tone, person, voice, tense, modal words, sentence shape, and paragraph shape.

## Tone

- Conversational, friendly, respectful — a knowledgeable friend, never a pedant, never a hype deck. Let some personality through; flat prose is not the goal.
- Cut filler and noise: `please note`, `at this time`, `just`, `of course`, exclamation marks, jokes, pop-culture references, and internet slang (`tl;dr`, `ymmv`). Drop `please` from instructions and cross-references — "Click **View**" needs no polite prefix.
- Never claim the task is `easy`, `simple`, or `quick` — the reader who is stuck disagrees.
- Vary sentence openings; don't start every sentence with "You can" or "To…". Use transitions ("Though", "This way") so paragraphs don't read as a list. Read the sentence aloud; if it sounds stilted or arch, rewrite it.

## Person

- Second person: "you", "your". Use `we` only for the organization as author, or in an FAQ question and a signed document where the author comments; never `let's`.
- Don't call the reader `the user`. Third person is for software or for other people ("end users see a consent screen").
- Name who `you` is once, then keep it stable across the page: developer, operator, or admin.
- Instructions use the imperative — "Click **Save**" — not a description of what you `should` do. IF imperative prose becomes a sequence of UI actions → THEN convert it to a numbered procedure (`references/style-procedures.md`).
- API reference: third person for facts about a programming element, "you" for what the reader does with it — both can appear on one page.

## Voice and tense

- Active voice; make the doer the subject. Zombie heuristic (not a guide rule): if "by zombies" fits after the verb, it's passive.
- Passive is acceptable to de-emphasize the actor ("Over 50 conflicts were found"), when readers don't need to know who acted, or when the object is the point. You can name the actor with "by", but the prose is usually weaker.
- Present tense. Cut `will` and hypothetical `would` from general behavior: "the server removes you from the list", not "the server `would` then remove you".
- Future tense is right for genuinely later or asynchronous events: "The backup process `will` archive the file the next time it runs"; "The service `will` notify any Pub/Sub subscribers."
- No anthropomorphism: software `detects` or `specifies`; it does not see, tell, want, or know. It is figurative language, which translates badly.

## Modal words

| Intent | Use | Avoid |
|---|---|---|
| Required | must, or the imperative | `should` |
| Recommended | "we recommend", "<Org> recommends" | `should` |
| Optional | can | `may` |
| Possible outcome | might, can | `may` |

- Use `should` only for a generally recognized recommendation: "You `should` use a strong password." Elsewhere it is ambiguous.
- `may` belongs to policy and legal text; for possibility use `might`, for permission use `can`.
- "The value `should` be `true`" is always ambiguous. Rewrite as who acts ("You must set the value to `true`", "The server sets the value to `true`") or as a check ("If the value is `false`, do the following").
- Prescriptive docs state one purpose and follow it through headings, examples, and sample commands: one recommended path, the most likely use case, an alternative only when the choice is real ("you can also…").

## Sentence and paragraph shape

- Condition, context, or goal first: "To delete the document, click **Delete**" beats "Click **Delete** if you want to delete the document"; "For more information, see X", not "See X for more information".
- One idea per sentence, one idea per paragraph, most important information first in both. Past five or six sentences a paragraph usually carries too much — but never lengthen sentences to shorten a paragraph, and a one-sentence paragraph is fine.
- Don't hard-wrap prose with forced line breaks; they break on small screens and at larger text sizes.

Upstream: [Voice and tone](https://developers.google.com/style/tone) · [Second person](https://developers.google.com/style/person) · [Active voice](https://developers.google.com/style/voice) · [Present tense](https://developers.google.com/style/tense) · [Anthropomorphism](https://developers.google.com/style/anthropomorphism) · [Prescriptive documentation](https://developers.google.com/style/prescriptive-documentation) · [Sentence structure](https://developers.google.com/style/sentence-structure) · [Paragraphs](https://developers.google.com/style/paragraph-structure). Verify a disputed or missing rule against the live page → `references/style-sources.md`.

Next: articles, pronouns, possessives, plurals → `references/style-grammar.md`; word-level swaps → `references/style-words.md`; translation and inclusion → `references/style-global.md`.
