# Prose: voice, grammar, and global readers

Load when judging prose (tone, person, voice, tense, modal words, sentence and paragraph shape), for article, pronoun, possessive, plural, preposition, and contraction questions, or when text must survive translation and reach readers of every ability.

## Tone

- Conversational, friendly, respectful — a knowledgeable friend, never a pedant, never a hype deck. Let some personality through; flat prose is not the goal.
- Cut filler and noise: `please note`, `at this time`, `just`, `of course`, exclamation marks, jokes, pop-culture references, and internet slang (`tl;dr`, `ymmv`). Drop `please` from instructions and cross-references — "Click **View**" needs no polite prefix.
- Never claim the task is `easy`, `simple`, or `quick` — the reader who is stuck disagrees.
- Vary sentence openings; don't start every sentence with "You can" or "To…". Use transitions ("Though", "This way") so paragraphs don't read as a list. Read the sentence aloud; if it sounds stilted or arch, rewrite it.

## Person, voice, and tense

- Second person: "you", "your". Use `we` only for the organization as author, or in an FAQ question and a signed document where the author comments; never `let's`.
- Don't call the reader `the user`. Third person is for software or for other people ("end users see a consent screen"). Name who `you` is once, then keep it stable across the page: developer, operator, or admin.
- Instructions use the imperative — "Click **Save**" — not a description of what you `should` do. IF imperative prose becomes a sequence of UI actions → THEN convert it to a numbered procedure (`references/style-structure.md`).
- API reference: third person for facts about a programming element, "you" for what the reader does with it — both can appear on one page.
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

- Use `should` only for a generally recognized recommendation: "You `should` use a strong password." Elsewhere it is ambiguous. `may` belongs to policy and legal text; for possibility use `might`, for permission use `can`.
- "The value `should` be `true`" is always ambiguous. Rewrite as who acts ("You must set the value to `true`", "The server sets the value to `true`") or as a check ("If the value is `false`, do the following").
- Prescriptive docs state one purpose and follow it through headings, examples, and sample commands: one recommended path, the most likely use case, an alternative only when the choice is real ("you can also…").

## Sentence and paragraph shape

- Condition, context, or goal first: "To delete the document, click **Delete**" beats "Click **Delete** if you want to delete the document"; "For more information, see X", not "See X for more information".
- One idea per sentence, one idea per paragraph, most important information first in both. Past five or six sentences a paragraph usually carries too much — but never lengthen sentences to shorten a paragraph, and a one-sentence paragraph is fine.
- Don't hard-wrap prose or force line breaks inside sentences or paragraphs; they break on small screens, in resized windows, and at larger text sizes.

## Grammar

- Keep `a`, `an`, and `the` — including in headings and titles: "Create a VM instance", not "Create VM instance". Dropping articles hurts comprehension and translation. Choose `a` or `an` by the following word's sound, not its letter; `references/style-claims.md` owns articles before product, tool, and API names.
- Every pronoun needs an unambiguous antecedent. IF "it", "this", or "they" might point at two nouns → THEN name the noun. Follow a demonstrative with a noun even when only one candidate exists: "Set **this value** to `true`", not "Set this to `true`".
- Keep optional relative pronouns — they aid clarity, not only ambiguity: "Right-click the link **that** you want to open"; "the fields, **which are** described in the following section"; "update the rules **that** you previously defined".
- "that" introduces a restrictive clause (no comma); "which" introduces a nonrestrictive one (comma). For people you can use "who"; "whose" works for people, animals, and things.
- Singular "they" is the gender-neutral pronoun; never `he/she`, `s/he`, or a generic `he`. First person only in an FAQ question or where a document's author comments.
- Possessives: singular nouns, including those ending in s, add `'s` ("the class's quota"); plural nouns ending in s take an apostrophe only ("the models' capabilities"); plurals not ending in s take `'s` ("the children's records").
- Company names take `'s` ("Google's office"). Product names, feature names, and trademarks never do — regardless of who owns them, so no "AWS's throughput".
- Code items: add a noun after the identifier and inflect that noun — "the `wordCount` method's return value" — or rewrite with "of". Don't form a possessive from an abbreviation paired with its expansion: "the rule that the Federal Trade Commission (FTC) issued". IF the possessive reads awkwardly → THEN rewrite the sentence.
- Plurals: standard US English; never `'s` for a plural. Abbreviations add `s`, or `es` after s, sh, ch, x (`OSes`). A spelled-out term and its abbreviation agree in number: "virtual machines (VMs)".
- Never park an optional plural in parentheses: no `your API key(s)`, no `the child(ren)`. Pick one form, or write "one or more" when both genuinely matter. "one or more" takes a plural verb ("if one or more tests fail"); "more than one" takes a singular ("you can create more than one instance").
- Units agree with the number and abbreviations don't pluralize: "1 degree", "0.5 degrees", "64 GB" — never "64 GBs". Don't pluralize a code item or a trademark; add a plural noun instead ("`Widget` objects").
- Match the verb to the real subject: "The efficiency of algorithms that process data sets depends on memory allocation."
- Ending a sentence with a preposition is fine when it reads better ("the language you're working with"); include the prepositions that add clarity and cut the ones that don't. UI prepositions: "in" a dialog, field, menu, window; "on" a page, tab, toolbar (`references/style-format.md`).
- Common two-word contractions are welcome; prefer negative contractions (`isn't`, `can't`, `don't`) because the negative is harder to miss. No nonstandard or three-word contractions ("mightn't've"), and none that can read as a possessive.
- IF a negative needs emphasis → THEN spell it out with formatting (`is <em>not</em>`), but most sentences don't need it.

## Plain, translatable sentences

- Short sentences — aim under 26 words. Plain words, standard structure, subject near the start, verb close behind.
- One term per concept, capitalized the same way every time; synonym variety hurts comprehension and machine translation.
- Keep helper words that conversational English drops: "If the key is not found, **then** the service returns the default"; "assumes **that** you have"; "Identify all **of** the datasets"; "Start the profiler, and **then** run the app."
- Don't stack more than two nouns as modifiers: "a cloud-native DevSecOps pipeline in a hybrid environment", not "a hybrid cloud-native DevSecOps pipeline".
- Put "only" immediately before what it limits: "Request only one token", not "Only request one token".
- Use a word in one sense per document, and don't use the same word as noun and verb nearby (`once`, `while`, `as`, `since`).
- Plain verbs over phrasal ones ("use", not "make use of") — except established forms like `set up`, `log in`, `sign in`.
- Minimize negatives and never double them: "You can continue without a path", not "A missing path `won't` prevent you from continuing".
- No idioms, slang, humor, sports references, holiday references, seasons, or region-specific assumptions.
- Define abbreviations, keep pronoun antecedents explicit, and repeat a noun when repetition prevents ambiguity. Use unambiguous dates (`references/style-punctuation.md`) and diverse example values (`references/style-claims.md`).

## Accessible language

- No directional or sensory instructions: replace `above`, `below`, `the left-hand pane`, `as you can see` with `preceding`, `the following`, or the element's label. IF an element is genuinely hard to find → THEN provide a screenshot.
- Name UI targets by label — never by shape, never by color. Color, size, and position must never be the only cue; add a text label or another secondary cue.
- Device-neutral verbs where possible: "expand the **Requirements** section", not `click the arrow`. Link text must make sense out of context (`references/style-claims.md`).
- Never carry information only in an image, and never use an image of text, code, or terminal output (`references/style-format.md`).
- Provide captions, transcripts, or descriptions for every audio and video asset. No flickering or flashing elements — they can trigger motion sickness or seizures.
- Left-align body text; don't center or justify it. Avoid all caps and camel case in prose, and use exclamation marks, question marks, and semicolons sparingly — screen readers handle them inconsistently.
- Semantic structure: real headings in order, real table headers, keyboard-reachable content, and error text that says what went wrong and how to fix it. Test with a screen reader when the page carries structure that matters.

Upstream: [Voice and tone](https://developers.google.com/style/tone) · [Second person](https://developers.google.com/style/person) · [Active voice](https://developers.google.com/style/voice) · [Present tense](https://developers.google.com/style/tense) · [Anthropomorphism](https://developers.google.com/style/anthropomorphism) · [Prescriptive documentation](https://developers.google.com/style/prescriptive-documentation) · [Sentence structure](https://developers.google.com/style/sentence-structure) · [Paragraphs](https://developers.google.com/style/paragraph-structure) · [Articles (a, an, the)](https://developers.google.com/style/articles) · [Pronouns](https://developers.google.com/style/pronouns) · [Possessives](https://developers.google.com/style/possessives) · [Pluralization](https://developers.google.com/style/pluralization) · [Prepositions](https://developers.google.com/style/prepositions) · [Contractions](https://developers.google.com/style/contractions) · [Global audience](https://developers.google.com/style/translation) · [Accessibility](https://developers.google.com/style/accessibility). Verify a disputed or missing rule against the live page → `references/style-pass.md`.
