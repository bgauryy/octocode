# Reading flows

Load when choosing between an outline, exact source, or a full file. `localFetch` and `ghGetFileContent` share selectors and pagination (GitHub adds `owner`, `repo`, `branch`).

- Known phrase or range: fetch exactly; several regions: one call with `ranges`.
- Large doc: `minify:"symbols"` lists headings with lines; read from a heading to the next equal-or-higher one. Underlined headings or `minifyFallback` need exact search; a missing entry does not prove absence.
- Code: `minify:"symbols"` gives signatures; read exact source before explaining behavior. Whole declaration: `block:true`, or `astSearch symbols` `line`/`endLine`; never guess an end.
- `minify:"standard"` rewrites whitespace and comments: never quote or edit from it.
- Defaults: `minify:"none"`, 100-line chunks, 16384-byte pages. `fullContent:true` returns up to 50000 bytes and excludes match/range controls; follow `next.continue` or report the terminal limit.
- Search, fetch, history, and AST continuations are different contracts: copy `next.*`, never translate. For exact PR text request `minify:"none"` on `ghGetHistoryItem`.

Next: local follow-ups → `workflow-local.md`; remote → `workflow-external.md`.
