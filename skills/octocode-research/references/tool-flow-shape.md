Load to ask for the least that answers, or to judge unread material with `clasify`. Other parts: [routes](tool-flow.md), [carry across sources](tool-flow-carry.md), [analysis, history and recovery](tool-flow-recover.md).

## Shape: the least that answers

| Needed evidence | Select |
|---|---|
| File names | `localSearch view:files`; `ghSearchCode fields:[path]`; `structureSearch view:paths` |
| Counts or distinct values | `localSearch` `view:"countLines"`, `view:"countMatches"` (these scan the whole scope and state exact totals; a content page's `loaded` total is a floor), or `view:"matchOnly"` + `unique:"count"` |
| Outline | `localFetch view:symbols`; large code: filtered `astSearch` symbols |
| One declaration | Unique `matchString` + `block:true`, or AST span → `ranges` |
| Neighboring lines | `matchString` + `contextLines`; reuse search case, whole-word and regex options |
| Several known spans | One `ranges:[{startLine,endLine},…]` read |
| Who uses a symbol | `lspSearch references view:files`; `callers` for calls only |
| Inferred type | `lspSearch hover`; definition alone only locates source |
| One change | History `sections` + `include`, then matching patch lines |
| Read comment-heavy code | `view:compact`; quote or edit from `view:content` |
| Minified text | Inspect the returned window and `column`; use the emitted `localFetch` call when the claim needs the whole line |

Views select evidence: files have no bodies, symbols no implementations, compact no exact quotes. Widen only when the claim needs more; smaller pages never replace completing the selected evidence.

- `block:true` returns the whole enclosing declaration or Markdown section, and `blocks[]` states its `line`/`endLine`. One Markdown section with its subsections: `localFetch` `{"path":"<file>.md","matchString":["^## <Heading>$"],"regexEngine":"rust","block":true}` (escape regex characters in the heading); a literal `"## <Heading>"` also matches longer headings that contain it. Underline-style headings are content of the enclosing `#` section. For code, verify the declaration closes; follow its continuation or use the AST's `line`/`endLine`, never a guessed end.
- On parser limits inspect `viewFallback` or `fileTooLarge`; use exact search rather than treating a missing outline as absence.
- `localSearch` owners use `enclosing` indexes into `declarations`; a hit that declares a name points at its own row by `declaration`: read that row's line to end line instead of a guessed range. LSP locations group under `files[].path`. Read the returned span before citing it.
- `workspaceSymbol` takes `symbolFuzzy`, not `symbolName`; confirm fuzzy candidates at a source anchor.

## Scout: judge what you have not read

Use exact tools for literal facts, counts, identity and absence. Narrow by path, glob or term first; scout only when judgment by meaning can change the next read, and only with `clasify` listed; otherwise page and read exactly. Discover concrete files before treating a directory as a source.

| Unread material | Scout | Verify |
|---|---|---|
| Search hits, listing rows or history rows | `clasify evaluate` with `source:{tool, query}` and `predicates` (answers per row as `p1`, `p2`, …) | `coverage`, then the emitted `next.read` of the deciding spans |
| One large source | `clasify evaluate` with a supported `source` and typed `questions` | Emitted `next.read` or resource read |
| Which files in a set | `clasify evaluate` + `resources` | One answer set per file, then source reads |
| Relevant regions across files | `clasify sections` + `resources` | Page until no `remainingSections`; read selected spans |
| Images | `clasify evaluate` + assets | Presence before `box`; inspect the image for layout claims |
| Long command output: tests, builds, CI or service logs, `git log`, `ps` | Save it under the workspace; `clasify evaluate` with that path as `source` for whether, why and where (the answer's `lines`), `sections` for every place or the order | A known marker: `localSearch` on the saved file; else the answer's `next.read` of its `lines` |

- Keep questions independently answerable, with enough context and exclusions for near misses. The provider has no conversation history or sibling answers. Include an unknown choice and use it for absence: over a sectioned source a `none` label from a section without the fact can outrank the section with it. Confidence is not correctness.
- Predicates over a tool `source` judge each row of that tool's page (a search hit with its context, a listing row as its path), not hidden whole files. The query's match context must include the condition being judged; `responseChars` changes output pages, not provider context.
- `evaluate` uses typed `questions` (`noul`, `choice`, `score`, `box`); use the live schema for mutually exclusive source/content/assets shapes. A path inside literal `content` is not fetched. A box alone does not establish target presence.
- Run the reply's `next` until none is left, then the section pages needed for the claim. Undecided, unanswered, failed (`next.retry`) and quiet units (`next.quiet`) are not negative proof. Counts and absence come from an exact search over the same scope, never from answers.
- A source larger than one evaluation window is judged from one section; it cannot establish whole-file absence, ordering or completeness. Use `sections` and its coverage for broader claims.
- Read the deciding original spans before citing a model answer. If they do not support it, revise the hypothesis. Scores and labels are judgments, not source facts or runtime observations.
- Check `errorCode`, `coverage` and `usage`. Saved-page replay differs from fresh billed judgment; do not repeat a fresh call to obtain its next page. Missing credentials or capabilities are gaps, not authorization to change settings.
