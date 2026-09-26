# Reading flows and defaults

Load when deciding between search, a compact overview, exact source, or a complete file. Choose by the evidence needed; a file extension alone does not determine the view.

## Semantic targets in unread files

Before loading a large body, decide whether a useful literal or known small range answers the task. If so, read directly. Otherwise use `clasify` with an unread file-reader request and atomic `questionType:"locate"` questions; batch independent same-file targets. Verify the returned source windows together. This route applies to local source, docs, logs, saved scrape text and browser artifacts. Preserve provenance and coverage; hints do not replace source proof.

## Markdown

- Known phrase or source range: fetch it directly with exact content. Skip an outline when it cannot change the selected region.
- Unfamiliar large document: request `minify:"symbols"` for headings with source line labels. Choose a heading and fetch from its source line through the line before the next heading of equal or higher level. Include nested subsections. Follow outline pagination before deciding where the section ends; use `totalLines` for the last section.
- The current Markdown outline indexes `#` headings outside fenced blocks. Underlined headings, missing headings, or `contentView`/`minifyFallback` reporting a fallback require exact source or lexical search. A missing outline entry does not prove a section is absent.
- `minify:"standard"` compacts document text and may change whitespace or remove comments. Use it only when that transformed text answers the question. Read exact source for quotes, examples, commands, and edits.

## Code

- API orientation: `localFetch` or `ghGetFileContent` with `minify:"symbols"` gives a compact signature outline with source line labels. It omits bodies and may include imports. Follow with an exact read before explaining behavior.
- Known local declaration: use `astSearch operation:"symbols"` with `name`. The name filter is a substring; select the intended declaration among candidates, including its path and parent. Its `line`/`endLine` are one-based and feed `localFetch` `startLine`/`endLine` directly. Do not guess a function's end from the next signature: nested declarations and overloads can make that wrong.
- Known literal: lexical search or `matchString` gives exact local evidence. Neither a line nor byte match-context window guarantees a complete function. Use a declaration range when the answer needs the body.
- Cross-file identity, usages, callers, or types: use `lspSearch` with an observed anchor. AST declarations and signature outlines provide syntax, not semantic identity. Inspect provider metadata and re-anchor when necessary.

## Reader defaults

Both readers keep `minify:"none"`, `chunkType:"lines"`, and `chunkSize:100`. Match context defaults to five source lines per side for line chunks and 256 UTF-8 bytes per side for byte chunks. Use `contextLines` or `contextBytes` (exclusive) to override the context unit explicitly. Continuations pin the chosen context even if pagination switches units. Line pages also have a 16384-byte budget; byte pages default to 16384 bytes. Larger pages trade fewer continuation calls for more context per response. Choose a targeted source region before increasing a page size.

Byte context follows full-source redaction, keeps UTF-8 characters whole, and joins disjoint windows with a newline. A byte page offset addresses this selected view, not source bytes.

`fullContent:true` requests the complete selected file view in one response up to 50000 UTF-8 bytes, subject to source/security limits. It cannot accompany match/range or chunk controls. Larger recoverable files expose `next.continue`; follow it to retrieve all content. A terminal limit must be reported. All modes retain redaction.

## Choose among all tools

| Need | Starting tool and scope | Next evidence |
|---|---|---|
| Local text or literal | `localSearch`, narrow `path`; `regex:"literal"` for plain text; files/count views when snippets are unnecessary | Exact fetch or an observed semantic anchor |
| Local file or directory discovery | `astSearch` files/tree | Read a known file; outline only when orientation is needed |
| Local declaration or syntax | `astSearch` symbols/match; filter a known name | Exact source range; LSP for identity |
| File dependencies | `astTopology` (beta), explicit analysis and root | Confirm relevant symbols and dynamic edges before deletion claims |
| Local source | `localFetch`, range/match when known | Continue the same view or select a body from its outline |
| Semantic code question | `lspSearch`, observed source anchor; structured output retains typed locations | Exact source or subsequent semantic query; inspect provider capabilities |
| Unknown GitHub repo/path/anchor | `ghSearch` repositories/tree/code respectively | Fetch at the intended ref; code search cannot select a non-default branch |
| Known GitHub file | `ghGetFileContent`, same read choices as local | Exact region, continuation, or local checkout for repeated analysis |
| Unknown history item | `ghSearchHistory`, metadata triage with relevant repo/path/time filters | `ghGetHistoryItem` after identifying the item |
| Known history item | `ghGetHistoryItem`, select only needed body/comments/files/patches | For PR review, changed files before selected patches; request `minify:"none"` for exact PR text or diff context. PR `standard` is a separate triage default, not the file-reader default |
| Repeated remote AST/LSP or cross-file work | `ghCloneRepo`, sparse only when that subtree covers the question | Use the returned local path and preserve sparse/cached-state limitations |
| Package identity | `artifactSearch` with explicit `type`; `packageName` when known, `keywords` for discovery (PyPI exact only) | Follow source repository links; package metadata is not code evidence |

Use each tool's declared continuation units. History body/patch offsets, search pages, AST snapshots, and fetch line/byte offsets are different contracts; copy executable `next.*` queries instead of translating between them. Successful results omit optional next-tool hints: choose the next evidence from returned paths, source anchors, and the table above. Empty/error results may offer brief recovery hints; pagination and completeness recovery calls remain available. Keep existing numeric defaults unless a task-specific measurement justifies an override. These workflows reduce unnecessary content; they do not establish one universally optimal page size.

Next: [local workflow](workflow-local.md), [external workflow](workflow-external.md), or [query examples](tool-examples.md).
