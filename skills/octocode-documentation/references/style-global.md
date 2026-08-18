# Global And Accessible Prose

Load when text must survive translation and reach readers of every ability.

## Plain, translatable sentences

- Short sentences — aim under 26 words. Simple words, standard structure, subject and verb near the start.
- One term per concept, capitalized the same way every time; synonym variety hurts comprehension and machine translation.
- Keep helper words that conversational English drops: "If the key is not found, **then** the default is returned"; "assumes **that** you have"; "Identify all **of** the datasets"; "Start the profiler, and **then** run the app."
- Keep relative pronouns: "update the rules **that** you previously defined".
- Don't stack more than two nouns as modifiers: "a cloud-native DevSecOps pipeline in a hybrid environment", not "a hybrid cloud-native DevSecOps pipeline".
- Put "only" immediately before what it limits: "Request only one token", not "Only request one token".
- Use a word in one sense per document, and don't use the same word as noun and verb nearby (`once`, `while`, `as`, `since`).
- Plain verbs over phrasal ones ("use", not "make use of") — except established forms like `set up`, `log in`, `sign in`.
- Minimize negatives and never double them: "You can continue without a path", not "A missing path won't prevent you from continuing".
- No idioms, slang, humor, sports or holiday references, seasons, or region-specific assumptions.
- Define abbreviations, keep pronoun antecedents explicit, and repeat a noun when repetition prevents ambiguity.
- Condition before instruction, unambiguous dates (`references/style-numbers.md`), and diverse example values (`references/style-examples.md`).

## Accessible language

- No directional or sensory instructions: replace "above", "below", "the left-hand pane", "as you can see" with "preceding", "the following", or the element's label. IF an element is genuinely hard to find → THEN provide a screenshot.
- Name UI targets by label, never by shape or color. Color, size, and position must never be the only cue — add a text label or another secondary cue.
- Device-neutral verbs where possible: "expand the **Requirements** section", not "click the arrow".
- Link text must make sense out of context (`references/style-links.md`).
- Don't force line breaks inside sentences or paragraphs; they break in resized windows and at larger text sizes.
- Never carry information only in an image, and never use an image of text, code, or terminal output (`references/style-images.md`).
- Provide captions, transcripts, or descriptions for audio and video. No flickering or flashing elements — they can trigger motion sickness or seizures.
- Left-align body text; don't center or justify it. Avoid all caps and camel case in prose, and go easy on exclamation marks, question marks, and semicolons — screen readers handle them inconsistently.
- Semantic structure: real headings in order, real table headers, keyboard-reachable content, and error text that says what went wrong and how to fix it.
- Test with a screen reader when the page carries structure that matters.

Next: inclusive terminology → `references/style-inclusive.md`; word-level swaps → `references/style-words.md`.
