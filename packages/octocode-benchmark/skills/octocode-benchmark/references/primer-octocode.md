# Octocode arm primer

Inject as the `octocode` runner's only primer for the local build (`compare/bin/octoc`).
Every research call is `./compare/bin/octoc <tool> '<json>'`, which runs
`octocode <tool> '<json>'` (no MCP, no gh). The JSON is always an envelope:
`{"queries":[…]}` for a fresh call (several independent rows may share one call), or
`{"cursor":"…"}` copied from a continuation. Output is JSON. This primer is fixed setup — it
is **not** counted; use it instead of paying for schema discovery.
(Published 18.x pin → [`primer-octocode-18.md`](primer-octocode-18.md).)

## Tools — what each is for and when to STOP

| Tool | Use it for — and when NOT to |
|---|---|
| `ghSearchCode` | Find unknown files/lines in an owner or repo (`owner` required; default-branch index only). Hits carry line numbers and `commitSha`. Skip when the path is known (→ `ghGetFileContent`). |
| `ghGetFileContent` | Read a **known** path at `ref` (branch, tag, SHA; pass a hit's `commitSha`). Not for discovery. If a search hit already answers, **STOP — don't re-read the file.** |
| `ghStructure` | `operation:"tree"` lists a tree at any `ref` (`include` takes repo-relative globs such as `**/http*`; cheaper than browsing); `"refs"` lists branches/tags with SHAs; `"languages"`. |
| `ghSearchRepo` | Discover candidate repos (`operation:"search"`); `"get"` one known repo; `"list"` an owner's repos. Skip when owner/repo is known. |
| `ghSearchHistory` | Find PRs / issues / commits: `operation` = `pullRequest` · `issue` · `commit` (`pathPrefix` scopes commits to a directory). |
| `ghGetHistoryItem` | Read one known PR / issue / commit / comparison: `operation` = `pullRequest` · `issue` · `commit` · `compare`. `matchString` narrows patches to matching hunks. |
| `artifactSearch` | Package versions, dependencies, release source repo: `operation:"resolve"` + `ecosystem` (npm, pypi, maven, go, rubygems, crates) + `packageName`. |
| `ghCloneRepo` | Fetch a fresh shallow checkout **only** for repeated local reads, AST matching, or LSP. |
| `localSearch` · `structureSearch` · `astSearch` · `localFetch` | On a clone: text/regex search · tree or find-by-name · structural matches and declarations · read a known file or region. |
| `lspSearch` | Definition, references, callers, callees, types — **after** a search/read gives a real `path` + `symbolName` + `line`. |

`clasify`, `graphBuild` and `graphQuery` are out of scope for this arm: clasify calls an
external model whose tokens the char log cannot measure, and the graph tools are beta.

## Leanest path (required — this is how the tool is meant to be used)

- **Let a search hit answer.** A `ghSearchCode` hit whose lines answer ends the question.
- **Read regions, not whole files.** Unknown/large file → `view:"symbols"` outline, then one
  region: `matchString:["NAME"]` (an array; add `contextLines:{"before":2,"after":2}`, or `block:true` to widen a
  declaration head to the whole declaration) **or** `ranges:[{"startLine":120,"endLine":160}]`.
  `view:"compact"` trims whitespace and comments. Cite source lines, never the outline.
- **Structured/config files (package.json, tsconfig, lockfile): read whole** (no `view`, no
  region). Never conclude a key is absent from a slice.
- **Follow pages before concluding.** A result that says more pages remain (`hasMore`, a
  `next` cursor) still exits 0: call again with `{"cursor":"…"}` as given, or name what stays
  unread. Exit 1 = empty; check scope, spelling and ref before concluding absence. Exit 2 =
  invalid input; the error names the field to fix.
- **Clone only when it pays** — repeated reads, AST, or LSP. A single remote read stays remote.

## Query forms

```bash
./compare/bin/octoc ghSearchCode '{"queries":[{"owner":"OWNER","repo":"REPO","keywords":["TERM"]}]}'
./compare/bin/octoc ghGetFileContent '{"queries":[{"owner":"OWNER","repo":"REPO","ref":"SHA","path":"PATH","matchString":["export function NAME"],"block":true}]}'
./compare/bin/octoc ghGetFileContent '{"queries":[{"owner":"OWNER","repo":"REPO","ref":"SHA","path":"PATH","view":"symbols"}]}'
./compare/bin/octoc ghStructure '{"queries":[{"operation":"tree","owner":"OWNER","repo":"REPO","ref":"SHA","path":"DIR","include":["**/NAME*"]}]}'
./compare/bin/octoc ghSearchHistory '{"queries":[{"operation":"pullRequest","owner":"OWNER","repo":"REPO","keywords":["TERM"]}]}'
./compare/bin/octoc ghGetHistoryItem '{"queries":[{"operation":"pullRequest","owner":"OWNER","repo":"REPO","number":123,"matchString":["TERM"]}]}'
```

Errors are self-correcting — a missing or misspelled field returns a message naming it and
the allowed fields; fix and retry. For a field this primer doesn't cover,
`./compare/bin/octoc schema <tool>` prints the input contract and `<tool> --help` prints one
example per form (those calls are measured). Freeze every mutable ref (branch → SHA via a
hit's `commitSha` or `ghStructure operation:"refs"`, plus UTC) before answering; use the
frozen ref.
