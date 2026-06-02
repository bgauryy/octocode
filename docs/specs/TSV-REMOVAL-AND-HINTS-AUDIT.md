# TSV Removal & Hints Audit

> Research-driven spec for: (1) removing TSV format entirely and returning to YAML/JSON structured output, and (2) restoring the dynamic hints that were dropped during the TSV migration.

---

## Part 1 — Hints Audit

### Design contract (current, intentional)

`src/hints/index.ts` states the enforced rule:

> Hints only fire on **empty/error** states. On success, the response data + pagination/evidence/warnings carry the signal — usage guidance lives in the tool description.

This is the **correct design**. The old `user-octocode` server emitted 8–11 hints on every success response, most of which were boilerplate repeated on every call ("Do findings answer your question?", "Got 3+ examples?"). These are static — they belong in the tool `description`, not in runtime hints.

### Hint categories

| Category | Owner | Status |
|---|---|---|
| Static usage guidance | Tool `description` field | ✅ Already there |
| Pagination cursors | `hints[]` in response (computed inline) | ✅ Already there |
| Empty-state recovery | `hints/` module, `empty` branch | ✅ Already there |
| Error-state recovery | `hints/` module, `error` branch | ✅ Already there |
| Answer readiness | `evidence.answerReady` | ✅ Already there (new) |
| **Dynamic success hints** | Should be in execution.ts | ⚠️ Partially missing |

### Missing dynamic hints (must add)

These hints are **response-state-dependent** — they use data from the current result. They cannot live in the static tool description. They were present in `user-octocode` and were not ported.

#### 1. `localGetFileContent` — partial file continuation

**Missing hint:** `"More content: use startLine={endLine + 1} to continue ({remaining} lines remaining)"`

- **When:** `isPartial === true`
- **Why:** Without this, agents that read lines 1–40 of a 589-line file have no signal telling them how to continue. They either re-read the whole file or stop.
- **Where to add:** `src/tools/local_fetch_content/execution.ts` — attach to the `evidence.incompleteReasons` or emit via `data.hints[]`

```typescript
// In buildFetchContentEvidence or in the result builder
if (data.isPartial && data.endLine && data.totalLines) {
  const remaining = data.totalLines - data.endLine;
  hints.push(
    `More content: use startLine=${data.endLine + 1} to continue (${remaining} lines remaining)`
  );
}
```

#### 2. `packageSearch` — concrete follow-up actions

**Missing hints:**
- `"Install: npm install {name}"` — exact install command using the found package name
- `"Explore: githubViewRepoStructure(owner={owner}, repo={repo})"` — uses resolved owner/repo

- **When:** `status === 'hasResults'` and package has `owner` + `repo` fields
- **Why:** These are actionable next-step hints that use data only available after the search resolves. The old server emitted them, and they prevent an extra round-trip.
- **Where to add:** `src/tools/package_search/execution.ts` — in the result builder when packages are found

```typescript
// After building the result
if (packages.length > 0) {
  const first = packages[0];
  hints.push(`Install: npm install ${first.name}`);
  if (first.owner && first.repo) {
    hints.push(`Explore: githubViewRepoStructure(owner="${first.owner}", repo="${first.repo}")`);
  }
}
```

### Intentionally removed hints (do NOT add back)

These were in `user-octocode` but were correctly removed. They are boilerplate that inflates every response with static text:

| Old hint | Why removed |
|---|---|
| `"Do findings answer your question? If partial, identify gaps and continue"` | Generic research guidance → lives in system prompt / SKILL.md |
| `"Got 3+ examples? Consider stopping to avoid over-research"` | Generic research guidance → SKILL.md |
| `"Check timestamps (pushedAt, lastModified) - skip stale content"` | Generic research guidance → SKILL.md |
| `"OUTPUT: Use lineHint (line number) for all LSP tools"` | Static → `localSearchCode` tool description |
| `"LSP? lspGotoDefinition(uri, symbolName, lineHint=N)"` | Static → tool description |
| `"Then lspCallHierarchy (calls) or lspFindReferences (all usages)"` | Static → tool description |
| `"No LSP? localGetFileContent(matchString) for context"` | Static → tool description |
| `"Source files: localSearchCode(pattern) -> get lineHint -> LSP tools"` | Static → tool description |
| `"NEVER skip localSearchCode for source code analysis"` | Static → tool description |
| `"This is typically the LAST step after search/LSP analysis"` | Static → tool description |
| `"Use 'owner', 'repo', 'branch', 'path' fields directly in next tool calls"` | Static → server instructions |
| `"Follow 'mainResearchGoal', 'researchGoal', 'reasoning', 'hints'"` | Meta → server instructions |

### Summary: 2 hints to add

| Tool | Hint | State | Dynamic data used |
|---|---|---|---|
| `localGetFileContent` | `"More content: use startLine=N to continue (X lines remaining)"` | success + isPartial | `endLine`, `totalLines` |
| `packageSearch` | `"Install: npm install {name}"` | success | `name` |
| `packageSearch` | `"Explore: githubViewRepoStructure(owner=X, repo=Y)"` | success | `owner`, `repo` |

---

## Part 2 — TSV Removal Plan

### Why TSV was introduced

From `tsvFormat.ts`:

> Tab-delimited because tabs almost never appear in source code / paths, while commas appear constantly. RFC-4180-style quote wrapping is replaced with literal-escape encoding (`\t`, `\n`, `\r`, `\\`) so every row stays on a single line — easier for downstream LLM consumers to scan.

The intent was **token efficiency** for tabular data.

### Why TSV should be removed

1. **File content is not tabular.** `localGetFileContent` puts code into a TSV cell with escaped `\n`. The result is completely unreadable and harder to parse than plain YAML. This is the most critical issue.

2. **Structured JSON/YAML is already token-efficient.** The `evidence`, `base` path prefix, and `shared` constant hoisting all reduce size. These are separate from the format and should be kept.

3. **TSV breaks the structured output contract.** In TSV mode, `results[]` is suppressed from `content[0].text`. Agents that process `content` directly lose the per-query structure. `structuredContent` still has full JSON, creating two sources of truth.

4. **Complexity cost.** `tsvColumns.ts` is 836 lines of column projection logic — one projection per tool. This is a large maintenance surface for a presentation layer.

5. **Token savings are real but modest.** TSV saves ~30–60% characters on search/listing results (see benchmark). But YAML with `base` prefix already saves ~25% on its own. The net incremental benefit of TSV over YAML-with-base is small.

### Token comparison (benchmark, same queries)

| Tool | user-octocode (YAML) | user-octocode-local (TSV) | Savings |
|---|---|---|---|
| `localSearchCode` (2 matches) | ~1,100 chars | ~380 chars | 65% |
| `localViewStructure` (20 entries) | ~2,800 chars | ~1,100 chars | 61% |
| `localGetFileContent` (40 lines) | ~1,800 chars | ~1,750 chars | ~0% (+ unreadable) |
| `packageSearch` | ~900 chars (1 result) | ~1,200 chars (9 results) | -33% (but more data) |
| `localFindFiles` (20 files) | ~4,200 chars | ~2,000 chars | 52% |

**Key finding:** `localGetFileContent` saves nothing from TSV and becomes unreadable. For other tools, the savings are significant. This argues for a **selective approach**: keep TSV for tabular data, remove it only for file content. However, the user has decided on full removal.

### What to keep after TSV removal

These features are format-agnostic and should be **retained in YAML output**:

- **`base` path prefix** — relativizes absolute paths to a common root. Saves 30–50 chars per path row. Keep in all tools.
- **`shared` constant hoisting** — promotes all-same column values to a top-level object. Saves repeated data. Keep in structured JSON response.
- **`evidence` object** — `answerReady`, `confidence`, `complete`, `reason`. Unrelated to TSV. Keep.
- **`responsePagination`** — char-based bulk pagination. Keep.

### Migration scope

#### Source files to delete

| File | Reason |
|---|---|
| `src/utils/response/tsvFormat.ts` | TSV serializer — no longer needed |
| `src/utils/response/tsvColumns.ts` | Per-tool column projections — 836 lines |
| `src/utils/response/tsvFinalize.ts` | TSV finalization (base/shared/column-drop) |
| `src/scheme/tsvEnvelope.ts` | Zod schema for TSV envelope fields |

#### Source files to modify

| File | Change |
|---|---|
| `src/utils/response/bulk.ts` | Remove TSV emission block (~30 lines around L272–310); remove `format: 'tsv'` config; remove `stripTsvEnvelope`; restore `results[]` to `content[0].text` unconditionally |
| `src/tools/github_fetch_content/finalizer.ts` | Remove TSV block (~15 lines around L473–484) |
| `src/tools/github_search_code/finalizer.ts` | Remove TSV block (~15 lines around L427–436) |
| `src/scheme/remoteSchemaOverlay.ts` | Remove `format`, `columns`, `rows` from 3 output schema shapes (L262–266, L371–375, L703) |
| `src/scheme/lspOutputSchemaOverlay.ts` | Remove TSV envelope fields from LSP output schemas |
| `src/types/bulk.ts` | Remove `format?: 'tsv' | 'json'` from input (L73) and `format?`, `columns`, `rows` from output (L118–122) |
| Input schemas (all tools) | Remove `format` parameter from query input |

#### Test files to update

13 test files reference TSV directly:

| Test file | Action |
|---|---|
| `tests/utils/response/tsvColumns.exports.test.ts` | Delete — tests deleted module |
| `tests/utils/response/tsvColumns.fallbacks.test.ts` | Delete — tests deleted module |
| `tests/utils/response/tsvFinalize.test.ts` | Delete — tests deleted module |
| `tests/utils/response/bulk.tsv-branch.test.ts` | Delete or repurpose — tests TSV branch in bulk |
| `tests/utils/response/all-tools-tsv-envelope.e2e.test.ts` | Delete — tests TSV envelope |
| `tests/scheme/tsvEnvelope.strip.test.ts` | Delete — tests deleted schema |
| `tests/tools/all-tools.tsv-and-pagination.test.ts` | Update — remove TSV assertions, keep pagination |
| `tests/tools/all-tools.pagination-contract.test.ts` | Update — remove TSV contract checks |
| `tests/tools/directToolCatalog.test.ts` | Update — remove TSV format field assertions |
| `tests/tools/github_fetch_content.finalizer.coverage.test.ts` | Update — remove TSV finalizer branch tests |
| `tests/tools/local_mcp_response_pagination.test.ts` | Update — remove TSV-specific pagination assertions |
| `tests/tools/remote-tools.contract.test.ts` | Update — remove TSV contract assertions |
| `tests/utils/bulkOperations.test.ts` | Update — remove TSV branch tests |
| `tests/utils/responses.test.ts` | Update — remove TSV format assertions |

### Migration phases

#### Phase 1 — Isolate TSV (prerequisite, no behavior change)
- Move all TSV logic behind a feature flag: `OCTOCODE_OUTPUT_FORMAT=tsv` (already exists as `OCTOCODE_OUTPUT_FORMAT`)
- Verify current tests pass with both `tsv` and `json`/`yaml` modes
- **Goal:** safe rollback point

#### Phase 2 — Remove `localGetFileContent` from TSV
- Change `local_fetch_content` to always emit YAML content block
- Add `"More content: use startLine=N..."` hint (from Part 1)
- Update `tsvColumns.ts` to remove `localGetFileContent` projection
- Tests: `tests/tools/local_fetch_content.test.ts`

#### Phase 3 — Remove TSV from all tools
- Delete `tsvFormat.ts`, `tsvColumns.ts`, `tsvFinalize.ts`, `tsvEnvelope.ts`
- Remove TSV emission from `bulk.ts`, `github_fetch_content/finalizer.ts`, `github_search_code/finalizer.ts`
- Restore `results[]` to `content[0].text` in all responses
- Remove `format` input param from all tool schemas
- Remove TSV output fields from all output schemas
- **Keep:** `base`, `shared`, `evidence`, `responsePagination`

#### Phase 4 — Clean up tests
- Delete the 6 test files that test only TSV modules
- Update the 8 test files with TSV assertions
- Verify 90% coverage threshold still met

### New response shape after migration

**Before (TSV mode, `localSearchCode`):**
```yaml
format: tsv
columns: [path, line, column, value]
rows: "path\tline\t...\nexecution.ts\t28\t50\t// import..."
base: /Users/guybary/Documents/.../local_ripgrep
evidence:
  kind: code
  answerReady: true
```

**After (YAML, `localSearchCode`):**
```yaml
results:
  - id: q1
    status: hasResults
    data:
      files:
        - path: execution.ts          # relative to base
          matchCount: 1
          matches:
            - value: "// import (execution → searchContentRipgrep → ripgrepExecutor → builder)."
              line: 28
              column: 50
      searchEngine: rg
      pagination:
        currentPage: 1
        totalPages: 1
        totalFiles: 2
        hasMore: false
base: /Users/guybary/Documents/.../local_ripgrep
evidence:
  kind: code
  answerReady: true
  complete: true
```

Token count comparison at this scale: TSV ~380 chars vs YAML ~450 chars. The difference is small for 2 results. For 20 results, YAML will be larger (~2,800 chars vs ~2,000 chars TSV). This is the accepted trade-off for readability.

### Open question: path relativization in YAML

TSV currently relativizes paths using `base`. In YAML, paths in `files[].path` can be kept relative to `base` just as they are in TSV. The `base` field stays in the response root. This should be preserved: it saves tokens and makes paths readable without `finalizeTsv`.

---

## Part 3 — Target Minimization Stack

The new output pipeline combines the best of both servers. Every layer below is additive — each one saves tokens or reduces noise independently.

### The stack

| Layer | From | What it does | Status after migration |
|---|---|---|---|
| `cleanJsonObject` | `user-octocode` | Strips `null`, `undefined`, `NaN`, empty arrays, empty objects recursively | ✅ Keep |
| `jsonToYamlString` | `user-octocode` | Compact YAML via `js-yaml` (`lineWidth: -1`, no line wrap, block style) | ✅ Keep |
| `keysPriority` ordering | `user-octocode` | Floats important keys (`results`, `status`, `data`, `evidence`) to top | ✅ Keep |
| `base` + path relativization | `user-octocode-local` | Replaces repeated absolute path prefix with a single root `base` field | ✅ Keep, move out of `finalizeTsv` |
| `shared` constant hoisting | `user-octocode-local` | Promotes column values that are identical across all rows to a top-level `shared` object | ✅ Keep, move out of `finalizeTsv` |
| `evidence.answerReady` | `user-octocode-local` | Tells the agent when results are sufficient — stops redundant follow-up calls | ✅ Keep |
| `responsePagination` (char-based) | `user-octocode-local` | Explicit char offsets for bulk response pagination — better budget control than row counts | ✅ Keep |
| TSV format | `user-octocode-local` | Tab-delimited row serialization | ❌ Remove |

### Layer details

#### `cleanJsonObject` — null/empty stripping

Defined in `src/responses.ts:449`. Recursively removes:
- `null`, `undefined`, `NaN`
- Empty arrays (with one exception: empty `files[]` at code-search path depth is preserved)
- Empty objects

Applies before serialization on every response. No TSV dependency — survives the migration unchanged.

#### `jsonToYamlString` — YAML serialization

Defined in `src/utils/minifier/jsonToYamlString.ts`. Uses `js-yaml` with:
- `lineWidth: -1` — no line wrapping
- `flowLevel: -1` — always block style (no inline `{}`  for objects)
- `forceQuotes: true` — consistent string quoting
- `sortKeys: createSortFunction()` — respects `keysPriority`

Activated when `OCTOCODE_OUTPUT_FORMAT=yaml` (default). JSON mode (`OCTOCODE_OUTPUT_FORMAT=json`) uses `JSON.stringify` with the same key ordering.

#### `keysPriority` ordering

Each tool passes a `keysPriority` array to `createResponseFormat`. Default for bulk tools: `['results', 'id', 'status', 'data']`. Individual tools add their own priority keys (e.g. `package_search/execution.ts` uses `['packages', 'totalFound', 'error']`). This puts the signal before the noise in every response.

#### `base` + path relativization ← **must migrate out of TSV**

Currently lives inside `finalizeTsv()` in `src/utils/response/tsvColumns.ts`. It:
1. Finds the longest common path prefix across all `path` fields in the result rows
2. Sets `responseData.base = commonPrefix`
3. Rewrites each `path` to be relative to that prefix

**Migration task:** Extract this logic into `src/utils/response/pathRelativization.ts` (new utility) and call it from the YAML response pipeline in `bulk.ts` — directly on the `results[]` array before serialization. The `base` field stays in the response root exactly as it does today.

```typescript
// New utility (extracted from finalizeTsv)
export function relativizePaths(
  results: FlatQueryResult[],
  pathFields: string[] = ['path']
): { base?: string } {
  // find common prefix, rewrite path fields in-place, return { base }
}
```

#### `shared` constant hoisting ← **must migrate out of TSV**

Also lives in `finalizeTsv()`. Finds columns where every row has the same value (e.g. `owner=octokit` across all GitHub results) and promotes them to a top-level `shared` object, removing them from individual rows. Saves tokens proportionally to result count.

**Migration task:** Extract into the same new `pathRelativization.ts` utility (or a sibling `responseFinalize.ts`) and call from the YAML pipeline.

#### `evidence.answerReady`

Defined in `src/tools/evidence.ts`. Set per-tool execution as `answerReady: hasResults`. AND-gated across bulk queries in `src/utils/response/bulk.ts:488–492`. Not a formatting concern — survives the migration unchanged.

#### `responsePagination` (char-based)

Bulk-level pagination using `charOffset` / `charLength` / `totalChars`. Already independent of TSV — survives unchanged.

### Token impact after migration

Estimated response sizes for the same queries, comparing current TSV vs target YAML-with-stack:

| Tool | TSV (current) | YAML + stack (target) | Delta |
|---|---|---|---|
| `localSearchCode` (2 matches) | ~380 chars | ~420 chars | +11% |
| `localSearchCode` (20 matches) | ~2,000 chars | ~2,600 chars | +30% |
| `localViewStructure` (20 entries) | ~1,100 chars | ~1,500 chars | +36% |
| `localFindFiles` (20 files) | ~2,000 chars | ~2,600 chars | +30% |
| `localGetFileContent` (40 lines) | ~1,750 chars (unreadable) | ~1,800 chars (readable) | +3% |
| `packageSearch` (9 results) | ~1,200 chars | ~1,400 chars | +17% |

The 30–36% overhead vs TSV is the cost of readability. With `base` + `shared` hoisting, YAML is already ~25% smaller than the original `user-octocode` YAML (which had no relativization). The net is a ~10–15% regression from current TSV — accepted.

### What the pipeline looks like after migration

```
Tool execution result
  → cleanJsonObject()           strip nulls, empty arrays, empty objects
  → relativizePaths()           compute base, rewrite paths to relative
  → hoistSharedFields()         promote constant fields to shared{}
  → jsonToYamlString(           serialize
      data,
      { keysPriority: [...] }
    )
  → ContentSanitizer            redact secrets
  → maskSensitiveData           final safety pass
  → content[0].text             single text block, no TSV envelope
```

`structuredContent` carries the same object (pre-serialization) for programmatic consumers.

---

## Action items

> **Status (implemented 2026-06-02, branch `updates`).** Both parts delivered in one change. Two spec claims were inaccurate and one Part-3 item is an open deviation — see notes.

### Done

- [x] Add `localGetFileContent` partial hint (Part 1, item 1) — added to `buildSuccessResult` (`fetchContent.ts`), scoped to explicit `startLine`/`endLine` reads that stop before EOF (matchString continuation stays char-offset paginated, so a startLine cursor would be wrong there).
- [x] Add `packageSearch` follow-up hints (Part 1, items 2–3) — **CORRECTION:** the explore hint already existed (`generateSuccessHints` → `"Source: github.com/{owner}/{repo} — use githubViewRepoStructure…"`). Only the literal `"Install: npm install {name}"` was missing; added. Three stale tests whose titles say *"should include install hint"* but asserted `not.toContain` were corrected to `toContain`.
- [x] ~~Phase 1: Verify `OCTOCODE_OUTPUT_FORMAT=json`~~ — **CORRECTION:** this env var never existed. Format was a per-call input field `format` with `.default('tsv')`. No flag-based rollback stage was possible; went straight to removal per the full-removal decision.
- [x] Phase 2 + 3: Full TSV removal — deleted `tsvFormat.ts`, `tsvColumns.ts`, `tsvFinalize.ts`, `tsvEnvelope.ts`; stripped TSV from `bulk.ts` + both GitHub finalizers + schemas + types; removed the `format` input param everywhere. **Kept:** `base` (relativization moved to new `src/utils/response/pathRelativize.ts`), `evidence`, `responsePagination`, peer-hints, `cleanJsonObject`, `jsonToYamlString`, `keysPriority`. `tsvEnvelope.ts` → new `responseEnvelope.ts` (`EvidenceSchema` + `withResponseEnvelope`, fields `{hints,base,evidence}`).
- [x] Phase 4: Test cleanup — deleted 6 TSV-only files; preserved `tsvFinalize.test.ts`'s relativize/commonDir suites as `pathRelativize.test.ts`; renamed `all-tools.tsv-and-pagination.test.ts` → `all-tools.pagination.test.ts`; rewrote `remote-tools.contract.test.ts` (scorecard 5→4 criteria, dropped `tsvIsParseable`). Full suite green except 4 pre-existing, unrelated `whitespace-only path` security failures.

### Done (continued)

- [x] **`shared` constant hoisting (Part 3, line 321) — implemented for YAML.** `hoistSharedFields` (in `src/utils/response/pathRelativize.ts`) hoists scalar fields identical across every flattened leaf object (`files[]`/`entries[]`/`matches[]`/`locations[]`) into a top-level `shared{}`, deleting them from each leaf; called from `bulk.ts` right after `base` relativization. `shared` re-added to `responseEnvelope`, `BulkToolResponse`, and the remote/LSP output schemas (typed `Record<string, string|number|boolean>`).
  - **Identity-key exclusion (deviation from the literal Part-3 example).** Unlike TSV, where `shared` was presentation-only and `structuredContent` kept full rows, the unified output has no separate presentation layer — hoisting mutates the canonical records. So chaining-identity keys (`owner`, `repo`, `name`, `id`) and path keys (`path`, `uri`) are NEVER hoisted: an agent uses them verbatim in the next tool call, and (unlike `base`, which leaves a reconstructable shortened path) deleting them would break tool-chaining and the per-item contract. `shared` therefore collapses only incidental constants (e.g. `type`/`permissions`/`language`). The Part-3 "owner=octokit" example was a flat-TSV-row artifact; in nested YAML `owner`/`repo` already appear once per result.

### References

- `src/hints/index.ts` — hint system design contract
- `src/utils/response/tsvColumns.ts` — full column projection map (836 lines)
- `src/utils/response/bulk.ts` L272–360 — TSV emission and suppression logic
- `src/tools/local_fetch_content/execution.ts` — where partial hint should be added
- `src/tools/package_search/execution.ts` — where follow-up hints should be added
