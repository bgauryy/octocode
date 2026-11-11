# Tool checks

How to check every Octocode tool and workflow by hand: direct calls, judged by reading the output, compared
with what `gh`, `rg` or a registry would return. This file is the whole method. Read the owner's output rule
first: a default answer carries the answer, identity for the next call, one indication, and the way to get
more; detail is one call away and nothing is lost.

Ready briefs for a checking agent: [local vs smart rg and ast-grep](../../../packages/octocode-benchmark/prompts/check-local.md), [GitHub and packages vs gh lean](../../../packages/octocode-benchmark/prompts/check-github.md).

## Rules

- **No saved harness scripts.** Scripts hard-code inputs, break when a field or default changes, and then
  report the script's error as the tool's. Call the tool, read the answer, decide. A shell one-liner to count
  characters or time a call is fine; do not keep it.
- **Judge the output, not an exit code.** For each call ask: is the answer right, is it on page 1, can the
  next call be made from what is shown, is anything there that the question did not need?
- **Compare against the lean alternative an expert writes** (`gh … --json/--jq`, `rg -o`, `sed -n`,
  `curl | jq`), not the plainest command. Record characters (≈ 4 per token) and time; Octocode first call and
  repeat call separately.
- **Live GitHub targets:** `vitejs/vite` and other public repositories; not `facebook/react`. Use a second
  repository before calling a GitHub result a defect.
- **Paid and private:** clasify bills per request; keep a count. Never send anything secret-shaped to a real
  host; use a loopback server for anything near credentials.
- **Pinned binary:** check one binary, the one the round pinned, and name its hash in the report.

How to call (CLI text is what agents read; `--json` shows structure; `schema <tool>` prints the schema,
`schema <tool> --select operation=X` one operation):

```bash
printf '%s' '{"queries":[{ … }]}' | NODE_ENV=development OCTOCODE_NATIVE_BIN=<abs path> \
  OCTOCODE_ENABLE_LOCAL=true OCTOCODE_ENABLE_CLONE=true OCTOCODE_SECURITY_CHECKS=true \
  node packages/octocode/out/octocode.js <tool> --input -
```

Add `mainGoal` and `reasoning` to each query. Continue a page only with the printed `{"cursor":"…"}`.

## Every tool, every time

| Check | Pass when |
|---|---|
| Answer | right for the question; the deciding row is on page 1 |
| Identity | the next call can be built from the row (path and line, owner/repo and commitSha, number, id) |
| Lean | no request echo, no value under two names, no per-row constant, no static prose in results |
| More | `next.nextPage`, a `related[]` call or `view:"details"` returns the rest; nothing unreachable |
| Paging | page 2 continues page 1 with no gap or repeat; a cursor with a changed request is refused |
| Incomplete | `isPartial`, coverage, skipped paths or `countScope` say what was not covered |
| Empty | an empty row says what was searched (`filesScanned`, coverage) and offers a lead |
| Errors | a bad call returns one cause line and a runnable example; a credential refusal names the fix |
| Redaction | a secret in a file shows as `[REDACTED-…]`, only the secret part, the same in every view |
| CLI = MCP | the same call gives the same answer on both surfaces, apart from cursors and timings |

## Per tool

### localSearch
- Calls: content search in a folder; `view: files`, `filesWithout`, `countMatches` with `limit` (sorted by
  count); `matchOnly` with a regex; `contextLines`; `wholeWord`; `include`/`exclude`; a term that only matches
  as a regex (expect a hint).
- Look for: hits grouped by file with line numbers; a hit in a line over 4 KB shows a window with its column
  and a ready read of the line; empty rows carry `files: []` and `filesScanned`, no zero counters.
- Compare: `rg -n`, `rg -c | sort | head`, `rg -o '.{0,60}term.{0,60}'`.

### localFetch
- Calls: whole file (page through); `ranges`; `matchString` + `block: true` on a short and a 600-line
  function; `view: symbols`, `compact`; a minified file with and without `matchString`.
- Look for: the exact declaration on page 1 and a full first page for a long one; a match in a long line comes
  first as a window with its byte offset; every byte reachable once by the cursor; a credential file name is
  refused.
- Compare: `sed -n`, `rg -A`, `head -c`.

### structureSearch
- Calls: tree default (names only); `maxDepth`; `operation: files` with `sort` by `sizeBytes`, `lineCount`,
  `modifiedAt`; `nameRegex` of a parent and of its child; `view: details`.
- Look for: every directory has its own row; a metric sort shows that one value per row; the parent and child
  filters give different listings.
- Compare: `find -maxdepth`, `find … stat | sort -rn | head`.

### astSearch
- Calls: `operation: symbols` on a file and a folder; `match` with a `rule` (kind + has/inside); a bare call
  pattern `f($$$A)` in Rust where only `x.f(…)` exists; `syntaxTree`.
- Look for: matches only of the shape asked (no comments or strings); an empty bare pattern says how many
  receiver calls exist and offers the rule call.
- Compare: `rg 'f\('` (which also returns definitions and comments).

### lspSearch
- Calls: `references`, `definition`, `callers`, `hover`, `documentSymbols`, `workspaceSymbol` on a TS and a
  Rust symbol (needs `line`), cold and repeat.
- Look for: rows with roles (declaration, import, call); `coverage.state complete` unless something real is
  missing; one cold-start warning, not three.
- Compare: `rg -n -w name` (spellings, not identity).

### graphBuild and graphQuery
- Calls: build a package folder; `stats` (summary, then `view: details`); `hubs`; `cycles`; `issues` with
  detectors; `find`; a second build of an unchanged folder (reuses the snapshot).
- Look for: the receipt carries linkage, not the call breakdown; stats summary is short and leads to details;
  issue rows state shared controls once; findings are marked as hypotheses.
- Compare: `rg -o "from '…'" | sort | uniq -c` for imports; nothing in the shell answers unused exports.

### clasify
- Calls: `evaluate` with a small `content` and 3 questions; `source` as a large file; `source` as another
  tool's query (`{tool: "localSearch", query: {…}}`); `content` array with `perItem: true`; `sections` over a
  folder glob with `next.nextPage`; an image with noul + choice + box; a choice question whose answer is in one
  section only.
- Look for: one answer per question with the deciding `lines`; coverage counts; units keyed by path:line or
  number; the one knowing section wins over `unknown`; `view: details` adds probabilities and confidence;
  `usage` tokens and request count.
- Compare: characters an agent would have read with localFetch versus the answer size.

### ghSearchRepo
- Calls: `search` with keywords, filters, `sort {by: stars | forks | updatedAt | relevance}`; `get`; `list` for
  an owner; `fields` for moved columns.
- Look for: rows of owner/repo, stars, description; the sorted value shown; moved fields back via `fields`; an
  old sort name refused with the new one named.
- Compare: `gh search repos … --json …`, `gh repo view --json …`.

### ghSearchCode
- Calls: keywords in a repo; `path`; `fields: ["path"]`; `matchIn: ["path"]`; two pages.
- Look for: matched lines per file; a renamed repository followed and named; page 2 fast and without repeats.
- Compare: `gh search code … --json path,textMatches --jq …`.

### ghStructure
- Calls: `tree` of a folder, `maxDepth`, `include` glob, `sort {by: sizeBytes, order: desc}`; `refs`,
  `refs` with `refKinds: ["tags"]` and `limit`; `languages`; `materialize` on a few files.
- Look for: sizes ranked over the whole listing; the repository's own release tags first; tags only when asked.
- Compare: `gh api repos/O/R/git/trees/main?recursive=1 --jq …`, `gh api repos/O/R/tags`.

### ghGetFileContent
- Calls: a small file; `ranges`; `matchString` + `block` on a long function; `view: symbols`, `compact`; a
  minified file with `matchString`; a path at a tag `ref`; a directory path (expect a clear error).
- Look for: the same as localFetch; `commitSha` on the first page.
- Compare: `gh api …/contents/PATH -H 'Accept: application/vnd.github.raw' | sed -n`.

### ghSearchHistory
- Calls: `commit` for a path; `pullRequest` with keywords, `filters: {isMerged: true}`, `sort {by:
  commentsCount}`; `issue` with state; `fields`; two pages.
- Look for: sha/number, date, headline by default; author and labels via `fields`; the sorted value shown.
- Compare: `gh api …/commits?path=… --jq …`, `gh search prs … --json …`.

### ghGetHistoryItem
- Calls: a PR summary; PR `sections: ["files"]`, `["reviews"]`, `["patches"]` with `include`; a commit with
  `sections: ["patches"]`; `compare` of two tags with one file; an issue with comments.
- Look for: a section read carries number, state and the commit ids, then the section; the compare summary
  pages commits apart; patch lines carry line numbers.
- Compare: `gh pr view --json files`, `gh api …/compare/A...B --jq …`.

### artifactSearch
- Calls: `resolve` npm with a version, PyPI latest, crates latest; `search` npm; `view: details`.
- Look for: name, version, owner/repo, directory, ref (commit) in the summary; dependencies, licence, engines
  in details; the summary's lead runs to details.
- Compare: `npm view pkg@v version repository gitHead --json`, `curl … | jq`.

### securityCheck
- Calls: `package` with a vulnerable version; `repository`; `commit` (one known, one unknown); `url` as a purl;
  `advisory` by id; each with `view: details`.
- Look for: `affected`, `upgradeTo`, advisory count, highest severity, one link; an unknown commit says that 0
  proves nothing; details lists every advisory with ranges and fixes.
- Compare: OSV `POST /v1/query | jq`, `gh api advisories?affects=…`.

### ghCloneRepo
- Calls: a small repository; a sparse `paths` clone; then localSearch inside the returned location.
- Look for: the location and commit stated; a repeat reuses the clone; local tools work on it.
- Compare: `gh repo clone O/R -- --depth 1` then `rg`.

## Workflows (each must finish in the same number of calls as last round)

| Workflow | Calls |
|---|---|
| Search a symbol, read its body | ghSearchCode → ghGetFileContent `matchString` + `block` |
| Folder, then an outline | ghStructure tree → ghGetFileContent `view: symbols` |
| A file's history, then the newest patch | ghSearchHistory `commit` with `path` → ghGetHistoryItem `patches` with `include` |
| A merged PR, then its files and reviews | ghSearchHistory `pullRequest` → ghGetHistoryItem `sections` |
| A tag, then one file's change between releases | ghStructure `refs` → ghGetHistoryItem `compare` |
| A package version, then its source at that release | artifactSearch `resolve` → ghGetFileContent at the returned `ref` |
| Is this version affected | securityCheck `package` → `view: details` only if the list is needed |
| Many searches in one repository | ghCloneRepo → localSearch ×3 |
| Local: find, then read | localSearch → localFetch `matchString` + `block` |
| Local: who calls it | localSearch → lspSearch `callers` at the hit's line |
| Local: what is unused | graphBuild → graphQuery `issues` |
| Large unread evidence | clasify with `source` (a file, or a tool query) → read only the deciding lines |

## Report

One table per round: tool, path, characters before and after, first and repeat time, the lean alternative's
characters, verdict, and a one-line note for any defect with the call that reproduces it. Defects go to the
queue with an owner; a fixer never signs off its own work.
