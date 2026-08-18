# API Reference Text

Load when writing or reviewing docstrings, generated reference pages, or parameter tables.

## Coverage

Document every class, interface, and struct; every constant, field, enum, and typedef; and every method with each parameter, the return value, and any exception thrown. Put a short code sample (about 5-20 lines) at the top of each class or interface page.

## Summaries

- Third-person singular present tense, verb first: "Creates a task on the specified task list." Not "Create a task", not "This method will create".
- Describe what the item does, not what a developer might use it for, and don't repeat the item's name.
- Class and type summaries are noun phrases: "A primary toolbar within the activity."
- Openers by kind: "Gets the…", "Sets the…", "Updates the…", "Deletes the…", "Registers…", "Creates a…" (convenience constructors), "Checks whether…" (boolean getters), "Called by… when…" (callbacks).
- Keep members (constants, fields) as short as possible and link the methods that use them, with a "See also:" pointer where it helps.
- No period before the real end of the first sentence and no abbreviations like `e.g.` — generators truncate the short description at the first period.
- IF a class name is also a common word → THEN you can refer to it in lowercase, non-code prose ("activities", "the action bar").

## Parameters, returns, exceptions

| Element | Pattern |
|---|---|
| Non-boolean parameter | "The name of the bucket." |
| Boolean parameter (behavior) | "If true, validates the certificate. If false, trusts it without validating." |
| Boolean parameter or return (state) | "True if the list is sorted; false otherwise." |
| Default | "Default: 10." — explain the behavior for each value or range first |
| Non-boolean return | "The generated task ID." |
| Exception | "If the list doesn't exist." when the generator adds "Throws", otherwise "Thrown when…" |

- In parameter and return descriptions, `true` and `false` are plain words: no code font, no quotation marks, capital "True" at the start of a sentence.
- String literals do take code font plus double quotation marks: `"wrap_content"`.
- Capitalize the first word and end with a period, even for fragments.
- Document dependencies the call needs — a permission, an enabled API — and what happens without them ("the method throws a `SecurityException`").
- Deprecations lead with the replacement: "Deprecated. Use `listTasks` instead." Then say why, how to migrate, and which version deprecated it.

## Mechanics

- Link the first mention of a related class or method instead of describing it twice.
- Keep identifiers in code font and don't inflect them (`references/style-code.md`).
- Keep parameter names and order identical to the signature.

Next: code font rules → `references/style-code.md`; verb and tense rules → `references/style-voice.md`.
