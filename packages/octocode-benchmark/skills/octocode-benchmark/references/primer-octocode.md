# Octocode arm primer

Inject as the `octocode` runner's only primer for the local build (`compare/bin/octoc`).
Every research call is `./compare/bin/octoc <tool> '<json>'`, which runs
`octocode <tool> '<json>'` (no MCP, no gh). The query is one JSON object, or
`{"queries":[…]}` to batch up to 5 independent rows. Output is JSON. This primer is fixed
setup — it is **not** counted; use it instead of paying for schema discovery.
(Published 18.x pin → [`primer-octocode-18.md`](primer-octocode-18.md).)

## Tools — what each is for and when to STOP

| Tool | Use it for — and when NOT to |
|---|---|
| `ghSearchCode` | Find unknown files/lines in an owner or repo (`owner` required; default-branch index only). `match:"path"` for paths only. Hits carry line numbers and `commitSha`. Skip when the path is known (→ `ghGetFileContent`). |
| `ghGetFileContent` | Read a **known** path at `ref` (branch, tag, SHA; pass a hit's `commitSha`). Not for discovery. If a search hit already answers, **STOP — don't re-read the file.** |
| `ghStructure` | Tree at any `ref`; `include` globs find paths by name (cheaper than browsing); `operation:"refs"` lists branches/tags with SHAs. `materialize:true` copies the listed files for local tools. |
| `ghSearchRepo` | Discover candidate repos. Skip when owner/repo is known. Start `concise:true`. |
| `ghSearchHistory` | Find PRs / issues / commits: `operation` = `pullRequest` · `issue` · `commit`. Metadata only. |
| `ghGetHistoryItem` | Read one known PR / issue / commit / comparison: `operation` = `pullRequest` · `issue` · `commit` · `compare`. `sections` picks body, files, patches, comments, reviews, commits; omit for a summary. |
| `artifactSearch` | Package versions, dependencies, release source repo (`ecosystem:"npm"`, `packageName`). |
| `ghCloneRepo` | Cache a shallow checkout (`path` for a sparse subtree) **only** for repeated reads, AST matching, or LSP. |
| `localSearch` · `structureSearch` · `astSearch` · `localFetch` | On a clone/materialized path: text/regex search · tree or find-by-name · declarations and structural matches · read a known file or region. |
| `lspSearch` | Callers, references, types, diagnostics — **after** a search/read gives a real `path` + `symbolName` + `lineHint`. |

`clasify` is out of scope for this arm: it calls an external model whose tokens the char log
cannot measure.

## Leanest path (required — this is how the tool is meant to be used)

- **Let a search hit answer.** A `ghSearchCode` hit whose lines answer ends the question.
- **Read regions, not whole files.** Unknown/large file → `minify:"symbols"` outline, then
  one region: `matchString` (+ `contextLines`, default 10; `block:true` widens a declaration
  head to the whole declaration) **or** `ranges:["120-160"]`. `fullContent:true` excludes both.
  Cite source lines, never the `minify` outline.
- **Structured/config files (package.json, tsconfig, lockfile): read whole with
  `fullContent:true, minify:"none"`.** Never conclude a key is absent from a slice.
- **`next.*` is the unread rest.** Exit code 6 means a page remains: follow it as given, or
  name what stays unread. Exit 1 = empty; check scope, spelling, ref before concluding absence.
- **Clone only when it pays** — repeated reads, AST, or LSP. A single remote read stays remote.

## Query forms

```bash
./compare/bin/octoc ghSearchCode '{"owner":"OWNER","repo":"REPO","keywords":["TERM"]}'
./compare/bin/octoc ghGetFileContent '{"owner":"OWNER","repo":"REPO","ref":"SHA","path":"PATH","matchString":"export function NAME","block":true}'
./compare/bin/octoc ghGetFileContent '{"owner":"OWNER","repo":"REPO","ref":"SHA","path":"PATH","minify":"symbols"}'
./compare/bin/octoc ghStructure '{"owner":"OWNER","repo":"REPO","ref":"SHA","path":"DIR","include":["NAME*"]}'
./compare/bin/octoc ghSearchHistory '{"operation":"pullRequest","owner":"OWNER","repo":"REPO","keywords":["TERM"],"concise":true}'
./compare/bin/octoc ghGetHistoryItem '{"operation":"pullRequest","owner":"OWNER","repo":"REPO","number":123,"sections":["files"]}'
```

Errors are self-correcting — a missing or misspelled field returns a guiding message; fix and
retry. For a field this primer doesn't cover, `./compare/bin/octoc schema <tool> --view query`
prints the input contract (that call is measured). Freeze every mutable ref (branch → SHA via
a hit's `commitSha` or `ghStructure operation:"refs"`, plus UTC) before answering; use the
frozen ref.
