# Code in text

Load when deciding what gets code font and how code appears in prose or a sample.

## Code font

Code font covers:

- Attribute names and values; class, method, and function names; language keywords; namespaces; package names.
- Command output; text the reader types; strings used in commands; placeholders; query parameters.
- Data types; defined constants; element and enum names; environment variables.
- Database elements: row names and column names.
- Filenames, extensions, paths, and folders.
- DNS record types; HTTP verbs, status codes, and content types; IAM roles; IP addresses; port numbers.
- UI values rendered from what the reader entered earlier — an instance name, a server name.

Not code font: product, service, and organization names, domain names, and URLs the reader opens in a browser.

Conditional cases:

| Case | Code font when | Plain when |
|---|---|---|
| CLI utility | it's the command itself: `gcloud`, `curl` | it's the project or product: "the curl project website" |
| Boolean | you mean the literal value `true` | you describe the evaluation: "the condition is false" |
| Email address | it's input or output: enter `alex` | it's a way to contact someone: support@example.com |

- Don't inflect code items: "send a `POST` request", not "`POST` the data". Add a noun and inflect that noun instead.
- Element names take no angle brackets in prose. Method names drop the class unless it prevents ambiguity: "call its `get` method".
- Code items are proper nouns for casing purposes; keep their case at the start of a sentence (`references/style-format.md`).
- No quotation marks around code unless the quotes are part of the code. API reference string literals are the exception: code font plus double quotes, `"wrap_content"` (`references/style-api.md`).
- A UI element that qualifies for code font takes both bold and code font (`references/style-ui.md`).

## HTTP status codes

Call it a **status code**, never a response code or error code. Put the number and the name in code font: "an HTTP `400 Bad Request` status code". Ranges read `2xx` or "the `200`-`299` range", both numbers in code font. Drop "HTTP" when context makes it obvious.

## Samples

- Introduce every sample with a sentence: colon when the sample follows immediately, period when other material intervenes or the last sentence isn't about the sample.
- Spaces, not tabs; two spaces per level; four-space indentation for Markdown code blocks; wrap at 80 characters, or narrower for print. Follow the language's own style guide, and Google's shell style guide for quoting in `bash`.
- Mark omitted **code** with a comment in the language's syntax, never an ellipsis. Omitted **output** lines use `...` on their own line (`references/style-cli.md`).
- A block with an omission is not click-to-copy.

Upstream: [Code in text](https://developers.google.com/style/code-in-text) · [Code samples](https://developers.google.com/style/code-samples). Verify a disputed or missing rule against the live page → `references/style-sources.md`.

Next: commands, prompts, placeholders → `references/style-cli.md`; reserved example values → `references/style-examples.md`; docstrings → `references/style-api.md`.
