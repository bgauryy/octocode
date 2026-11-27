Load when ruling on code in text, samples, placeholders, API comments, claims, example values or link text. Review with the [style guide procedure](style-guide.md#review-procedure); for a full rule, open the Google style guide page its heading links.

# Style: code, claims and links

## Code ([code in text](https://developers.google.com/style/code-in-text), [samples](https://developers.google.com/style/code-samples), [syntax](https://developers.google.com/style/code-syntax), [placeholders](https://developers.google.com/style/placeholders), [API reference](https://developers.google.com/style/api-reference-comments))

- Code font for identifiers, paths, commands, values, env vars, HTTP verbs, status codes; never inflected: `POST`ed → send a `POST` request.
- Introduce samples and match the language and project formatting; mark omissions with comments, not `...`.
- Syntax: `[OPTIONAL]`, `{a|b}`, `ARG...`; none in click-to-copy blocks.
- Placeholders: `PROJECT_ID`, never `MY_ID`, explained once.
- Output in its own block: "The output is similar to the following:".
- API summaries: third person, verb first, no period inside the first sentence. Booleans: "True if the list is sorted; false otherwise." Deprecations lead with the replacement: Deprecated. Use `listTasks` instead.

## Claims, examples, links ([timeless](https://developers.google.com/style/timeless-documentation), [examples](https://developers.google.com/style/examples), [link text](https://developers.google.com/style/cross-references))

- Timeless: cut `currently`, `new`, `soon`, `latest`; name a release when a date matters.
- No unreleased features or superlatives (`best`, `fastest`); cite performance and cost claims or cut them. Security: helps prevent, not prevents ([excessive claims](https://developers.google.com/style/excessive-claims)).
- Products: official name and case, never a verb or plural; "the" before tool and API names. Link others' docs, never copy.
- Example values: `example.com`; 800-555-0100 to 0199; IPs `192.0.2.0/24`, `2001:db8::/32`; varied names (Dana, Kai); no real data, `foo`, or `bar`.
- Link text names the destination, never `click here`; say when a link downloads a file or leaves the site. Renaming a heading or file: run the rename check in the [doc check](../SKILL.md#check-before-delivery).
