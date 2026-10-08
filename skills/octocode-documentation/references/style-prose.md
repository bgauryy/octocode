# Prose: voice, grammar, and global readers

Load when judging tone, person, voice, tense, modal words, sentence shape, grammar, or translation-safe and accessible wording.

## Tone

- Conversational, friendly, respectful: a knowledgeable friend, not a pedant or a hype deck.
- Cut filler: `please note`, `at this time`, `just`, `of course`, exclamation marks, jokes, pop-culture references, slang. No `please` in instructions or cross-references. Never call a task `easy`, `simple`, or `quick`.
- Vary sentence openings; don't start every sentence with "You can" or "To…". Use transitions ("Though", "This way").

## Person, voice, and tense

- Second person ("you"). `we` only for the organization as author, an FAQ question, or a signed author comment; never `let's`.
- Don't call the reader `the user`; third person is for software or other people. Name who `you` is once (developer, operator, admin) and keep it.
- Instructions are imperative ("Click **Save**"), not what you `should` do. A run of UI actions becomes a numbered procedure (`references/style-structure.md`).
- API reference: third person for facts about the element, "you" for what the reader does.
- Active voice; the doer is the subject. Passive only to de-emphasize an actor that doesn't matter, or when the object is the point.
- Present tense; cut `will` and hypothetical `would` for general behavior. Use future only for later or asynchronous events ("The backup process `will` archive the file the next time it runs").
- No anthropomorphism: software `detects`; it doesn't see, want, or know.

## Modal words

| Intent | Use | Avoid |
|---|---|---|
| Required | must, or the imperative | `should` |
| Recommended | "we recommend", "<Org> recommends" | `should` |
| Optional | can | `may` |
| Possible outcome | might, can | `may` |

- `should` only for a generally recognized recommendation ("use a strong password"). `may` belongs to policy and legal text.
- Rewrite "The value `should` be `true`" as who acts or as a check.
- Prescriptive docs: one recommended path for the most likely use case; an alternative only when the choice is real.

## Sentence and paragraph shape

- Condition, context, or goal first: "To delete the document, click **Delete**"; "For more information, see X".
- One idea per sentence and per paragraph, most important first; five or six sentences at most, one is fine. Don't lengthen sentences to shorten paragraphs.
- Don't hard-wrap prose or force line breaks inside paragraphs.

## Grammar

- Keep `a`, `an`, `the`, also in headings ("Create a VM instance"). Pick `a`/`an` by sound. Articles before product, tool, and API names: `references/style-claims.md`.
- Follow a demonstrative with a noun ("Set **this value** to `true`"). Name the noun when "it", "this", or "they" could point at two nouns.
- Keep optional relative pronouns: "the link **that** you want to open"; "the fields, **which are** described…".
- "that" restrictive (no comma); "which" nonrestrictive (comma). "who" for people; "whose" for people, animals, and things.
- Singular "they"; never `he/she`, `s/he`, or generic `he`.
- Possessives: singular nouns add `'s`, also after s ("the class's quota"); plurals ending in s add `'` ("the models' capabilities"); other plurals add `'s`.
- Company names take `'s`; product names, feature names, and trademarks never do (no "AWS's throughput").
- Code items: inflect an added noun ("the `wordCount` method's return value") or use "of". No possessive on an abbreviation with its expansion. Rewrite an awkward possessive.
- Plurals: abbreviations add `s`, or `es` after s, sh, ch, x (`OSes`). A term and its abbreviation agree in number: "virtual machines (VMs)".
- No optional plural in parentheses (`key(s)`, `child(ren)`); pick one form or write "one or more" (plural verb). "more than one" takes a singular verb.
- Units agree with the number and abbreviations don't pluralize: "1 degree", "0.5 degrees", "64 GB". Don't pluralize a code item or trademark; add a noun ("`Widget` objects").
- Use common two-word contractions; prefer negative ones (`isn't`, `don't`). No three-word contractions or ones that read as possessives.
- Emphasize a needed negative with formatting (`is <em>not</em>`); most sentences don't need it.

## Plain, translatable sentences

- Split dense sentences where the reader changes focus. Keep the subject and verb close.
- One term per concept, capitalized the same way each time.
- Keep helper words: "If the key is not found, **then**…"; "assumes **that** you have"; "all **of** the datasets"; "and **then** run the app".
- No more than two stacked noun modifiers.
- Put "only" right before what it limits: "Request only one token".
- One sense per word per document; don't reuse a word as noun and verb nearby (`once`, `while`, `as`, `since`).
- Plain verbs over phrasal ones ("use", not "make use of"), except `set up`, `log in`, `sign in`.
- Minimize negatives; never double them.
- No idioms, humor, sports, holidays, seasons, or region-specific assumptions.
- Repeat a noun when that prevents ambiguity.

## Accessible language

- No directional or sensory instructions (`above`, `the left-hand pane`, `as you can see`); use `preceding`, `following`, or the element's label. Hard-to-find element: add a screenshot.
- Name UI targets by label, never by shape or color. Color, size, or position is never the only cue.
- Device-neutral verbs: "expand the **Requirements** section", not `click the arrow`. Images and alt text: `references/style-format.md`.
- Captions, transcripts, or descriptions for every audio and video asset. No flashing elements.
- Left-align body text. Avoid all caps and camel case in prose; use exclamation marks, question marks, and semicolons sparingly.
- Semantic structure: ordered headings, table headers, keyboard-reachable content, error text that says what went wrong and how to fix it.

Source: [Voice and tone](https://developers.google.com/style/tone).
