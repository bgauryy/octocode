# Code, commands, and API reference

Load when deciding code font, writing a sample or command line, or writing API reference.

## Code font

Code font covers:
- Attribute names and values; class, method, and function names; keywords; namespaces; package names; command output; text the reader types; strings in commands; placeholders; query parameters.
- Data types; constants; element and enum names; environment variables; database rows and columns; filenames, extensions, paths, and folders.
- DNS record types; HTTP verbs, status codes, and content types; IAM roles; IP addresses; port numbers; UI values the reader entered earlier (an instance name).

Not code font: product, service, and organization names; domains; URLs opened in a browser.

| Case | Code font when | Plain when |
|---|---|---|
| CLI utility | the command: `gcloud`, `curl` | the project: "the curl project website" |
| Boolean | the literal value `true` | the evaluation: "the condition is false" |
| Email address | input or output: enter `alex` | a contact: support@example.com |

- Don't inflect code items: "send a `POST` request", not "`POST` the data".
- No angle brackets on element names in prose. Drop the class from a method name unless ambiguous ("its `get` method").
- Code items keep their case at sentence start; rewrite if that looks wrong.
- No quotes around code unless part of it. Exception: API reference string literals take code font plus double quotes (`"wrap_content"`).
- Say **status code**, never response or error code: "an HTTP `400 Bad Request` status code". Ranges: `2xx`, or "the `200`-`299` range". Drop "HTTP" when obvious.

## Samples

- Introduce every sample like a list (`references/style-structure.md`).
- Spaces, not tabs; two spaces per level; four-space indent for Markdown code blocks; wrap at 80 characters. Follow the language style guide, and Google's shell style guide for `bash` quoting.
- Omitted code: a comment in the language, never an ellipsis. Omitted output: `...` on its own line.
- A block with an omission is not click-to-copy.

## Command syntax

| Notation | Meaning |
|---|---|
| `[FLAG]` | optional; one bracket pair per item |
| `{a\|b}` | choose exactly one |
| `ARG...` | repeatable; three dots, no spaces |

- Wrap over 80 characters with a four-space continuation indent; end each line but the last with `\` (Linux, Cloud Shell) or `^` (Windows).
- Click-to-copy blocks hold no `[]`, `{}`, `|`, or `...`. Drop the optional arguments, give each option a block, split the task, or say the command has optional arguments.
- Multi-line input: prompt symbol on each line; if any block shows a prompt, all do. Never show the current directory before the prompt. Change the indicator when context changes (`shell@ $`).
- Flag descriptions end with punctuation only when they are full sentences, unless the list mixes both.
- Terms: command, command group, flag, argument; "option" is informal; `--` separates tool from user arguments. Don't map a tool's commands to Linux equivalents.
- One verb per signal; no synonyms: `SIGKILL` kill, `SIGTERM` terminate, `SIGQUIT` quit, `SIGINT` interrupt, `SIGPAUSE` pause (sleep), `SIGSUSPEND` suspend, `SIGSTOP` stop.
- Linux terms: metacharacters (`*`, `?`, `^`) glob; `|` pipes; `>`, `>>`, `<`, `<<` redirect.
- Link the full command reference instead of restating every flag.

## Placeholders

- `UPPERCASE_WITH_UNDERSCORES` (`PROJECT_ID`, `INSTANCE_NAME`). Never `MY_*` or `YOUR_*`. If that casing is wrong for the context, pick another scheme and keep it.
- Markup: `<var>` inside `<code>`; bare `<var>` outside code; `` *`PROJECT_ID`* `` in Markdown; plain uppercase inside a fenced block. No brackets or braces in the placeholder.
- Explain on first use: "Replace `PROJECT_ID` with your project ID." Several: "Replace the following:", listed in order of appearance, lowercase descriptions, even when obvious. Introduce an example with an em dash or "such as".
- Repeat explanations in long documents, with many placeholders, or for non-linear reading.
- No `x` or `xxx` placeholders except established forms (`4xx`).

## Command output

- Input and output in separate blocks. Introduce output with "The output is similar to the following:" or "The output is the following:"; say what to look at when it matters.
- Show only the relevant part.
- Output placeholders: "This output includes the following values:", listed in order.

## API coverage and summaries

Document every class, interface, struct, constant, field, enum, typedef, and method with each parameter, return value, and exception. Put a 5-20 line sample at the top of each class or interface page.

- Third-person present, verb first: "Creates a task on the specified task list." Not "Create", not "This method `will` create".
- Say what the item does, not what it is for; don't repeat its name.
- Class and type summaries are noun phrases: "A primary toolbar within the activity."
- Openers: "Gets the…", "Sets the…", "Updates the…", "Deletes the…", "Registers…", "Creates a…" (convenience constructors), "Checks whether…" (boolean getters), "Called by… when…" (callbacks).
- Keep members short; link the methods that use them ("See also:").
- No period before the end of the first sentence and no `e.g.`: generators cut the summary at the first period.
- A class name that is a common word can appear lowercase in plain prose (`activities`).

## API parameters, returns, exceptions

| Element | Pattern |
|---|---|
| Non-boolean parameter | "The name of the bucket." |
| Boolean parameter (behavior) | "If true, validates the certificate. If false, trusts it without validating." |
| Boolean parameter or return (state) | "True if the list is in sorted order; false otherwise." |
| Default | "Default: 10." after the behavior for each value or range |
| Non-boolean return | "The generated task ID." |
| Exception | "If the list doesn't exist." when the generator adds "Throws"; else "Thrown when…" |

- `true` and `false` in these descriptions are plain words (capital "True" at sentence start).
- Capitalize the first word and end with a period, even for fragments.
- Document required dependencies (a permission, an enabled API) and what happens without them.
- Deprecations lead with the replacement: "Deprecated. Use `listTasks` instead." Then why, how to migrate, and the deprecating version.
- Link the first mention of a related class or method. Keep parameter names and order identical to the signature.

Source: [Code in text](https://developers.google.com/style/code-in-text).
