# Tool Behavior Guide

> **Purpose**: Known behaviors, tradeoffs, and control patterns per tool.  
> **Audience**: Agents and developers who want to tune quality, token cost, or completeness.  
> Read the [GitHub Tools Reference](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/GITHUB_TOOLS.md) first for the basic API.

---

## How to read this doc

Each section covers one tool and is structured as:

1. **Data shape** — what is returned by default vs what costs more.
2. **Known behaviors** — non-obvious constraints and limitations.
3. **Control patterns** — field combinations to get more or less.
4. **Cost table** — rough token cost by mode.

Skip to the tool you are using.

---

## `ghGetFileContent`

### Data shape

| Mode | What you get | Approx tokens |
|------|-------------|---------------|
| `matchString` | Matching line(s) ± context | ~50–300 |
| `startLine`/`endLine` (small) | Exact line range | ~100–500 |
| `minify: "symbols"` | Imports + function/class/type signatures, bodies stripped | 5–20% of full file |
| `startLine`/`endLine` (large chunk) | Up to `charLength` chars of a range | 1k–10k |
| `fullContent` | Entire file, minified | Can exceed 50k |

### Known behaviors

**B1 — `matchString` returns only the first occurrence.**  
When the string appears N times, the response shows one slice centered on line 1. Other line numbers appear in `warnings` but their content is not included. You must re-call with `startLine` from each warning line to read subsequent occurrences.

**B2 — Large file 413 error is not fixed by `startLine`/`endLine`.**  
The GitHub `/contents/` API rejects files over ~300 KB before line filtering can be applied. Adding `startLine`/`endLine` does **not** help — the 413 fires upstream. The tool's error hint ("use startLine/endLine") is misleading in this case.

**B3 — `matchString` matches after minification.**  
The file is minified before the string search runs. If your search string contains comments or redundant whitespace that would be stripped, the match fails.

**B4 — `matchString` with `matchStringIsRegex` can match multiple occurrences.**  
Regex matching still returns only the first match slice (same as B1).

### Control patterns

**To get exactly what you need — cheapest path:**
```json
{ "matchString": "function createServer", "contextLines": 8 }
```

**When matchString misses (B1/B3) — escalate to a range:**
```json
{ "startLine": 460, "endLine": 520 }
```

**When `matchString` returns "no matches" — check minification:**
```json
{ "matchString": "export async function createServer", "minify": false }
```

**When file is 3MB+ and 413 fires — clone instead:**
```json
// ghCloneRepo with sparsePath, then read locally
{ "owner": "microsoft", "repo": "TypeScript", "sparsePath": "src/compiler" }
```

**Skeleton scan before diving in:**
```json
{ "minify": "symbols" }
// then follow up:
{ "startLine": 1486, "endLine": 1530 }
```

**All N occurrences of a symbol — manual loop:**
1. Call with `matchString: "export function"`, read `warnings` for all line numbers.
2. Re-call with `startLine: <each line>`, `endLine: <line + 10>` for each.

---

## `ghHistoryResearch`

### Data shape

| Call pattern | What you get | Approx tokens |
|-------------|-------------|---------------|
| Broad search (no `prNumber`) | PR list: number, title, author, dates, `reviewSummary` metadata | ~200–2k |
| `prNumber` + no `content` | Same as above for one PR | ~200 |
| `prNumber` + `content.body` | PR description text | +500–5k |
| `prNumber` + `content.changedFiles` | File paths + additions/deletions | +200–2k |
| `prNumber` + `content.patches.mode: "selected"` | Diffs for specific files | +500–5k per file |
| `prNumber` + `content.patches.mode: "all"` | All diffs (can be 100k+) | unbounded |
| `prNumber` + `content.comments` | Inline + discussion comment bodies | +1k–20k |
| `prNumber` + `content.commits` | Commit SHA, message, author per commit | +200–2k |
| `reviewMode: "full"` | All of the above in one call | unbounded |

### Known behaviors

**B1 — `withComments`/`withCommits` are removed from the schema.**  
These legacy fields (`withComments: true`, `withCommits: true`) are silently ignored. The new API requires `content: { comments: {...}, commits: {...} }` with an explicit `prNumber`. Passing the old fields returns only `reviewSummary` metadata — not comment bodies.

**B2 — `reviewSummary.themes` is inferred from keywords, not actual text.**  
`themes: ["approval", "question"]` is produced by scanning comment bodies for regex patterns (`lgtm`, `?`, `change`). It is a heuristic, not a verbatim summary. To read actual comment text, request `content.comments` with a `prNumber`.

**B3 — Label search with spaces is unreliable.**  
`label: "Pages Router"` may return 0 results even when labeled PRs exist. GitHub Search API label filtering for multi-word labels is inconsistent. Use the `query` field instead: `query: "label:\"Pages Router\""`.

**B4 — Broad search strips `content` and `reviewMode`.**  
When `prNumber` is absent, content selectors are silently stripped and a downgrade hint is emitted. Only metadata is returned for list searches.

**B5 — `fullContent` for large PRs is unbounded.**  
A PR with 100+ changed files with `content.patches.mode: "all"` (or legacy `type: "fullContent"`) will return all diffs — potentially 100k–300k chars. The large-PR warning appears in the response **after** the payload is already fetched.

### Control patterns

**Cheapest first pass — find the PR:**
```json
{ "owner": "microsoft", "repo": "TypeScript",
  "query": "satisfies", "matchScope": ["title"], "state": "merged" }
```

**Read PR body and motivation only:**
```json
{ "prNumber": 46827, "owner": "microsoft", "repo": "TypeScript",
  "content": { "body": true, "metadata": true } }
```

**List changed files without diffs:**
```json
{ "prNumber": 46827, "content": { "changedFiles": true } }
```

**Read diffs for specific files only:**
```json
{ "prNumber": 46827, "content": {
    "patches": { "mode": "selected", "files": ["src/compiler/checker.ts"] }
} }
```

**Read actual inline review comments (not the summary heuristic):**
```json
{ "prNumber": 27733, "owner": "facebook", "repo": "react",
  "content": { "comments": { "reviewInline": true, "discussion": false } } }
```

**Read commit list:**
```json
{ "prNumber": 306, "content": { "commits": { "list": true } } }
```

**Full review of a small PR (≤30 files):**
```json
{ "prNumber": 306, "reviewMode": "full" }
```

**When label filter returns 0 — use query syntax:**
```json
{ "owner": "vercel", "repo": "next.js",
  "query": "label:\"Pages Router\" merged:>=2025-01-01",
  "state": "closed" }
```

---

## `ghSearchRepos`

### Data shape

| Field combination | What you get |
|------------------|-------------|
| `owner` only | All repos in that org (bypasses 1000-result cap) |
| `topicsToSearch` only | Repos tagged with that topic |
| `keywords` only | Repos whose name/description matches |
| `topicsToSearch` + `keywords` | Two parallel searches, deduplicated, `totalMatches` double-counted |
| `language` | Filtered by primary language |
| `stars: ">500"`, `updated: ">=2025-01-01"` | GitHub range filters |

Each result includes: `owner`, `repo`, `stars`, `forks`, `language`, `description`, `pushedAt`, `topics`.

### Known behaviors

**B1 — `language` is a dedicated field, not a keyword.**  
Do not pass `keywords: ["language:TypeScript"]` — that searches for the literal string "language:TypeScript" in names and descriptions. Use `language: "TypeScript"` as its own field.

**B2 — `totalMatches` is double-counted when both `topicsToSearch` and `keywords` are set.**  
Two separate GitHub API calls run in parallel and their totals are summed. A repo matching both will count twice. The hint says "upper bound... counted twice." The deduplicated repository list is correct; only the count is inflated.

**B3 — `sort: "created"` is missing from the schema.**  
The `sort` field supports `stars`, `forks`, `help-wanted-issues`, `updated`, `best-match` — but not `created`, even though the underlying ranking logic handles it. Use `created: ">=YYYY-MM-DD"` as a filter instead.

**B4 — The `visibility` field is always `public` in search results.**  
Private repo visibility requires the org-scoped listing endpoint, not the search API.

### Control patterns

**Find TypeScript repos with a topic + stars filter:**
```json
{ "topicsToSearch": ["mcp"], "language": "TypeScript", "stars": ">=500",
  "updated": ">=2025-01-01", "sort": "stars" }
```
> Not `keywords: ["language:TypeScript"]`.

**Enumerate all repos in an org (no 1000-result cap):**
```json
{ "owner": "vercel", "sort": "stars" }
```
> Omitting `keywords` uses the listing endpoint, not search.

**Get deduplicated count when using both topics and keywords:**
```json
// The displayed repository list is accurate.
// Ignore totalMatches; trust the count of returned repos × pages.
{ "topicsToSearch": ["vite-plugin"], "keywords": ["plugin"],
  "owner": "vitejs", "sort": "stars" }
```

**Narrow by recency without double-counting:**
```json
// Use only topics OR keywords, not both, when exact counts matter.
{ "topicsToSearch": ["vite-plugin"], "updated": ">=2024-01-01", "owner": "vitejs" }
```

---

## `npmSearch`

### Data shape

| Data point | Reliability |
|-----------|------------|
| `version` | High — from `npm view` or registry direct |
| `repository.url` | High — from package.json |
| `homepage` | High |
| `description` | High |
| `weeklyDownloads` | Low — separate endpoint, 8s timeout, 0 retries |
| `entrypoints` (main, module, types) | Medium — parsed from package.json exports map |

### Known behaviors

**B1 — `npmSearch` has a hard 8-second timeout on every network call, with no retry for weekly downloads.**  
`fetchWeeklyDownloads` uses `maxRetries: 0`. In restricted networks (VPN, corporate proxy, CI), the weekly download count will silently be absent. The package version and repo URL are fetched separately and are more resilient.

**B2 — Circuit breaker fallback goes to web search, not GitHub.**  
When the npm registry is unreachable, the tool attempts a web-search fallback. If that also fails, it returns an error with a hint to use `ghSearchRepos`. This fallback is not automatic — you must act on the hint.

**B3 — Missing weekly downloads has no fallback hint.**  
When `weeklyDownloads` is absent from the response, no hint is emitted. The field is simply omitted. There is no URL provided to find it manually.

### Control patterns

**Standard lookup — version + repo + homepage:**
```json
{ "name": "hono" }
{ "name": "zod" }
{ "name": "vite" }
// Batch all three in one call.
```

**When `npmSearch` times out — fall back to GitHub:**
```json
// ghSearchRepos with the package name
{ "keywords": ["hono"], "sort": "stars", "limit": 3 }
// Then read package.json from the repo for version:
{ "owner": "honojs", "repo": "hono", "path": "package.json" }
```

**When `weeklyDownloads` is missing from response:**
- Check `https://www.npmjs.com/package/<name>` manually.
- Or use the npm downloads API: `https://api.npmjs.org/downloads/point/last-week/<name>`.

**Scoped packages:**
```json
{ "name": "@octocode/core" }
// Scoped names must include the full `@scope/name`.
```

---

## Cross-tool: token cost control

| Goal | Cheapest approach |
|------|------------------|
| Find if a function exists in a file | `ghSearchCode` with `keywords: ["functionName"]` |
| Read one function body | `ghGetFileContent` with `matchString: "function name"` + small `contextLines` |
| Scan a whole file's structure | `ghGetFileContent` with `minify: "symbols"` |
| Read 2–10 functions from a file | Multiple `startLine`/`endLine` reads in one batched call |
| Read a 3MB+ file | `ghCloneRepo` sparse + local read |
| Understand why a PR was made | `ghHistoryResearch` with `content.body: true` only |
| Review a PR's changes | `content.changedFiles: true` first, then `content.patches.mode: "selected"` for relevant files |
| Get all inline code comments on a PR | `content: { comments: { reviewInline: true, discussion: false } }` |
| Count repos in an org | `owner: "vercel"` with no keywords → `totalMatches` from pagination |
| Get package version only | `npmSearch` — if it times out, read `package.json` from GitHub |

---

## Related

- [GitHub Tools Reference](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/GITHUB_TOOLS.md)
- [Local + LSP Tools Reference](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/LOCAL_TOOLS.md)
- [Clone & Local Tools Workflow](https://github.com/bgauryy/octocode/blob/main/docs/mcp/CLONE_WORKFLOW.md)
