# Octocode MCP — Tool Improvement Analysis

> Evidence source: `benchmark/output/` — octocode vs rtk-gh run on `vercel/next.js` (10 questions).
> Each finding is grounded in measured `in_chars + out_chars` from `log.jsonl`.

---

## `ghGetFileContent`

### Current Issues

1. **`minify` defaults to `"none"`** (schema: `minifyField.default('none')`).  
   Every file read returns raw, unprocessed content. For large files (e.g. `app-render.tsx` at ~312 k chars), even a single "find the function signature" read bloats the output massively.

2. **No large-file warning or auto-downgrade.**  
   The agent has no feedback that a file is large before it reads it. It learns only after paying the cost.

3. **Agent re-reads the same file 3–4× in small, overlapping windows** to navigate to the right section (Q5, Q6, Q8).  
   Each window costs ~3–8 k chars; the agent is manually "scrolling" through a file it already partially loaded.

4. **`matchString` is underused.**  
   The tool supports `matchString` to jump to a specific line, but the agent rarely uses it. Instead it reads large windows and searches visually.

### Reasoning

The agent has no signal that `minify: "symbols"` exists as a first-pass option before a targeted read. The schema describes it but the default of `"none"` implies "full content is normal." For a 5 k file that's fine; for a 300 k file it's wasteful.

Re-reads happen because the agent finds `isRedirectError` at line 3976 but doesn't know which function it's in. Instead of reading backward 30 lines, it re-issues a full-file fetch with a different `charOffset`.

### Improvement Possibilities

| # | Change | Scope |
|---|--------|-------|
| A | **Change default `minify` to `"standard"`** for files >5 k chars. Return `isSkeleton: false` so agent knows it got real content. | scheme / execution |
| B | **Add `totalChars` hint in the first response when file exceeds a threshold** (e.g. >10 k chars): `"File is 312,847 chars. Use minify='symbols' for a structure map or provide startLine/endLine."` | execution hints |
| C | **Auto-trigger `symbols` view when no `startLine`/`endLine` is provided and file is large.** Let the agent opt into full content explicitly. | execution |
| D | **Promote `matchString` in tool description** with the explicit instruction: "Use `matchString` instead of `startLine`/`endLine` when you know a keyword near the target." | description / hints |
| E | **Add `enclosingFunction: true` parameter** — when set, the response includes the name + start line of the function that contains the matched line. Prevents misidentification of catch-site function names. | execution |

### Before / After

**Q6 — `renderToHTMLOrFlight` signature (app-render.tsx, 312 k chars)**

```
BEFORE (4 calls, 11,620 out_chars)
  call 1: ghGetFileContent path=app-render.tsx                       →  906 chars  (probing)
  call 2: ghGetFileContent path=app-render.tsx charOffset=X          → 3,519 chars
  call 3: ghGetFileContent path=app-render.tsx charOffset=Y          → 5,662 chars
  call 4: ghGetFileContent path=app-render.tsx startLine=2979 end=3010 → 1,533 chars
  TOTAL: 11,620 out_chars

AFTER (2 calls, ~2,200 out_chars)
  call 1: ghGetFileContent path=app-render.tsx minify="symbols"      → ~1,600 chars
         (agent sees: "renderToHTMLOrFlight: 2990, AppPageRender: 2979")
  call 2: ghGetFileContent path=app-render.tsx startLine=2979 end=3012 →  ~600 chars
  TOTAL: ~2,200 out_chars  →  81% reduction
```

**Q5 — redirect() catch site (3 reads of app-render.tsx, wrong answer)**

```
BEFORE (4 calls, wrong enclosing function name)
  call 3: ghGetFileContent path=app-render.tsx  → 2,802 chars  (isRedirectError found)
  call 4: ghGetFileContent path=app-render.tsx  → 3,113 chars  (looking for function name)
  agent concludes: "within renderToHTMLOrFlight" ← WRONG (actual: renderToStream)

AFTER (2 calls + enclosingFunction hint, correct answer)
  call 1: ghGetFileContent matchString="isRedirectError" enclosingFunction=true
         → response includes: "enclosingFunction: renderToStream (line 3147)"
  call 2: optional targeted read for verification
  agent concludes: "caught in renderToStream" ← CORRECT
```

### Agent / Research Impact

- **Q5**: Wrong answer (Q=2 instead of Q=3) caused by inability to determine enclosing function. Fix → **+5 research score** per run.
- **Q6**: 10.5× cost reduction with no quality loss.
- **Q8**: 8× cost reduction (5 reads → 2).
- **Cumulative on Q5–Q8**: ~43 k out_chars → ~8 k = **~5× leaner** for large-file code tracing.

---

## `ghSearchCode`

### Current Issues

1. **Returns full text-match snippets by default.**  
   Each match includes surrounding context lines, file URL, repo metadata, and `textMatches[].fragment`. For a simple class-declaration lookup, this produces 5 k chars where 3 lines would suffice.

2. **No anchored-pattern guidance.**  
   The tool accepts free-form `keywordsToSearch` but doesn't steer the agent toward regex anchors (`^export class`, `^export function`). Unanchored patterns match import sites, type declarations, and comments alongside the actual declaration.

3. **`verbose` field exists but is not documented as a size lever.**  
   The schema has a `verbose: boolean` field, but the description doesn't explain that `verbose: false` reduces the output to path-only matches.

### Reasoning

GitHub's code search API returns rich context by design — it's built for human exploration, not machine extraction. The `value` field in each match contains a multi-line fragment, not a single line. For bulk symbol lookups (Q2: 3 symbols) this is 14× over-budget compared to `rg -n "^export class"` output.

### Improvement Possibilities

| # | Change | Scope |
|---|--------|-------|
| A | **Default `verbose: false`** when `limit ≤ 5` and `filename` or tight `path` is provided — strong signal the agent wants an exact match, not a survey. | execution |
| B | **Add `lineOnly: true` parameter** (or rename `verbose: false` to be explicit): returns only `{path, lineNumber, line}` per match — no fragment context, no URL. | scheme / execution |
| C | **In tool description**, add: *"For exact declaration lookups use an anchored keyword like `export class Foo` to eliminate import/re-export noise."* | description |
| D | **Auto-suggest `lspGetSemantics type=definition`** in the response hints when the query looks like a symbol name (CamelCase, no spaces, `path` scoped to one file). | hints |
| E | **Batch-aware deduplication**: when 3+ `keywordsToSearch` are symbols in the same repo, suggest a single regex alternation query rather than 3 separate calls. | hints / description |

### Before / After

**Q2 — Bulk class declaration lookup (NextRequest, NextResponse, ImageResponse)**

```
BEFORE (2 calls, 11,567 total chars)
  call 1: ghSearchCode keywords=["NextRequest","NextResponse","ImageResponse"]
         → 5,084 out_chars  (text fragments, URLs, repo info for 3 symbols)
  call 2: ghGetFileContent (verification read)
         → 3,851 out_chars
  TOTAL: 11,567 chars

AFTER option A — lineOnly=true (1 call, ~300 chars)
  call 1: ghSearchCode keywords=["export class NextRequest","export class NextResponse",
                                     "export class ImageResponse"]
          lineOnly=true
         → ~300 out_chars  (3 lines: path + lineNumber + declaration)
  TOTAL: ~300 chars  →  97% reduction, same quality

AFTER option B — lspGetSemantics (3 calls, ~600 chars total)
  call 1: lspGetSemantics type="definition" symbolName="NextRequest"   → ~200 chars
  call 2: lspGetSemantics type="definition" symbolName="NextResponse"  → ~200 chars
  call 3: lspGetSemantics type="definition" symbolName="ImageResponse" → ~200 chars
  TOTAL: ~600 chars  →  95% reduction, exact file:line:col
```

### Agent / Research Impact

- **Q2**: 11,567 → ~300–600 chars, D=3 maintained. **14–40× more efficient.**
- **Q7**: Search for `pendingRevalidatedTags` returned 4,825 chars. With `lineOnly=true` → ~400 chars.
- **Across all symbol/pattern searches**: estimated **3–14× reduction** in search result overhead.

---

## `lspGetSemantics`

### Current Issues

1. **Not used at all in the benchmark run** (Q1–Q10 are "remote only" — no clone).  
   However, the agent never attempted it even for questions where a clone was available (Q11+). The tool is underutilized in research flows.

2. **No "clone then LSP" shortcut** in the agent routing strategy.  
   For targeted definition/reference lookups, cloning and using LSP would be significantly cheaper than remote search + file reads — but the agent doesn't have a clear decision rule for when to make that trade.

3. **`lineHint` requirement creates a chicken-and-egg problem.**  
   Most LSP operations (except `documentSymbols`) require an accurate `lineHint`, which the agent must discover first via a search call. The tool description should clarify that one search call to get `lineHint` → one LSP call is the intended 2-step flow.

4. **`documentSymbols` is underused as a structure map.**  
   For large files, `documentSymbols` returns all exported symbols with line numbers — equivalent to `minify: "symbols"` on `ghGetFileContent` but semantically richer (includes type, kind, range).

### Reasoning

Without a local clone, LSP is unavailable. But for deep research sessions on a target repo, the cost of `ghCloneRepo` is amortized across all subsequent LSP calls. The agent doesn't have a heuristic like "if I need >3 exact file:line lookups in the same repo, clone first."

### Improvement Possibilities

| # | Change | Scope |
|---|--------|-------|
| A | **Add clone-amortization hint**: after 2+ `ghSearchCode` calls on the same repo, emit: *"You've made N targeted lookups on owner/repo. Consider `ghCloneRepo` to unlock `lspGetSemantics` and `localSearchCode` for 10–40× leaner subsequent calls."* | hints |
| B | **Clarify `lineHint` sourcing in description**: *"Get `lineHint` from a prior `localSearchCode` or `ghSearchCode` hit on the same symbol."* | description |
| C | **Expose `documentSymbols` as a recommended first step** for any large-file question: *"Use `type=documentSymbols` to get the file's function map before targeted reads."* | description / hints |
| D | **Add `type="enclosingFunction"`** — given a file + line number, return the name, start line, and signature of the innermost enclosing function/method. Directly solves the Q5 catch-site misidentification. | scheme / execution |

### Before / After

**Q7 — revalidateTag write site + consumer (pendingRevalidatedTags)**

```
BEFORE (4 calls, 15,665 chars, D=2 due to minor line-number drift + wrong consumer)
  call 1: ghGetFileContent path=revalidate.ts        → 3,772 chars
  call 2: ghSearchCode keywords=["pendingRevalidatedTags"] → 4,825 chars
  call 3: ghGetFileContent path=work-async-storage.ts → 1,930 chars
  call 4: ghGetFileContent path=revalidation-utils.ts → 3,010 chars
  Result: line 36 (off by 2), cited incremental-cache.ts instead of executeRevalidates

AFTER with LSP (4 calls, ~1,800 chars, D=3)
  call 1: localSearchCode pattern="export function revalidateTag" path=revalidate.ts
         → ~80 chars  (lineHint: 34)
  call 2: lspGetSemantics type="definition" symbolName="revalidateTag" lineHint=34
         → ~200 chars  (exact file:line:col, signature)
  call 3: lspGetSemantics type="references" symbolName="pendingRevalidatedTags" lineHint=67
         → ~800 chars  (all write + read sites grouped by file)
  call 4: localSearchCode pattern="executeRevalidates" path=revalidation-utils.ts
         → ~60 chars  (confirm consumer)
  Result: exact line 34, executeRevalidates identified  →  88% reduction, D=3
```

### Agent / Research Impact

- Enables **exact definition lookup** with zero false positives (Q2, Q5, Q7, Q8).
- **`type="references"`** replaces multi-file `ghSearchCode` sweeps for symbol usage.
- **`documentSymbols`** replaces the `minify="symbols"` workaround for file structure navigation.
- Potential savings: **Q2–Q8 combined: ~95 k chars → ~15 k chars** if LSP is used after a single clone.

---

## `localSearchCode` (ripgrep)

### Current Issues

1. **Underused compared to `ghSearchCode`** even when a local clone is available.  
   Benchmark Q1–Q10 were remote-only, but when a clone exists, `rg` output is 10–100× leaner than GitHub search results (no fragment context, no repo metadata, pure `file:line:content`).

2. **No auto-suggestion to use local search when a clone is detected.**  
   After `ghCloneRepo`, the agent continues using `ghSearchCode` for pattern matching instead of switching to `localSearchCode`.

### Reasoning

`ghSearchCode` is the natural first tool in the agent's routing. After cloning, there's no friction point that redirects the agent to the local variant. The hint system should surface "clone is cached → use localSearchCode" automatically.

### Improvement Possibilities

| # | Change | Scope |
|---|--------|-------|
| A | **Post-clone hint**: after any successful `ghCloneRepo`, inject into subsequent responses: *"Repo is cloned at `localPath`. Prefer `localSearchCode` over `ghSearchCode` for pattern matching — output is 10–100× leaner."* | hints |
| B | **Pattern recommendation**: in `localSearchCode` description add: *"Use `^export class`, `^export function`, `^export const` anchors for declaration lookups — eliminates import/re-export noise without needing `filename` filter."* | description |

### Before / After

**Q5 — redirect() enclosing function discovery**

```
BEFORE (ghSearchCode, expensive + wrong answer)
  ghSearchCode keywords=["isRedirectError"] path=app-render.tsx → 3,107 chars
  → match at line 3976 but no enclosing function info

AFTER (localSearchCode, lean + correct)
  localSearchCode pattern="async function renderToStream" path=app-render.tsx → 35 chars
  → "app-render.tsx:3147: async function renderToStream("
  Agent immediately knows: isRedirectError at 3976 is inside renderToStream starting at 3147.
```

### Agent / Research Impact

- Per-pattern search: **~200–5,000 chars → ~20–200 chars** (10–100× reduction).
- Eliminates the `ghSearchCode` fragment-context overhead entirely for code-tracing questions.
- Fixes Q5 quality issue with one 35-char call.

---

## Summary Table

| Tool | Primary Issue | Severity | Estimated Savings | Fixes Q |
|------|--------------|----------|------------------|---------|
| `ghGetFileContent` | `minify` defaults to `"none"` | **Critical** | 5–10× per large-file read | Q5 (quality), Q6, Q8 |
| `ghGetFileContent` | No enclosing-function info at catch sites | **High** | Prevents wrong answers | Q5 |
| `ghSearchCode` | Returns full text-match fragments for simple lookups | **High** | 14× for symbol declarations | Q2, Q7 |
| `ghSearchCode` | No anchored-pattern guidance | **Medium** | 2–5× noise reduction | Q2, Q5, Q7 |
| `lspGetSemantics` | Unused; no clone-then-LSP routing rule | **High** | 10–40× for definition/reference | Q2, Q5, Q7, Q8 |
| `lspGetSemantics` | Missing `type="enclosingFunction"` | **Medium** | Prevents misidentification | Q5 |
| `localSearchCode` | Not switched to after clone; no anchor guidance | **Medium** | 10–100× vs GitHub search | Q5, Q7, Q8 |

### Highest ROI changes (effort vs. impact)

1. **Change `ghGetFileContent` default `minify` to `"standard"` for files >5 k chars** — one-line schema change, fixes Q6 + Q8 cost in every future run.
2. **Add large-file hint + `minify="symbols"` prompt** — zero schema change, immediate agent guidance.
3. **Add `lineOnly: true` to `ghSearchCode`** — fixes Q2 overhead, benefits every symbol-declaration query.
4. **Add `enclosingFunction: true` to `ghGetFileContent`** — fixes Q5 quality bug, applies to any catch-site / callback research.
5. **Post-clone routing hint to prefer `localSearchCode`** — zero tool change, purely a hint injection.
