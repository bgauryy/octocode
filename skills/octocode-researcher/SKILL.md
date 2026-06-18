---
name: octocode-researcher
description: Use this skill whenever you call the octocode MCP tools directly to research code — local (localSearchCode, localGetFileContent, localViewStructure, localFindFiles, lspGetSemantics, localBinaryInspect) or GitHub (npmSearch, ghSearchRepos, ghViewRepoStructure, ghSearchCode, ghGetFileContent, ghHistoryResearch, ghCloneRepo). It is the operating guide for picking the right tool, chaining find → map → read → prove, minifying reads, paginating losslessly, and reading the evidence/hints signals. Triggers on "how does X work", "where is Y defined", "who calls Z", "trace this flow", "find usages", "read this file efficiently", "research this repo/package". For multi-step research over the HTTP server, use octocode-research instead.
---

# Octocode Researcher — Tool Operating Guide

Drive the octocode tools efficiently and prove every claim with `file:line` evidence. Two families:

- **Local** (a checkout on disk): `localSearchCode`, `localGetFileContent`, `localViewStructure`, `localFindFiles`, `lspGetSemantics`, `localBinaryInspect`.
- **GitHub** (the live API): `npmSearch`, `ghSearchRepos`, `ghViewRepoStructure`, `ghSearchCode`, `ghGetFileContent`, `ghHistoryResearch`, `ghCloneRepo`.

## Where to start (decide this first)

```
Is the code already on disk (a checkout you're reading or editing)?
   YES → Part A (local tools).
   NO, it's an external repo/package:
        have a package name?  → npmSearch       (resolves owner/repo + monorepo subpath)
        have only a concept?  → ghSearchRepos   (discover the repo)
        know owner/repo?      → ghViewRepoStructure (skip discovery)
        ↳ then Part B. If the investigation grows past ~3 files in one repo,
          ghCloneRepo and switch to Part A on the clone.
```

## Universal rules (apply to every call, both families)

> **Golden rule — cheap → targeted → read → prove.** Orient with the cheapest tool (discovery / path-only / skeleton), narrow with scope filters, read the smallest slice with the right `minify`, then prove semantics with the LSP. Never open with a full-file read or a broad scan.

- **Stop at the evidence gate.** `evidence.answerReady:true` → stop; don't issue a confirming read. Page only when `pagination.hasMore` (or `contentPagination.*.hasMore`) is true.
- **Follow `hints[]`.** Every response carries the exact next step (next `page`, `charOffset`, `lineHint`, `prNumber`, lean→verbose nudge). Use it instead of recomputing offsets. Pass `researchGoal`/`reasoning` so the hints sharpen.
- **Batch up to 5 independent queries per call** (every tool). N paths / packages / PR numbers → one call. Serialize only when step N feeds N+1 (search → read → LSP). LSP batches share one warm server — a big win (see §A9).
- **Two pagination layers, don't confuse them.** *Per-query* pagination lives inside each result (`pagination`, `contentPagination`, `matchRanges`, `matchPage`). *Whole-response* `responseCharLength`/`responseCharOffset` are **root** params (siblings of `queries`) capping the entire envelope across all batched queries.
- **Empty ≠ absent.** GitHub code-search misses on unindexed / renamed / archived repos and non-default branches; LSP relational queries are bounded by open files. Confirm a "not found" with `ghViewRepoStructure` + `ghGetFileContent` (or a local read) before concluding it.
- **Remote → local when work deepens** (> ~3 files in one repo): `ghCloneRepo`, then `localSearchCode` / `localGetFileContent` / `lspGetSemantics` on the clone — cheaper than many round-trips.
- **Path bounds (local):** tools are confined to `$HOME` (+ `ALLOWED_PATHS`); paths outside (e.g. `/tmp`) are rejected (`pathValidationFailed`).

## Worked example — the mindset in action

> *Task: "Where is request retry/backoff implemented in this checkout, and who calls it?"*

1. **Orient cheaply, don't read yet.** `localSearchCode mode:"discovery" keywords:["retry","backoff"]` → 38 files. Too broad to read — **narrow, don't page.** Re-run with `langType:"ts"` + `excludeDir:["node_modules","dist","tests"]` → 3 files. *Decision: a discovery hit count this high means my query is wrong, not that I should paginate.*
2. **Read the smallest slice.** On the top file, `localGetFileContent minify:"symbols"` → skeleton shows `fetchWithRetries(...)` at line 84. *Decision: skeleton first, never `fullContent` on a file I haven't seen.*
3. **Land on the real line.** `localGetFileContent matchString:"fetchWithRetries" contextLines:20` → `matchRanges[0].start = 84` (a true `lineHint`) plus the body. Now I can quote it `file:84`.
4. **Prove the blast radius.** `lspGetSemantics(uri, "fetchWithRetries", lineHint:84, type:"callers")` → empty. *Decision: empty ≠ unused — this is open-file scope.* Re-issue **batched in one call**: a `documentSymbols` query on the likely consumer (`src/github/client.ts`) **plus** the `callers` query, so the consumer loads first → now 4 callers resolve.
5. **Stop at the gate.** `evidence.answerReady:true` → answer: *"Retry/backoff is `fetchWithRetries` (`src/utils/http.ts:84`), exponential backoff, called by 4 sites in `src/github/*` (cited)."* No confirming re-read.

The thinking, distilled: **high hit count → narrow; unknown file → skeleton; need a quote/line → `matchString`; empty relational result → reconsider scope before concluding; `answerReady` → stop.**

---

# Part A — Local tools

## A1. Which tool

| You want | Use |
|---|---|
| Which files contain X (orient) | `localSearchCode` `mode:"discovery"` (paths) or `countLinesPerFile:true` (hit counts) |
| X with snippets | `localSearchCode` (default `mode:"paginated"`) |
| X with surrounding context | `localSearchCode` `mode:"detailed"` |
| A code **shape** regex can't express | `localSearchCode` `mode:"structural"` + `pattern` or `rule` |
| Read a file / region | `localGetFileContent` (start with `minify:"symbols"`) |
| Map a directory | `localViewStructure` |
| Find files by name / size / mtime / regex | `localFindFiles` |
| Symbol identity / callers / all usages | `lspGetSemantics` (needs a real `lineHint`) |
| Look inside an archive / compressed file / binary | `localBinaryInspect` (needs `ENABLE_LOCAL=true`) |

## A2. Core flow: find → prove → read

```
localSearchCode (text or mode:"structural")        find the line → matches[].line
        │
        ▼  lineHint
lspGetSemantics(uri, symbolName, lineHint)         prove identity / blast radius
        │
        ▼
localGetFileContent(startLine/endLine | matchString)   read only what you must
```

Every LSP call except `documentSymbols` needs `symbolName` + a `lineHint` from a **real match** (`localSearchCode` `matches[0].line`, `localGetFileContent` `matchRanges[0].start`, or a structural match's `line`). **Never guess a `lineHint`** — a wrong value silently returns nothing.

## A3. `localSearchCode` — text search

- **Discovery first** on unknown trees: `mode:"discovery"` or `countLinesPerFile:true` (near-zero tokens), then re-run `paginated` with `include:[…]` scoped to the files found.
- Narrow before paging noise: `langType` (`ts`/`py`/`go`/…) beats `include` globs; `excludeDir:["node_modules","dist"]` skips whole trees.
- `filesWithoutMatch:true` → files **missing** a required import/header. `invertMatch:true` → non-matching lines.
- `perlRegex:true` only for lookaheads/backreferences; `fixedString:true` for literals.
- **Pagination:** files → `itemsPerPage`+`page`; matches in one noisy file → `maxMatchesPerFile`+`matchPage`.

## A4. `localSearchCode` — `mode:"structural"` (AST)

The layer between text and semantics: shape queries ripgrep gives false positives on (e.g. a call only outside comments/strings). `keywords` is ignored; supply **exactly one** of `pattern` or `rule`.

**`pattern`** — a code-shaped fragment. Metavars: `$X` = one node (captured), `$$$ARGS` = a node list.
- `eval($X)` → real `eval` call sites only, never comments/strings.
- `$X` is a **single** argument: `foo($X)` matches `foo(1)`, not `foo(1,2)` — use `foo($$$A)` for any arity.
- A bare-identifier call doesn't match a member call: `eval($X)` ≠ `window.eval(x)` — use `$F($X)` or `$$.eval($X)`.

**`rule`** — YAML for what patterns can't express (`not`/`inside`/`has`/`all`/`any`):
```yaml
rule:
  pattern: await $C
  inside:
    kind: for_in_statement   # check the grammar's node kind (TS for-of = for_in_statement)
    stopBy: end              # REQUIRED — without it the sub-rule silently matches nothing
```
- **`stopBy: end` is the top gotcha:** relational sub-rules (`inside`/`has`) only check the immediate parent/child unless you add it.

**Make it fast — give the pattern a literal token.** A literal (e.g. `eval`, `validateToolPath`) auto-becomes a text anchor that skips *parsing* files that can't match. A metavar-only pattern (`$A.$B($C)`) has no anchor → it parses every candidate file and warns. Scope further with `path`, `include:["*.ts"]`, `excludeDir`, `maxFiles` (here `maxFiles` caps files **enumerated**). Pagination matches text mode.

**Grammars:** ts/tsx, js/jsx/mjs/cjs, py, go, rs, java, c/h, cpp/cc/cxx/hpp, cs, sh/bash/zsh. Other extensions are skipped silently.

**Go nuance:** a literal-selector pattern like `fmt.Println($X)` matches nothing (a bare snippet is invalid at Go top level, so it can't be parsed — inherent to ast-grep). Use a metavar callee `$F($X)`, or a rule with `pattern: { context: "func f(){ fmt.Println($X) }", selector: call_expression }`.

## A5. `localGetFileContent` — read & minify

Pick `minify` by goal — `"symbols"` (smallest: skeleton + `NNN|` gutter) to **orient first** on any unknown file · `"standard"` (default: strips comments/blank lines) to read · `"none"` (raw) to quote/diff exact text.

**Extraction modes (mutually exclusive):**
- `matchString` — anchor by text; returns `matchRanges` (1-based) → `matchRanges[0].start` is an LSP `lineHint`. Add `contextLines` to capture a whole body in one read.
- `startLine`/`endLine` — a known range (both required).
- `fullContent` — small files only.
- default — first `charLength` chars.

**Pagination:** set `charLength` on files over ~200 lines; when `isPartial:true`, take the next offset from the `hints[]` value (`Next: charOffset=…`).

## A6. `localViewStructure` — map a directory

Map before searching. On monorepos start shallow (`recursive:true` + `maxDepth:1`), then drill. `sortBy:"size"`+`details:true` spots large files; `sortBy:"time"` shows recent churn; `extensions:["ts"]` filters type. Paginates with `itemsPerPage`+`page`. Prefer `localFindFiles` when you need metadata filters (size/mtime) or a name regex.

## A7. `localFindFiles` — find by metadata

Cheaper than `localSearchCode` when you only need locations:
- `regex:"^(index|main)\\.(ts|js)$"` (basename, precise) or `names:["*.test.ts"]` (globs, OR); `pathPattern:"packages/*/src/**"` to slice a monorepo.
- `modifiedWithin:"24h"` + `showFileLastModified:true` + `sortBy:"modified"` → what changed recently.
- `sizeGreater:"5k"` + `sortBy:"size"` → largest first. `entryType:"f"|"d"`. Default `excludeDir` covers node_modules/dist/.git/build; pass `[]` to search everything.

## A8. `localBinaryInspect` — archives, compressed streams, binaries

Needs `ENABLE_LOCAL=true`. Unpacks/inspects only — never searches across entries or reads plain text. Pick `mode`; there is **no default**.

```
identify   → what is this file?            type + magic bytes; start here when unsure
list       → archive entry names           .zip/.jar/.tar.*/.7z…; page entriesPerPage + entryPageNumber
extract    → one entry's content           archiveFile = an exact name from a prior list
decompress → a single-stream file's text   .gz/.bz2/.xz/.zst…; + matchString filtering
strings    → printable runs from a binary  .so/.dylib/.node/.exe; minLength + includeOffsets
```

- **`list` before `extract`** — `archiveFile` must be an exact entry name (case-sensitive; must not start with `-`). `extract` without `archiveFile`, and `decompress` on a multi-entry archive, are rejected with guidance.
- `decompress` is single-stream only. Override detection with `format:` only when the extension lies.
- `strings`: raise `minLength` (12–16) for symbols/URLs/versions only; `includeOffsets:true` gives hex offsets to pivot on.
- Pagination: `list` → `entriesPerPage`+`entryPageNumber` (`totalEntries` = true count); the rest char-paginate via `charOffset` from `hints[]`.
- **Gotcha:** `extract` on `.tar.*`/`.7z` requires the `7z` binary on PATH (`.zip` does not); without it → `spawn 7z ENOENT`.

## A9. `lspGetSemantics` — semantic navigation

`uri` always; `symbolName` + `lineHint` for every type except `documentSymbols`.

| `type` | Returns |
|---|---|
| `documentSymbols` | file outline (no `lineHint` needed) |
| `definition` / `typeDefinition` | declaration site / the type's definition |
| `hover` | signature + JSDoc |
| `references` | usages (same-package) |
| `callers` / `callees` / `callHierarchy` | incoming / outgoing / both directions |
| `implementation` | concrete implementations of a member |

- **Open-file scope:** `references`, `callers`, `implementation` are bounded by the files the server has open — an **empty result ≠ "unused"**. Batch a `documentSymbols`/`definition` query on the likely consumer file **in the same call** so it loads, then the relational query resolves. For broad blast-radius, prefer `callers`.
- **Language tiers:** `callers`/`callees`/`callHierarchy` are TS/JS/Go/Rust only; Python/C++ have no call hierarchy → use `references`. Shell/HTML/CSS/YAML get only `documentSymbols`/`hover`/`definition`.
- **Cheaper output:** `format:"compact"` on call-flow queries; `groupByFile:true` on `references` (per-file `lines[]`, each a follow-up `lineHint`); `contextLines` embeds call-site source; `depth` follows chains.
- **Signals:** `resolvedSymbol.foundAtLine` far from your `lineHint` → re-anchor with search. `kind:"empty"` + `reason` distinguishes `symbolNotFound` (re-anchor) from `serverUnavailable` (fall back to search) from no-locations (open-file scope).

---

# Part B — GitHub tools

## B1. Which tool

| You want | Use |
|---|---|
| Research an npm package's source (smart entry) | `npmSearch` → `repository` + `repositoryDirectory` |
| Find repos by name / topic / popularity / owner | `ghSearchRepos` (`verbose:false` first) |
| List every repo an org owns | `ghSearchRepos` with `owner` and no `keywords` |
| Map an unknown repo's layout | `ghViewRepoStructure` (`maxDepth:1`, then drill) |
| Confirm a file exists before reading | `ghSearchCode` `match:"path"` (cheapest — no snippets) |
| Find which files mention X (discovery) | `ghSearchCode` `match:"file"` (snippets) |
| Read / orient on a file | `ghGetFileContent` (start `minify:"symbols"`) |
| Quote exact code or get a `lineHint` | `ghGetFileContent` `matchString` / `minify:"none"` |
| Why/when/who changed code (PRs) | `ghHistoryResearch` `type:"prs"` |
| Commit log for a file / dir / repo | `ghHistoryResearch` `type:"commits"` |
| Deep multi-file analysis in one repo | `ghCloneRepo` then local + `lspGetSemantics` |

## B2. Core flow: locate → map → read → prove

```
npmSearch (have a package)  ─┐
ghSearchRepos (have a concept)├─ get owner/repo
ghViewRepoStructure (maxDepth:1) ─ map (cheap)
ghSearchCode ─ discovery hint, NOT proof (matchIndices = char offset in snippet, NOT a line number)
        │
        ▼  ghGetFileContent(matchString=same-keyword) → matchRanges[0].start = REAL line number = lineHint
        ▼
lspGetSemantics(uri, symbolName, lineHint) ─ prove identity / blast radius (after ghCloneRepo)
ghHistoryResearch ─ why it got that way (PR rationale, commit archaeology)
```

Entry point: have a **package name** → `npmSearch` (resolves `repository` + monorepo `repositoryDirectory`). Have a **concept** → `ghSearchRepos`. Already know `owner/repo` → skip both, go to `ghViewRepoStructure`. **Never guess `owner/repo`.**

## B3. Best flows (copy these)

**A — npm package source:**
```
npmSearch(packageName:"@tanstack/react-query")
   → repository:"TanStack/query", repositoryDirectory:"packages/react-query"
ghViewRepoStructure(owner:"TanStack", repo:"query", path:"packages/react-query", maxDepth:1)   ← scope to the subpackage
ghGetFileContent(path:"packages/react-query/src/index.ts", minify:"symbols")                    ← orient
ghGetFileContent(path:…, matchString:"useQuery", minify:"none")                                 ← read/quote
```
`repositoryDirectory` jumps you straight into the right monorepo subfolder — the single biggest efficiency win for package research.

**B — find code → land on the line → read → prove** (canonical):
```
ghSearchCode(owner, repo, keywords:["createStore"])      ← discovery; snippet offsets are NOT lines
ghGetFileContent(path, matchString:"createStore")         ← matchRanges[0].start = REAL lineHint
ghGetFileContent(path, startLine, endLine, minify:"none") ← read/quote the exact body
lspGetSemantics(uri, "createStore", lineHint)             ← prove (after ghCloneRepo)
```

**C — concept → repo → layout → code** (no package name):
```
ghSearchRepos(keywords, language, stars:">5000", verbose:false)   ← lean discovery
ghViewRepoStructure(owner, repo, maxDepth:1) → drill into src      ← map
ghSearchCode(owner, repo, match:"path", keywords)                  ← confirm a file exists (cheapest)
ghGetFileContent(path, minify:"symbols")                           ← orient, then read
```

**D — why/when did this change?** (PR & commit archaeology):
```
ghHistoryResearch(type:"commits", owner, repo, path:"src/x.ts")    ← messageHeadline embeds "(#3391)"
ghHistoryResearch(type:"prs", owner, repo, prNumber:3391, reviewMode:"full")  ← body+diff+comments+reviews in ONE call
```
For "which PR introduced X": `type:"prs"`, `state:"merged"`, `sort:"created"`, `order:"asc"` → oldest merged first.

## B4. `npmSearch` — package → repo+path handoff

The cheapest, most accurate way to start *package* research.
- **Exact name** (`"react"`, `"@octokit/rest"`) → one rich result: `version`, `license`, `weeklyDownloads`, `repository`, and a `Browse source` hint.
- **`repositoryDirectory`** — present for monorepo packages. **Pass it as `path=` to `ghViewRepoStructure`/`ghSearchCode`** to scope straight to the subpackage.
- **Keyword query** (`"http client typescript"`) → lean ranked list; re-run with an exact name for full source details.
- Scoped packages need the full scope (`@octokit/rest`, not `rest`). An empty exact-name result usually means a typo or a private package.

## B5. `ghSearchRepos` — discover repos

- **Lean first.** `verbose:false` (default) returns one pipe-string per repo: `owner/repo | stars | forks | issues | lang | pushed | #topics | description`. `verbose:true` only to filter/sort programmatically.
- **Owner semantics:** `owner` alone enumerates an org's repos; `owner`+`keywords` scopes to them; `keywords` alone searches across GitHub.
- **AND vs OR:** `topicsToSearch` is strict AND and *sparse* — pair with `keywords`/`language`. Both `topicsToSearch` **and** `keywords` fires two searches merged with OR; for strict AND use one keyword set.
- **GitHub range syntax:** `stars:">5000"`, `forks:"50..500"`, `created:">2023-01-01"`, `updated:">2024-01-01"` (`updated` maps to `pushed:`). `sort`: `stars`/`forks`/`updated`/`help-wanted-issues`/`best-match`.

## B6. `ghViewRepoStructure` — map a tree

Map before searching. Start shallow (`maxDepth:1`), then drill (`path:"src"`, `maxDepth:1`). Output `structure[]` = `{dir, files[], folders[]}` + `summary` + `resolvedBranch`. Artifacts (`node_modules`, `.git`, `dist`, `build`) auto-excluded. `includeSizes:true` → `fileSizes`. `branch` accepts a tag/SHA; a missing ref silently falls back to default with a warning. Faster than `ghSearchCode match:"path"` for the *whole* layout.

## B7. `ghSearchCode` — code & path search (discovery only)

> ⚠️ **GitHub is deprecating this API (planned removal ~Sep 2026).** For known paths prefer `ghGetFileContent` / `ghViewRepoStructure`. Treat it as a hint generator, **never as proof.**

- **`match:"path"` is the cheapest call in the suite** — searches paths only, no snippet payload. Use it to confirm a file exists.
- **`match:"file"`** (default) searches contents → `matches[].value` (snippet) + `matchIndices` (char offsets, **not** lines). Matches comments/strings/docs too — a hit in `docs/*.md` is not a definition. Re-anchor with `ghGetFileContent(matchString=…)` for a real line number.
- **Keywords are ANDed** (every term must appear). Put alternatives in separate query objects. `filename`/`extension`/`language`/`path` narrow scope; `repo` requires `owner`. `verbose:true` adds a SHA-pinned `html_url`.
- **Hard caps (GitHub's):** **20 results max per code search**; ~1000 / 10 pages total. Indexes the **default branch only**. (See Universal "Empty ≠ absent.")

## B8. `ghGetFileContent` — read & minify (the proof tool)

`minify` by goal — `"symbols"` (skeleton + `NNN|` gutter, **never paginated**) to orient · `"standard"` (default) to read · `"none"` (raw) to quote/diff.

**Extraction modes (mutually exclusive — `fullContent` XOR `matchString` XOR `startLine`/`endLine`):**
- `matchString` — returns `matchRanges[]` with **1-based line numbers** → `matchRanges[0].start` is a valid LSP `lineHint`. Add `contextLines` (≤100) to capture a whole body. `matchStringIsRegex`/`matchStringCaseSensitive` available. Ignored when `minify:"symbols"`.
- `startLine`/`endLine` — both required, `endLine ≥ startLine`. · `fullContent` — small files only. · default — first `charLength` chars.

**Pagination:** char-window via `charLength` (set on files over ~200 lines); response carries `pagination` + a `Next: charOffset=N` hint. `symbols` returns the whole skeleton unpaginated.

**`type:"directory"`** materializes a subtree to disk for LSP work — **clone-gated** (`ENABLE_LOCAL=true` + `ENABLE_CLONE=true`). `forceRefresh:true` bypasses cache; `warnings[]` flags sanitized content or a `symbols`→`standard` fallback.

## B9. `ghHistoryResearch` — PRs + commit history

**`type:"prs"` — LIST mode** (no `prNumber`): search by `keywordsToSearch`+`match:["title"]` (most precise) or raw `query`. Filter by `state`/`author`/`label`/`review`/`checks`/`base`/dates. Returns lean metadata (`number`, `title`, `state`, `author`, dates, counts) — not file contents.

**DETAIL mode** (`prNumber` required): select surfaces via `content{body,changedFiles,patches,comments,reviews,commits}`, or `reviewMode:"full"` for **all surfaces in one call** (body + files + patches + threaded comments with `in_reply_to_id` + reviews + commits + `reviewSummary`). Selectors are silently ignored without `prNumber`. `patches.mode`: `"none"` / `"selected"` (`files[]` or per-file `ranges` — cheapest) / `"all"`. Bot comments hidden by default → `content.comments.includeBots:true`.

**`type:"commits"`:** `owner`+`repo` required; `path`=file, `path` ending `/`=subtree, omit=whole repo. Returns `sha`, `message`, `messageHeadline`, `author`, `date`, `url`. `since`/`until`/`author` filter; `includeDiff:true` adds patches. **`messageHeadline` often embeds a PR ref** like `(#3391)` — extract it and re-call DETAIL mode for the full rationale.

**Pagination:** list search exposes `reportedTotalMatches` vs **`reachableTotalMatches`** (GitHub's 1000-cap) — trust *reachable* for completeness claims. `contentPagination` lists every surface with `hasMore:true` and a ready `nextQuery`.
