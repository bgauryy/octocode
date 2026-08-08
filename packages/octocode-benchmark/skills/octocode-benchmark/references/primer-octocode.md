# Octocode arm primer

Inject as the `octocode` runner's only primer. Every research call is
`npx octocode tools <tool> --queries '<json>'` (no MCP, no gh). `--queries` takes one JSON
object or an array of objects (batch). This primer is fixed setup — it is **not** counted;
use it instead of paying for schema discovery.

## Tools — what each is for and when to STOP

| Tool | Use it for — and when NOT to |
|---|---|
| `ghSearchCode` | Remote code/file discovery. `match:"path"` for filenames, `match:"file"` only when snippets matter. Skip if you already have the path (→ `ghGetFileContent`) or need repo discovery (→ `ghSearchRepos`). |
| `ghGetFileContent` | Read a file or a **region** once you know the path — not for discovery. If a `ghSearchCode` snippet already answers, **STOP — don't re-read the file.** |
| `ghViewRepoStructure` | Orient a repo/tree before fetching, or verify a path/branch after a 404. Know a filename fragment? `ghSearchCode(match:"path")` is cheaper. |
| `ghSearchRepos` | Discover candidate repos. Skip when owner/repo is known — go straight to code/structure. Start `concise:true`. |
| `ghSearchPullRequests` | PR archaeology — how/why a change landed, reviews, one PR's diff/files. Not current code, commits, or issues. |
| `ghSearchIssues` | Issue/comment search. Not PRs (→ `ghSearchPullRequests`). |
| `ghSearchCommits` | Commit archaeology — when/why a file/area changed. Not current code or PRs. |
| `ghCloneRepo` | Materialize a repo/sparse subtree **only** for repeated reads, structural (AST) matching, or LSP semantics. |
| `npmSearch` | Resolve an npm package → its source repo. |
| `localSearchCode` | Text/regex/AST search in a clone → file+line anchors. Modes: `discovery`=paths, `paginated`=snippets, `detailed`=context, `structural`=AST. |
| `localGetFileContent` | Read an exact cloned file/region (same region params as `ghGetFileContent`). |
| `localViewStructure` · `localFindFiles` | Cloned tree browse · find-by-path-metadata. |
| `localFindDeadCode` | Likely-unreferenced exports in a clone. |
| `lspGetSemantics` | Definitions, references, callers/callees, symbols, types, diagnostics — **after** search/read gives a real file+line. `documentSymbols`/`diagnostic` need `uri`; `workspaceSymbol` needs `symbolName`; others need `uri`+`symbolName`+`lineHint`. |

`ghListReleases` / `ghSearchDiscussions` are opt-in; a disabled tool is unavailable, not a failed requirement.

## Leanest path (required — this is how the tool is meant to be used)

- **Let a search snippet answer.** A `ghSearchCode` hit with the answer in its snippet ends the question — do not fetch the file.
- **Read regions, not whole files.** `ghGetFileContent` least-cost path: unknown/large file → `minify:"symbols"` outline first, then a region via `matchString` (pairs with `contextLines`, returns padded `matchRanges` + exact `matchedLines`) **or** `startLine`+`endLine`. Choose exactly one of `fullContent` / `matchString` / `startLine+endLine`. `charOffset` pages a partial read.
- **Structured/config files (package.json, tsconfig, lockfile): read whole with `minify:"none"`** — compaction can elide object boundaries. Exact key/field/value membership requires an unminified read; a partial slice can cut a nested object, so never conclude a field is absent from a slice — continue via `charOffset`/`next` or re-read the small file whole.
- **Every response reports `totalLines`/`sourceChars`/`isPartial`.** Report only bytes you fetched; never invent a field or quote.
- **Clone only when it pays** — repeated reads, AST/structural matching, or LSP. A single remote read should stay remote.

## Query forms

```bash
npx octocode tools ghSearchCode --queries '{"owner":"OWNER","repo":"REPO","keywords":["TERM"],"match":"path"}'
npx octocode tools ghGetFileContent --queries '{"owner":"OWNER","repo":"REPO","path":"PATH","branch":"SHA","matchString":"SYMBOL","contextLines":8}'
npx octocode tools ghGetFileContent --queries '{"owner":"OWNER","repo":"REPO","path":"PATH","branch":"SHA","minify":"symbols"}'
npx octocode tools ghViewRepoStructure --queries '{"owner":"OWNER","repo":"REPO","branch":"SHA","path":"PATH"}'
npx octocode tools ghSearchPullRequests --queries '{"owner":"OWNER","repo":"REPO","prNumber":123}'
```

Errors are self-correcting — a missing/typo'd field returns a guiding message (e.g. *"Repository
scope requires owner"*); fix and retry. For a field this primer doesn't cover, `npx octocode
tools <name> --scheme --brief` prints the compact schema (that call is measured). Freeze every
mutable ref (branch/PR-state/SHA + UTC) before answering; use the frozen ref.
