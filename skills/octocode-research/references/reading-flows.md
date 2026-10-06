# Reading flows

Load when choosing between an outline, exact source, or a full file. `localFetch` and `ghGetFileContent` share selectors and pagination (GitHub adds `owner`, `repo`, `ref`).

- Output is numbered source, `<line>\t<text>`, with `... [lines A-B not requested] ...` between requested windows: quote and cite from it.
- Known literal: `matchString` (a list for several literals is a grep map); `contextLines` defaults to 10 (max 100); read a wider span with `ranges`.
- Several known regions: one call with `ranges` (e.g. `["95-105","255-265"]`). Whole declaration: `block:true` on a match or range; check the returned span closes the declaration, else read `astSearch symbols` `line`/`endLine`. Never guess an end.
- Large doc: `minify:"symbols"` lists headings with lines; read from a heading to the next equal-or-higher one. Underlined headings or `minifyFallback` need exact search; a missing entry does not prove absence.
- Code: `minify:"symbols"` gives signatures; read exact source before explaining behavior. `minify:"standard"` compacts text: never quote or edit from it.
- Unanchored reads page by a byte budget; copy `next.continue`. `fullContent:true` is for small files: past its size limit it returns the first page with `partialReasons:["full-content-size-limit"]`. A large unanchored read that set `mainGoal` may offer `hints.clasify` to locate it first.
- Search, fetch, history, and AST continuations are different contracts: copy `next.*` pages and `hints.*` leads; never rename fields. PR patches default to `minify:"standard"` (outer context trimmed to `...`); request `minify:"none"` on `ghGetHistoryItem` for exact text.

Next: local follow-ups → `workflow-local.md`; remote → `workflow-external.md`.
