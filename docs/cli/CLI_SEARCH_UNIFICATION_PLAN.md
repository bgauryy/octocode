# CLI Search Unification Plan

> One command — `search` — for all read-only code research via OQL.  
> Side-effecting actions (`clone`, `cache`, `unzip`) stay as named commands.

---

## Scope

Commands that `search` replaces:

| Command | OQL target |
|---|---|
| `grep` | `code` · `files` |
| `cat` | `content` |
| `ls` | `structure` · `semantics` (documentSymbols) |
| `lsp` | `semantics` |
| `repo` | `repositories` |
| `pr` | `pullRequests` · `diff` (PR lane) |
| `history` | `commits` |
| `pkg` | `packages` |
| `binary` (inspect/list/strings/extract/decompress) | `artifacts` |
| `diff` | `diff` (needs new local lane — see §Blockers) |

Commands that **stay unchanged** (write to disk):  
`clone` · `cache` · `unzip` · `binary --unpack`

---

## OQL Language Rating

### Strengths

| Area | Score | Notes |
|---|:---:|---|
| Semantic coverage | 10/10 | 14 active targets; all 10 read-only operations map cleanly |
| Type safety | 9/10 | Discriminated unions per target; typed `params`, `where`, `fetch` per lane |
| Agent ergonomics | 9/10 | JSON-first, structured output, `next.*` pre-filled continuations, `--explain` routing |
| Orthogonality | 9/10 | `where` / `scope` / `fetch` / `controls` are independent axes |
| Evidence model | 10/10 | `answerReady`, `kind`, `proofGrade` per row — unique in the field |
| Batch | 8/10 | 1–5 independent queries in one call |
| Diagnostics | 9/10 | Typed codes, repair hints, routing transparency via `PUSHDOWN/RESIDUAL/ROUTE` |

### Weaknesses

| Area | Score | Problem |
|---|:---:|---|
| CLI shorthand coverage | 5/10 | `buildSugar()` passes only 9/~50 needed fields today |
| `--type` collision | 4/10 | Same flag means language filter (`ts`) **and** LSP operation (`references`) — confirmed in `shorthand.ts` |
| Target auto-detection | 5/10 | No file-vs-directory routing; shorthand always emits `target:"code"` |
| Schema completeness | 7/10 | `contextLines` and `invertMatch` missing from `controls.search` |
| Local diff | 0/10 | No local-file diff lane in `diffLanes.ts` |
| `--mode` overload | 5/10 | Means search view in grep context AND content minification in cat context |

### Verdict

**OQL as a research language: 9/10.**  
The schema is complete, typed, and well-designed. It is the right language.

**OQL CLI shorthand today: 5/10.**  
The bottleneck is the shorthand layer, not the language. The `--type` collision and missing
auto-routing are design issues that must be resolved before `search` can absorb the quick commands.

---

## Design Issues to Resolve First

### 1. `--type` collision

Current state in `shorthand.ts`:
- `--type ts` → `scope.language` / `langType` (language filter for grep/cat)
- `lsp --type references` → `params.type` (LSP operation)

Both land on the same `--type` flag in a unified command. Values are disjoint
(`ts`/`js`/`py` vs `references`/`callers`/`definition`) but conflating them is fragile.

**Resolution:** split into two flags:
- `--lang <ext|language>` — scope language / extension filter (rename from `--type` for grep/cat/ls)
- `--op <lsp-type>` — LSP operation type (`definition`, `references`, `callers`, …)

`--lang` is already a flag (structural engine language for `--pattern`/`--rule`), so the rename
unifies: `--lang ts` means "TypeScript" in both structural and filter contexts.  
`--op` is new and unambiguous.

### 2. Target auto-routing from positional

The shorthand must infer `target` from the positional argument:

| Positional resolves to | Emitted target |
|---|---|
| local file | `content` |
| local directory | `code` (default) or `structure` (with `--tree`) |
| `owner/repo` GitHub ref | `code` (GitHub search) |
| local file + `--op <lsp-type>` | `semantics` |
| local dir + `--tree` | `structure` |

### 3. `--mode` overload

| Context | `--mode` today | Resolution |
|---|---|---|
| grep search view | `paginated\|discovery\|detailed` | rename to `--view` |
| cat content minification | `none\|standard\|symbols` | keep `--mode` |

Values are disjoint; a single flag could self-dispatch by value, but explicit rename is cleaner.

---

## Schema Gaps (must fix before removing `grep`)

**File:** `packages/octocode-tools-core/src/oql/types.ts`  
**File:** `packages/octocode-tools-core/src/oql/adapters/local.ts:searchControls()`

| Add to `controls.search` | Passes to `localSearchCode` | `grep` flag |
|---|---|---|
| `contextLines?: number` | `contextLines` | `--context-lines` |
| `invertMatch?: boolean` | `invertMatch` | `--invert-match` |

---

## Blockers (cannot remove until fixed)

| Blocker | Fix |
|---|---|
| **`diff` local lane** | `diffLanes.ts` only has `prPatch` (GitHub PR) and `directFile` (GitHub two-ref). Add `localFile: { leftPath, rightPath }` → `localGetFileContent` × 2 + line diff in the diff adapter. |

---

## CLI Shorthand — Complete Flag Map

All changes in `search.ts` options array + `buildSugar()` in `shorthand.ts`.

### grep → `target:"code"` / `target:"files"`

| Flag | OQL field | Schema gap? |
|---|---|:---:|
| `--case-insensitive` | `where.text.case:"insensitive"` | |
| `--case-sensitive` | `where.text.case:"sensitive"` | |
| `--whole-word` | `where.text.wholeWord:true` | |
| `--fixed` | text predicate (already literal; alias) | |
| `--multiline` | `where.regex.multiline:true` | |
| `--multiline-dotall` | `where.regex.dotAll:true` | |
| `--files-only` | `filesOnly:true` | |
| `--files-without-match` | `filesWithoutMatch:true` | |
| `--concise` | `view:"discovery"` | |
| `--view paginated\|discovery\|detailed` | `view` | |
| `--count-lines` | `controls.search.countLinesPerFile` | |
| `--count-matches` | `controls.search.countMatchesPerFile` | |
| `--only-matching` | `controls.search.onlyMatching` | |
| `--unique` | `controls.search.unique` | |
| `--count` | `controls.search.countUnique` | |
| `--match-window <n>` | `controls.search.matchWindow` | |
| `--match-length <n>` | `controls.search.matchContentLength` | |
| `--max-matches <n>` | `controls.search.maxMatchesPerFile` | |
| `--match-page <n>` | `controls.search.matchPage` | |
| `--max-files <n>` | `controls.budget.maxFiles` | |
| `--hidden` | `scope.hidden` | |
| `--no-ignore` | `scope.noIgnore` | |
| `--include <glob>` | `scope.include` | |
| `--exclude <glob>` | `scope.exclude` | |
| `--page <n>` | `page` | |
| `--page-size <n>` | `itemsPerPage` | |
| `--limit <n>` | `limit` | |
| `--branch <ref>` | `from.ref` | |
| `--force-refresh` | `materialize.forceRefresh` | |
| `--context-lines <n>` | `controls.search.contextLines` | ✅ new |
| `--invert-match` | `controls.search.invertMatch` | ✅ new |

### cat → `target:"content"` (auto-routed when positional is a file)

| Flag | OQL field |
|---|---|
| `--mode none\|standard\|symbols` | `fetch.content.contentView:"exact"\|"compact"\|"symbols"` |
| `--match-string <s>` | `fetch.content.match.text` |
| `--match-regex` | `fetch.content.match.regex` |
| `--match-case-sensitive` | `fetch.content.match.caseSensitive` |
| `--start-line <n>` | `fetch.content.range.startLine` |
| `--end-line <n>` | `fetch.content.range.endLine` |
| `--context-lines <n>` | `fetch.content.range.contextLines` |
| `--char-offset <n>` | `fetch.content.charOffset` |
| `--char-length <n>` | `fetch.content.charLength` |
| `--full-content` | `fetch.content.fullContent` |
| `--raw` | render-layer only: print `row.content` body, no YAML |

### ls → `target:"structure"` / `target:"semantics"`

`--symbols` switches target to `semantics` + `params.type:"documentSymbols"`.

| Flag | OQL field |
|---|---|
| `--symbols` | `target:"semantics"`, `params.type:"documentSymbols"` |
| `--kind <kind>` | `params.kind` (post-filter) |
| `--depth <n>` | `fetch.tree.maxDepth` |
| `--include <glob>` | `scope.include` |
| `--ext <list>` | `scope.include` globs |
| `--files-only` | field predicate `entryType:file` |
| `--dirs-only` | field predicate `entryType:directory` |
| `--sort <field>` | `controls.search.sort` |
| `--hidden` | `scope.hidden` |
| `--tree` | force `target:"structure"` on a directory |

### lsp → `target:"semantics"`

| Flag | OQL field |
|---|---|
| `--op <lsp-type>` | `params.type` (renamed from `--type`) |
| `--symbol <name>` | `params.symbolName` |
| `--line <n>` | `params.lineHint` |
| `--workspace-root <path>` | `params.workspaceRoot` |
| `--depth <n>` | `params.depth` |
| `--format structured\|compact` | `params.format` |

### repo → `target:"repositories"`

| Flag | OQL field |
|---|---|
| `--topic <list>` | `params.topicsToSearch` |
| `--lang <lang>` | `params.language` |
| `--owner <owner>` | `params.owner` |
| `--stars <range>` | `params.stars` |
| `--sort <field>` | `params.sort` |
| `--limit <n>` | `limit` |
| `--concise` | `view:"discovery"` |

### history → `target:"commits"`

| Flag | OQL field |
|---|---|
| `--since <iso>` | `params.since` |
| `--until <iso>` | `params.until` |
| `--author <name>` | `params.author` |
| `--branch <ref>` | `params.branch` |
| `--diff` | `params.includeDiff` |
| `--limit <n>` | `limit` |

### pr → `target:"pullRequests"`

| Flag | OQL field |
|---|---|
| `--pr <n>` | `params.prNumber` |
| `--state open\|closed\|merged` | `params.state` |
| `--author <user>` | `params.author` |
| `--patches` | PR patch mode |
| `--comments` | `params.commentPage:1` |
| `--commits` | `params.commitPage:1` |
| `--deep` | all content flags |
| `--file <path>` | `params.files:[path]` |
| `--match-string <s>` | `params.matchString` |
| `--concise` | `view:"discovery"` |

### pkg → `target:"packages"`

| Flag | OQL field |
|---|---|
| `--mode lean\|full` | `params.mode` |
| `--page <n>` | `page` |

### binary (read modes) → `target:"artifacts"`

| Flag | OQL field |
|---|---|
| `--inspect` | `params.mode:"inspect"` |
| `--list` | `params.mode:"list"` |
| `--strings` | `params.mode:"strings"` |
| `--extract <entry>` | `params.mode:"extract"`, `params.archiveFile` |
| `--decompress` | `params.mode:"decompress"` |
| `--detailed` | `params.verbose` |
| `--match <s>` | `params.matchString` |
| `--min-length <n>` | `params.minLength` |

---

## Render Gap

| Gap | Fix |
|---|---|
| `--raw` (body-only stdout) | When `target:"content"` + `--raw`: print `row.content` directly, suppress YAML envelope. Render-layer only — no OQL field needed. |

---

## Implementation Order

Dependencies: Phase 2 requires Phase 1. Phases 3–5 are independent after Phase 1.

### Phase 1 — Design fixes (prerequisite for everything)
1. Resolve `--type` collision: rename language filter to `--lang`, introduce `--op` for LSP type
2. Resolve `--mode` overload: rename search-view flag to `--view`
3. Implement positional auto-routing (`file` → `content`, `--op` present → `semantics`, `--tree` → `structure`)

### Phase 2 — Schema additions
4. Add `contextLines` + `invertMatch` to `controls.search` in `oql/types.ts`
5. Wire in `oql/adapters/local.ts:searchControls()`

### Phase 3 — Zero-risk flag porting (no schema deps)
6. `repo` · `pkg` · `history` · `pr`

### Phase 4 — Binary read modes
7. `binary` (inspect/list/strings/extract/decompress)

### Phase 5 — LSP + structure
8. `lsp` → `--op` + §lsp flags
9. `ls` → `--symbols` switch + §ls flags

### Phase 6 — Search + content (after Phase 1 + 2)
10. `grep` → full §grep flag set
11. `cat` → full §cat flag set + `--raw`

### Phase 7 — Diff blocker
12. Add `localFile` lane to `diffLanes.ts`
13. Remove `diff` command

---

## Files Changed

| File | Change |
|---|---|
| `oql/types.ts` | `controls.search.contextLines`, `controls.search.invertMatch` |
| `oql/adapters/local.ts` | `searchControls()`: wire two new fields |
| `oql/diffLanes.ts` | Add `localFile` lane |
| `oql/adapters/` | Local diff execution |
| `oql/schemeText.ts` | Document new fields |
| `oql/shorthand.ts` | `buildShorthandInput()`: `--lang` rename, auto-routing, all new fields |
| `search.ts` | Options array, `buildSugar()`, `--raw` renderer |
| `docs/OCTOCODE_QUERY_LANGUAGE.md` | Controls table, diff lanes, `--op` / `--lang` |

**Not changed:** `clone.ts` · `cache.ts` · `unzip.ts`  
**Deleted per phase:** `grep.ts` · `cat.ts` · `ls.ts` · `lsp.ts` · `repo.ts` · `pr.ts` · `history.ts` · `pkg.ts` · `binary.ts` · `diff.ts`
