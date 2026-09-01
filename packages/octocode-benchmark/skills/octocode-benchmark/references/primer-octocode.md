# Octocode arm primer

Inject as the `octocode` runner's only primer. Every research call is
`npx octocode tools <tool> --queries '<json>'` (no MCP, no gh). `--queries` takes one JSON
object or an array of objects (batch). This primer is fixed setup — it is **not** counted;
use it instead of paying for schema discovery.

## Tools — what each is for and when to STOP

| Tool | Use it for — and when NOT to |
|---|---|
| `ghSearch` | Remote discovery through strict operations: `code` for code/file paths, `repositories` for repository discovery, and `tree` for a known repository. For code, use `match:"path"` when only filenames matter. For repositories, start with `concise:true`. Skip discovery when you already know the exact file path. |
| `ghGetFileContent` | Read a file or a **region** once you know the path — not for discovery. If a `ghSearch(operation:"code")` snippet already answers, **STOP — don't re-read the file.** |
| `ghSearchHistory` | Search or list history with `operation:"pullRequests"`, `"issues"`, or `"commits"`. Use it for discovery, not exact item reads. |
| `ghGetHistoryItem` | Read one PR or issue by `number`, one commit by `ref`, or compare `base`+`head`. Select only the content or diff you need. |
| `ghCloneRepo` | Materialize a repo/sparse subtree **only** for repeated reads, structural (AST) matching, or LSP semantics. |
| `npmSearch` | Resolve an npm package → its source repo. |
| `localSearch` | Unified text/regex/AST/path/tree search in a clone → file+line anchors. Select a strict `operation`. |
| `localGetFileContent` | Read an exact cloned file/region (same region params as `ghGetFileContent`). |
| `localAnalyzeGraph` | Bounded dependencies, dependents, paths, reachability, cycles, and dead-code candidates in a clone. |
| `lspGetSemantics` | Definitions, references, callers/callees, symbols, types, diagnostics — **after** search/read gives a real file+line. `documentSymbols`/`diagnostic` need `uri`; `workspaceSymbol` needs `symbolName`; others need `uri`+`symbolName`+`lineHint`. |

## Leanest path (required — this is how the tool is meant to be used)

- **Let a search snippet answer.** A `ghSearch(operation:"code")` hit with the answer in its snippet ends the question — do not fetch the file.
- **Read regions, not whole files.** `ghGetFileContent` least-cost path: unknown/large file → `minify:"symbols"` outline first, then a region via `matchString` (pairs with `contextLines`, returns padded `matchRanges` + exact `matchedLines`) **or** `startLine`+`endLine`. Choose exactly one of `fullContent` / `matchString` / `startLine+endLine`. `charOffset` pages a partial read.
- **Structured/config files (package.json, tsconfig, lockfile): read whole with `minify:"none"`** — compaction can elide object boundaries. Exact key/field/value membership requires an unminified read; a partial slice can cut a nested object, so never conclude a field is absent from a slice — continue via `charOffset`/`next` or re-read the small file whole.
- **Every response reports `totalLines`/`sourceChars`/`isPartial`.** Report only bytes you fetched; never invent a field or quote.
- **Clone only when it pays** — repeated reads, AST/structural matching, or LSP. A single remote read should stay remote.

## Query forms

```bash
npx octocode tools ghSearch --queries '{"operation":"code","owner":"OWNER","repo":"REPO","keywords":["TERM"],"match":"path"}'
npx octocode tools ghSearch --queries '{"operation":"repositories","keywords":["TERM"],"concise":true}'
npx octocode tools ghSearch --queries '{"operation":"tree","owner":"OWNER","repo":"REPO","branch":"SHA","path":"PATH"}'
npx octocode tools ghGetFileContent --queries '{"owner":"OWNER","repo":"REPO","path":"PATH","branch":"SHA","matchString":"SYMBOL","contextLines":8}'
npx octocode tools ghGetFileContent --queries '{"owner":"OWNER","repo":"REPO","path":"PATH","branch":"SHA","minify":"symbols"}'
npx octocode tools ghSearchHistory --queries '{"operation":"commits","owner":"OWNER","repo":"REPO","path":"PATH"}'
npx octocode tools ghGetHistoryItem --queries '{"operation":"pullRequest","owner":"OWNER","repo":"REPO","number":123,"content":{"body":true}}'
```

Errors are self-correcting — a missing/typo'd field returns a guiding message (e.g. *"Repository
scope requires owner"*); fix and retry. For a field this primer doesn't cover, `npx octocode
tools <name> --scheme --brief` prints the compact schema (that call is measured). Freeze every
mutable ref (branch/PR-state/SHA + UTC) before answering; use the frozen ref.
