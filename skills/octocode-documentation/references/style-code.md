# Code, commands, and API reference

Load when deciding what gets code font and how code appears in prose or a sample, when documenting a command line (its syntax, placeholders, or output), or when writing or reviewing docstrings, generated reference pages, or parameter tables.

## Code font

Code font covers:
- Attribute names and values; class, method, and function names; language keywords; namespaces; package names. Command output; text the reader types; strings used in commands; placeholders; query parameters.
- Data types; defined constants; element and enum names; environment variables. Database elements: row names and column names. Filenames, extensions, paths, and folders.
- DNS record types; HTTP verbs, status codes, and content types; IAM roles; IP addresses; port numbers. UI values rendered from what the reader entered earlier — an instance name, a server name.

Not code font: product, service, and organization names, domain names, and URLs the reader opens in a browser. Conditional cases:

| Case | Code font when | Plain when |
|---|---|---|
| CLI utility | it's the command itself: `gcloud`, `curl` | it's the project or product: "the curl project website" |
| Boolean | you mean the literal value `true` | you describe the evaluation: "the condition is false" |
| Email address | it's input or output: enter `alex` | it's a way to contact someone: support@example.com |

- Don't inflect code items: "send a `POST` request", not "`POST` the data". Add a noun and inflect that noun instead.
- Element names take no angle brackets in prose. Method names drop the class unless it prevents ambiguity: "call its `get` method".
- Code items are proper nouns for casing purposes; keep their case at the start of a sentence (`references/style-format.md`).
- No quotation marks around code unless the quotes are part of the code. API reference string literals are the exception: code font plus double quotes, `"wrap_content"` (§ API parameters, returns, exceptions).
- A UI element that qualifies for code font takes both bold and code font (`references/style-format.md`).
- HTTP status codes: call it a **status code**, never a response code or error code. Put the number and the name in code font: "an HTTP `400 Bad Request` status code". Ranges read `2xx` or "the `200`-`299` range", both numbers in code font. Drop "HTTP" when context makes it obvious.

## Samples

- Introduce every sample with a sentence: colon when the sample follows immediately, period when other material intervenes or the last sentence isn't about the sample.
- Spaces, not tabs; two spaces per level; four-space indentation for Markdown code blocks; wrap at 80 characters, or narrower for print. Follow the language's own style guide, and Google's shell style guide for quoting in `bash`.
- Mark omitted **code** with a comment in the language's syntax, never an ellipsis. Omitted **output** lines use `...` on their own line (§ Command output).
- A block with an omission is not click-to-copy.

## Command syntax

| Notation | Meaning |
|---|---|
| `[FLAG]` | optional — one set of brackets per optional item |
| `{a\|b}` | choose exactly one |
| `ARG...` | repeatable, three dots, no spaces |

- Wrap lines over 80 characters with a four-space continuation indent. Every line except the last must end with the continuation character — `\` on Linux and Cloud Shell, `^` on Windows — or the command doesn't run.
- Click-to-copy blocks must not contain `[]`, `{}`, `|`, or `...`, because the reader can't paste them as-is. Options: drop the optional arguments, give each option its own block, split them into separate tasks, or tell the reader the command contains optional arguments.
- Multi-line input blocks start each line with the prompt symbol; single-line commands can show it, and if the page has both, use it everywhere. Never show the current directory before the prompt. Change the prompt indicator when the context changes (`shell@ $`).
- Document flags with end punctuation only for complete sentences; single words and noun phrases go unpunctuated unless the list mixes both.
- Terminology: command, command group, flag (the Google Cloud term), argument; "option" is the informal catchall; `--` separates tool arguments from user arguments. Don't map a tool's commands onto Linux equivalents.
- Signal names carry one verb each, and no synonym is safe — `SIGKILL` kill · `SIGTERM` terminate · `SIGQUIT` quit · `SIGINT` interrupt · `SIGPAUSE` pause (sleep) · `SIGSUSPEND` suspend · `SIGSTOP` stop. Never swap in cancel, end, exit, or terminate for a signal that means something else.
- Linux commands: name the pieces functionally (option, parameter, argument); metacharacters (`*`, `?`, `^`) do globbing, `|` is a pipe, and `>`, `>>`, `<`, `<<` redirect.
- Link to the full command reference instead of restating every flag.

## Placeholders

- `UPPERCASE_WITH_UNDERSCORES`: `PROJECT_ID`, `INSTANCE_NAME`, `REGION`, `API_NAME`, `BUILD_ID`. Never `MY_*` or `YOUR_*`. IF that casing is genuinely wrong for the context → THEN pick another scheme and stay consistent.
- Markup: `<var>` inside `<code>` for code and command placeholders, bare `<var>` outside code, `` *`PROJECT_ID`* `` in Markdown. Inside a fenced block, formatting doesn't apply — the placeholder is plain uppercase text. No brackets or braces inside the placeholder.
- Explain on first use: "Replace `PROJECT_ID` with your project ID." For several, write "Replace the following:" and list them in the order they appear, each with a lowercase description, even when the value looks obvious. Introduce an example inside a description with an em dash or "such as".
- Repeat the explanations when the document is long, holds several placeholders, or isn't read start to finish.
- Avoid `x` or `xxx` as a placeholder except in established forms such as HTTP `4xx`.

## Command output

- Keep input and output in separate blocks. Introduce output with "The output is similar to the following:" or "The output is the following:", and say what to look at when it matters.
- Show only the relevant part; mark omitted lines with `...` on its own line, not an ellipsis character.
- Introduce placeholders in output with "This output includes the following values:" and list them in order of appearance.

## API coverage and summaries

Document every class, interface, and struct; every constant, field, enum, and typedef; and every method with each parameter, the return value, and any exception thrown. Put a short code sample (about 5-20 lines) at the top of each class or interface page.

- Third-person singular present tense, verb first: "Creates a task on the specified task list." Not "Create a task", not "This method `will` create".
- Describe what the item does, not what a developer might use it for, and don't repeat the item's name.
- Class and type summaries are noun phrases: "A primary toolbar within the activity."
- Openers by kind: "Gets the…", "Sets the…", "Updates the…", "Deletes the…", "Registers…", "Creates a…" (convenience constructors), "Checks whether…" (boolean getters), "Called by… when…" (callbacks).
- Keep members (constants, fields) as short as possible and link the methods that use them, with a "See also:" pointer where it helps.
- No period before the real end of the first sentence and no abbreviations like `e.g.` — generators truncate the short description at the first period.
- IF a class name is also a common word → THEN you can refer to it in lowercase, non-code prose (`activities`, `the action bar`).

## API parameters, returns, exceptions

| Element | Pattern |
|---|---|
| Non-boolean parameter | "The name of the bucket." |
| Boolean parameter (behavior) | "If true, validates the certificate. If false, trusts it without validating." |
| Boolean parameter or return (state) | "True if the list is in sorted order; false otherwise." |
| Default | "Default: 10." — explain the behavior for each value or range first |
| Non-boolean return | "The generated task ID." |
| Exception | "If the list doesn't exist." when the generator adds "Throws", otherwise "Thrown when…" |

- In parameter and return descriptions, `true` and `false` are plain words: no code font, no quotation marks, capital "True" at the start of a sentence. String literals do take code font plus double quotation marks: `"wrap_content"`.
- Capitalize the first word and end with a period, even for fragments.
- Document dependencies the call needs — a permission, an enabled API — and what happens without them ("the method throws a `SecurityException`").
- Deprecations lead with the replacement: "Deprecated. Use `listTasks` instead." Then say why, how to migrate, and which version deprecated it.

## API mechanics

- Link the first mention of a related class or method instead of describing it twice.
- Keep identifiers in code font and don't inflect them (§ Code font).
- Keep parameter names and order identical to the signature.

Upstream: [Code in text](https://developers.google.com/style/code-in-text) · [Code samples](https://developers.google.com/style/code-samples) · [Command-line syntax](https://developers.google.com/style/code-syntax) · [Placeholder formatting](https://developers.google.com/style/placeholders) · [API reference code comments](https://developers.google.com/style/api-reference-comments) · [Verbs in reference documents](https://developers.google.com/style/reference-verbs). Verify a disputed or missing rule against the live page → `references/style-pass.md`.
