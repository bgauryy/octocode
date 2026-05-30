# Verbosity — Per-Tool Implementation Guide

Implementation-ready spec for landing the `basic | compact | concise` contract across all 14 tools. Pair this doc with `IMPROVEMENTS.md` (schema cleanup) — that work is done; this is the per-handler wiring.

---

## 🔴 Source-of-truth rule (read first)

**All schema content and tool description edits MUST be made directly in:**

```
/Users/guybary/Documents/octocode-mcp-host/packages/octocode-core/src/resources/
```

| Edit | Goes in |
|---|---|
| Tool description text (`<when>`, `<fromTool>`, `<toTool>`, `<gotchas>`, `<examples>`, `<verbose_instructions>`, `<vs_*>` blocks) | `octocode-mcp-host/packages/octocode-core/src/resources/tools/<tool>.ts` |
| Schema field strings (the `schema: { ... }` map in each tool spec) | `octocode-mcp-host/packages/octocode-core/src/resources/tools/<tool>.ts` |
| Cross-tool agent guidance (`<tool_invariants>`, `<workflow>`, `<principles>`, `<response_handling>`, etc.) | `octocode-mcp-host/packages/octocode-core/src/resources/systemPrompt.ts` |
| Tool name registry, `baseSchema`, `verbosity` enum description | `octocode-mcp-host/packages/octocode-core/src/resources/global.ts` |
| Public types (`Verbosity`, `ToolSpec`, `BaseSchema`, etc.) + runtime exports (`VERBOSITY_VALUES`) | `octocode-mcp-host/packages/octocode-core/src/types/index.ts` + `src/index.ts` |

**Never re-describe upstream fields in the consumer overlay** (`octocode-mcp/src/scheme/*Overlay.ts`). The overlay does behavior only — Zod enums, pagination defaults, preprocessors. Any `.describe(...)` call in the overlay that duplicates an upstream description is drift risk and must be removed (see `IMPROVEMENTS.md` §1 for the historical inventory of these).

**Consumer-side edits** (in `/Users/guybary/Documents/octocode-mcp/packages/octocode-mcp/src/`) are limited to:
- Per-tool runtime transformers (`apply{Tool}Verbosity` in `tools/<tool>/{execution,finalizer}.ts`).
- Zod overlay behaviour (`scheme/*Overlay.ts`) — pagination defaults, preprocessors, enum tightenings only. No description text.
- Shared verbosity helpers (`scheme/verbosity.ts`).
- Hint generators (`tools/<tool>/hints.ts`).

After upstream edits land, rebuild `octocode-core` and bump the dependency in `packages/octocode-mcp/package.json` for the new descriptions/types to flow through.

---

## 1. Status snapshot

Rating: **A+** (was B+ at session start). **All 14/14 tools fully wired.**

| # | Tool | `isConcise` | `isCompact` | `isBasic` | Caps + downgrade warning |
|---|---|:-:|:-:|:-:|---|
| 1 | `local_ripgrep` | ✅ | ✅ | ✅ | n/a (local) |
| 2 | `local_find_files` | ✅ | ✅ | ✅ | n/a |
| 3 | `local_fetch_content` | ✅ | ✅ | ✅ | n/a |
| 4 | `local_view_structure` | ✅ | ✅ | ✅ | n/a |
| 5 | `lsp_goto_definition` | ✅ | ✅ | ✅ | n/a |
| 6 | `lsp_find_references` | ✅ | ✅ | ✅ | adaptive concise at threshold 500 |
| 7 | `lsp_call_hierarchy` | ✅ | ✅ | ✅ | n/a |
| 8 | `github_search_code` | ✅ | ✅ | ✅ | `limit` → 3 + warning |
| 9 | `github_fetch_content` | ✅ | ✅ | ✅ | drops `content` + warning on `fullContent=true` |
| 10 | `github_view_repo_structure` | ✅ | ✅ | ✅ | no cap (data already shaped) |
| 11 | `github_search_repos` | ✅ | ✅ | ✅ | `limit` → 3 + warning |
| 12 | `github_search_pull_requests` | ✅ | ✅ | ✅ | `limit` → 3 + coerce `type=metadata` + drop `partialContentMetadata` + warning |
| 13 | `github_clone_repo` | ✅ | ✅ | ✅ | n/a (single-shot) |
| 14 | `package_search` | ✅ | ✅ | ✅ | `searchLimit` → 1 + force `npmFetchMetadata=false` + warning |

**Net:** 14/14 fully wired. Every tool implements the canonical `basic | compact | concise` contract. Every cap firing emits a `verbosity-downgrade:` hint naming the affected field(s).

Legend: ✅ done.

---

## 1.1 Followups

### ✅ Done this session
- **Followup #1** — `githubSearchCode` finalizer wired (`finalizer.ts`). All-concise detection at bulk level; projects `results[].matches[].value=''`, caps groups to 3, emits `verbosity-downgrade` warning on `limit>3`. Compact trims advisory hints. TSV skipped under all-concise.
- **Followup #2** — `githubGetFileContent` finalizer wired (`finalizer.ts`). All-concise: strips `content` from every file, drops `lastModified*`, emits token estimate + drill-back; `fullContent=true` × concise fires `verbosity-downgrade`. Compact trims truncation prose. TSV skipped under all-concise.
- **Followup #3** — `verbosity-downgrade` variant added to `GroupedToolWarningSchema` (`remoteSchemaOverlay.ts:150`). Third discriminated kind alongside `match-value-truncated` and `content-truncated`.
- **Followup #4** — Pre-flight limit caps wired in all 5 execution-only remote tools' `execution.ts`:
  - `githubSearchRepositories`: cap `limit` → 3.
  - `githubViewRepoStructure`: no cap.
  - `githubCloneRepo`: no cap.
  - `githubSearchPullRequests`: cap `limit` → 3 + coerce `type → "metadata"` (unless `prNumber + explicit type`) + drop `partialContentMetadata`.
  - `packageSearch`: cap `searchLimit` → 1 + force `npmFetchMetadata=false`.
  - Each fires a `verbosity-downgrade: ...` hint listing the affected fields.
- **Followup #5** — Compact-trim wired in all 5 execution-only remote tools. Each ships a `isAdvisory<Tool>Hint` predicate matching §3.1; when `isCompact(query.verbosity)`, `extraHints` are filtered + capped at 2.

### 🟢 Followup #6 — Explicit compact-trim test assertions
- Existing `tests/scheme/verbosity_concise.test.ts` already covers compact identity-on-data via `VERBOSITIES_PRESERVING_DEFAULT = [undefined, 'basic', 'compact']`.
- Remaining: assertions that compact REMOVES advisory hints (not just that the data field is preserved). Mirror per tool; ~14 small `it()` blocks.

### 🟢 Followup #7 — Consume `octocode-core` output types in consumer
- 30+ types declared upstream (`types/index.ts`); consumer `src/tools/*/types.ts` files still declare local versions.
- After `octocode-core` rebuild + version bump, replace local declarations with imports from `@octocodeai/octocode-core`.

### 🟢 Followup #8 — Verbosity-aware hint generators
- Push tier-awareness into `src/tools/*/hints.ts` so dynamic hint generators emit advisory hints only under `basic`, skipping the post-hoc `compactTrimHints` pattern-matching dance.

### 🟢 Followup #9 — Pre-flight concise budget assertion in tests
- `assertConcisePayload(result, maxBytes=2048)` is exported from `verbosity.ts`. Wire into the 14 concise-mode snapshot tests so contract drift trips a test failure.

---

## 2. Canonical contract

Defined ONCE in `octocode-mcp-host/packages/octocode-core/src/resources/global.ts` `baseSchema.verbosity`. Do NOT duplicate per-tool.

| Tier | Meaning | Default? |
|---|---|---|
| `"basic"` | Full content + full hints/metadata. | ✅ (omitted ⇒ basic) |
| `"compact"` | Full content + **TRIMMED hints/metadata**. Data payload intact. | — |
| `"concise"` | Counts/top refs + drill-back hint. Data field DROPPED. | — |

**Invariants:**
1. Omitted `verbosity` ≡ `"basic"` ≡ byte-identical to today's output.
2. `compact` never reduces the data payload (content/files/entries/refs etc.) — it only trims advisory hints/metadata.
3. `compact` never increases user value vs the caller's explicit args (cannot raise `limit`, cannot enable expensive options).
4. `concise` MUST append a drill-back hint via `conciseDrillBackHint(...)`.
5. Cache key never includes `verbosity` (cache shapes down post-fetch). Exception: concise paths that fetched LESS upstream data must not satisfy compact/basic — see §4.

---

## 3. Shared helpers

Location: `packages/octocode-mcp/src/scheme/verbosity.ts`

```ts
isConcise(v)    // true iff v === 'concise'
isCompact(v)  // true iff v === 'compact'
isBasic(v)    // true iff v === undefined || v === 'basic'
conciseDrillBackHint(call) // returns ['verbosity:"concise" — ...', 'Drill-back: ${call}']
```

**Add this helper (one PR, removes 14× of boilerplate):**

```ts
// Trim hints/metadata for compact. Returns hints array capped + advisory-only entries removed.
// Tools pass their per-tool predicate to identify "advisory" hints (recovery prose, large-payload
// warnings, fallback notes). Data-bearing fields are untouched.
export function compactTrimHints(
  hints: string[] | undefined,
  isAdvisory: (h: string) => boolean,
  cap = 2
): string[] | undefined {
  if (!hints?.length) return hints;
  return hints.filter(h => !isAdvisory(h)).slice(0, cap);
}
```

### 3.1 Per-tool advisory-string catalog (predicates)

Concrete substrings/regexes each tool's `isAdvisory` predicate must match. Grounded against the strings the handlers actually emit today.

| Tool | Advisory prefixes / regex |
|---|---|
| `localSearchCode` | `/^Large result set/`, `/^Result payload is large/`, hints starting with `Narrow:` |
| `localFindFiles` | `/^Pass excludeDir/`, `/^Raw name globs over/` |
| `localGetFileContent` | `'Regex is per-line only — verify the pattern fits on one line.'`, `'caseSensitive=true is active — disable for fuzzier matching.'`, `/^Pattern .* not found/`, `/^Continuation:/` |
| `localViewStructure` | `/^Monorepos:/`, `/^Start at workspace/`, `/^Auto-excludes/` |
| `lspGotoDefinition` | `/^Multiple definitions returned/`, `/^Resolved via dynamic import/`, fallback-mode prose: `/^lspMode="fallback"/` |
| `lspFindReferences` | `/^Use groupByFile/`, `/^includePattern/`, fallback-mode prose |
| `lspCallHierarchy` | `/^Prefer depth=1/`, `/^depth>1 risks timeouts/`, fallback-mode prose |
| `githubSearchCode` | `/^pivot terms/`, `/^Long match values may be truncated/`, `/^Cross-repo search needs/` |
| `githubGetFileContent` | `/^Content may be truncated/`, `/^Files >300KB/` |
| `githubViewRepoStructure` | `/^Tree may report .truncated at depth/`, `/^Monorepos:/` |
| `githubSearchRepositories` | `/^Try synonyms/`, `/^High star filters cut noise but hide/` |
| `githubSearchPullRequests` | `/^PR archaeology/`, `/^Title-only is THE/`, `/^withComments\|withCommits add tokens/` |
| `githubCloneRepo` | `/^Cached for 24h/`, `/^Requires disk space/` |
| `packageSearch` | `/^searchLimit=1 for exact names/`, `/^NPM scoped packages use/` |

**Convention:** an advisory hint is *recovery prose, large-payload warnings, or fallback-mode notes* — NOT pagination hints (those are data-bearing under `compact`).

---

## 4. Pagination & limit policy

| Knob | Under `basic` | Under `compact` | Under `concise` |
|---|---|---|---|
| Domain `limit` (API/upstream cap) | user value | user value (preserves recall) | **may cap down** per-tool (see §6); emit `warnings[].kind="verbosity-downgrade"` when cap fires |
| `*PerPage` (rows per page) | user value | user value | n/a (concise returns no rows) |
| `page` / `*PageNumber` | user value | user value | n/a |
| `charOffset` / `charLength` (rendered payload) | user value | user value | n/a (concise payload is tiny) |
| `matchesPerPage` / `referencesPerPage` / `callsPerPage` | user value | user value | n/a |

**Limit-cap insertion point:** caps are applied in **`execution.ts`** (after Zod parse, before the upstream fetch). NEVER in the Zod preprocess — preprocess must preserve the caller's original intent so we can detect the cap firing and emit the `verbosity-downgrade` warning. Pattern:

```ts
const CONCISE_LIMIT = 3;
const userLimit = query.limit ?? 10;
const effectiveLimit = isConcise(query.verbosity) ? Math.min(userLimit, CONCISE_LIMIT) : userLimit;
const downgrade = isConcise(query.verbosity) && userLimit > CONCISE_LIMIT;
// ... fetch with effectiveLimit ...
if (downgrade) warnings.push({ kind: 'verbosity-downgrade', detail: `limit capped to ${CONCISE_LIMIT}` });
```

**Cache key rule:**
- Include `verbosity` in key ONLY when concise fetched less data than basic would (`packageSearch` skipping `npmFetchMetadata`; `githubSearchPullRequests` forcing `type="metadata"`). Otherwise key on the semantic query alone and shape down.

### 4.1 Typed `warnings[].kind` enum

Schema lives at `packages/octocode-mcp/src/scheme/remoteSchemaOverlay.ts:150` as `GroupedToolWarningSchema = z.discriminatedUnion('kind', [...])`.

**Currently shipping (2 kinds):**
- `match-value-truncated` — `githubSearchCode` had to clip a single match value. (line 152)
- `content-truncated` — `githubGetFileContent` had to clip a file's content. (line 160)

**Add for the verbosity rollout (1 new kind):**
```ts
z.object({
  kind: z.literal('verbosity-downgrade'),
  detail: z.string(), // e.g. "limit capped to 3 (concise); user passed 50"
  field: z.string().optional(), // e.g. "limit", "type", "npmFetchMetadata", "fullContent"
}),
```
Add the variant to the discriminated union at `remoteSchemaOverlay.ts:150`. Local + LSP output schemas don't currently carry `warnings[]`; for those tools, append the downgrade as a string in `hints[]` instead with the literal prefix `verbosity-downgrade:` so the agent can pattern-match.

**Open for the future (not blocking this PR):** `has-more-pages`, `non-canonical-paths`, `flag-files-detected` — design decision: keep advisory information in `hints[]` strings, or promote to typed warnings later. Don't ship empty stubs.

---

## 5. Schema descriptors — what's already there, what to add

**Already global** (in `baseSchema.verbosity`):
- `verbosity` field, enum, default, description text — applied to every tool.

**Per-tool additions to upstream `tools/*.ts` description block:**

Every tool gets a new `<verbose_instructions>` block placed AFTER `<toTool>` and BEFORE `<gotchas>`. Template:

```
<verbose_instructions>
- basic (default): {what the agent gets back}
- compact: trims {list advisory hints / metadata that go}; data payload unchanged
- concise: returns {counts / top sample / drill-back}; {data field} dropped — re-call basic with {drill-back action} to recover
</verbose_instructions>
```

The block is documentation only — the runtime behaviour lives in the tool's handler (next section). The block exists so the agent reads tool-specific guidance INLINE, not just the global `baseSchema.verbosity` blurb.

---

## 6. Per-tool spec

Each spec lists: (a) `<verbose_instructions>` text to add upstream; (b) `isConcise` branch (already wired for 6 tools — confirm parity with this spec); (c) `isCompact` branch to add; (d) pagination/limit policy; (e) handler files.

**Where the transformer is invoked (read this before §6.8 onward):**

| Pattern | Tools | Invocation site |
|---|---|---|
| **Result builder (returns before result is wrapped)** | `localSearchCode`, `localFindFiles`, `localFetchContent`, `localViewStructure`, `lspGotoDefinition`, `lspFindReferences`, `lspCallHierarchy` | Inside the `apply{Tool}Verbosity` call, just before the `return` that produces the final result object. Existing call-sites: `ripgrepResultBuilder.ts:157`, `findFiles.ts:235`, `lsp_find_references.ts:63`, `callHierarchy.ts:55`, `local_view_structure.ts:141,286`, `execution.ts:211,241` (goto), `fetchContent.ts:587`. |
| **Finalizer wrapper (2 of 7 remote tools have one)** | `githubSearchCode`, `githubFetchContent` | Inside `buildGithubSearchCodeFinalizer` (`finalizer.ts:124`) / `buildGithubFetchContentFinalizer` (finalizer.ts). |
| **Execution-direct (5 of 7 remote tools, no finalizer.ts)** | `githubViewRepoStructure`, `githubSearchRepositories`, `githubSearchPullRequests`, `githubCloneRepo`, `packageSearch` | Wrap the result object in `apply{Tool}Verbosity(result, query)` immediately before `createSuccessResult(...)` returns it (look for the `executeBulkOperation` → `createSuccessResult` pattern in each `execution.ts`). |

### 6.1 `localSearchCode` (local_ripgrep)

```
<verbose_instructions>
- basic (default): files[] with path:line matches + snippets + per-file match counts + pagination + recovery hints (when query is broad).
- compact: trims "Large result set" warnings, scan stats, and recovery prose; files[] and snippets unchanged.
- concise: files[]=[] and hints=["{N} matches in {M} files (top: path:line)", drill-back]; re-call with verbosity:"basic" or scope path/include to the top path.
</verbose_instructions>
```

**isConcise** (wired at `ripgrepResultBuilder.ts:171`): keep summary line + drill-back; clear `files`.
**isCompact** (TODO): drop `"Large result payload"` / `"narrow with type/include filters"` advisory hints; drop `stats` block when present; cap remaining hints to 2.
**Limit policy:** all knobs preserved across basic/compact. Concise: no cap (cheap already because content shapes down post-parse).
**Files:** `src/tools/local_ripgrep/ripgrepResultBuilder.ts`.

### 6.2 `localFindFiles`

```
<verbose_instructions>
- basic (default): files[] with path/type/size/perms/timestamps + pagination + noisy-dir suggestions.
- compact: trims excludeDir suggestions and large-payload prose; files[] unchanged.
- concise: files[]=[] and hints=["{N} files in {M} dirs (newest: path)", drill-back]; re-call basic or narrow name/type/time filters.
</verbose_instructions>
```

**isConcise** (wired at `findFiles.ts:252`): keep.
**isCompact** (TODO): drop "excludeDir for noisy dirs" recovery prose; cap to 2 hints.
**Limit policy:** unchanged across tiers.
**Files:** `src/tools/local_find_files/findFiles.ts`.

### 6.3 `localGetFileContent`

```
<verbose_instructions>
- basic (default): content slice + line ranges + matchRanges + partial flag + truncation prose + lastModified metadata when available.
- compact: trims "Pattern not found" prose, partial-truncation explanations, and lastModified metadata; content slice unchanged.
- concise: content="" and hints=["{path}: {N} lines, ~{T} tokens raw", drill-back]; re-call basic or use matchString/lineRange for a slice.
</verbose_instructions>
```

**isConcise** (wired at `fetchContent.ts:617`): keep.
**isCompact** (TODO): drop truncation-explanation hints; drop `lastModified` / `lastModifiedBy` fields; cap to 2 hints.
**Limit policy:** unchanged.
**Files:** `src/tools/local_fetch_content/fetchContent.ts`.

### 6.4 `localViewStructure`

```
<verbose_instructions>
- basic (default): entries[] with names/types/size/modified + summary + pagination + monorepo recovery hints.
- compact: trims monorepo/recovery prose and large-tree warnings; entries[] unchanged.
- concise: entries[]=[] and hints=["{F} files, {D} dirs (top: ...)", drill-back]; re-call basic with entryPageNumber + entriesPerPage.
</verbose_instructions>
```

**isConcise** (CURRENT: raw `verbosity !== 'concise'` at `local_view_structure.ts:314`): **migrate to `isConcise(verbosity)`** for consistency with the helper.
**isCompact** (TODO): drop monorepo-hint advisory; cap to 2 hints.
**Limit policy:** unchanged.
**Files:** `src/tools/local_view_structure/local_view_structure.ts`.

### 6.5 `lspGotoDefinition`

```
<verbose_instructions>
- basic (default): locations[] with ranges + snippets + resolved position + lspMode (semantic/fallback).
- compact: trims fallback-mode advisory prose and recovery hints; locations[] and snippets unchanged.
- concise: locations[].content="" and hints=["{N} defs (top: path:line:col)", drill-back]; re-call basic for snippets around the location.
</verbose_instructions>
```

**isConcise** (wired at `execution.ts:626`): keep.
**isCompact** (TODO): drop "Multiple definitions returned ⇒ overloads or re-exports" advisory; drop `lspMode==='fallback'` warning prose; cap to 2 hints.
**Limit policy:** unchanged.
**Files:** `src/tools/lsp_goto_definition/execution.ts`.

### 6.6 `lspFindReferences`

```
<verbose_instructions>
- basic (default): refs with ranges + snippets + definition markers + pagination + lspMode + groupByFile-recommendation hints when fanout is large.
- compact: trims groupByFile recommendations and impact-analysis prose; refs and snippets unchanged.
- concise: adaptive — under threshold (~500) emits flat refs[] of "file:line" strings; at/above threshold emits per-file rollup; snippets always dropped; drill-back recommends compact, groupByFile, or includePattern.
</verbose_instructions>
```

**isConcise** (wired at `lsp_find_references.ts:449`, adaptive): keep the adaptive threshold. Constant: `CONCISE_REFS_FLAT_THRESHOLD = 500` at `lsp_find_references.ts:442`. Below the threshold → flat `refs[]` of `"file:line"` strings; at/above → per-file rollup with `topFiles`.
**isCompact** (TODO): drop "use groupByFile for impact analysis" recommendation; cap to 2 hints.
**Limit policy:** unchanged. `referencesPerPage` honored on every tier.
**Files:** `src/tools/lsp_find_references/lsp_find_references.ts`.

### 6.7 `lspCallHierarchy`

```
<verbose_instructions>
- basic (default): target item + caller/callee nodes + snippets + call ranges + pagination + lspMode.
- compact: trims depth>1 timeout warnings and fallback-mode prose; nodes and snippets unchanged.
- concise: node content="" and call arrays dropped; emits edge counts + compact "A -> B" edge list + drill-back; re-call basic for full per-node context.
</verbose_instructions>
```

**isConcise** (wired at `callHierarchy.ts:298`): keep.
**isCompact** (TODO): drop "Prefer depth=1" advisory; cap to 2 hints.
**Limit policy:** unchanged.
**Files:** `src/tools/lsp_call_hierarchy/callHierarchy.ts`.

### 6.8 `githubSearchCode` ❌ TODO

```
<verbose_instructions>
- basic (default): results[] grouped by owner/repo with matches[].path + matches[].value (snippet) + pagination + recovery hints.
- compact: trims pivot-suggestion hints and content-truncation prose; matches and snippets unchanged.
- concise: matches[].value="" and hints=["{N} matches across {M} paths (top: owner/repo:path)", drill-back]; caps limit to 3 and emits warnings[].kind="verbosity-downgrade" if user passed limit>3; re-call basic and scope owner/repo/path to the top match.
</verbose_instructions>
```

**Add to overlay finalizer:** call `applyGithubSearchCodeVerbosity(result, query, totals)` from `finalizer.ts`.
**isConcise:** drop `matches[].value` strings; emit summary + drill-back; cap query `limit` to 3 BEFORE the upstream fetch.
**isCompact:** drop "pivot terms or check repo structure" prose and "Long match values may be truncated" warnings; cap hints to 2.
**Limit policy:** concise caps `limit ≤ 3`; basic/compact preserve user value. Cache key includes effective `limit` (not `verbosity`); concise cache can serve only concise.
**Files:** `src/scheme/remoteSchemaOverlay.ts` (transformer location); `src/tools/github_search_code/execution.ts` + `finalizer.ts`.

### 6.9 `githubGetFileContent` ❌ TODO

```
<verbose_instructions>
- basic (default): path + content + totalLines + startLine/endLine + isPartial + resolvedBranch + pagination + lastModified metadata.
- compact: trims content-truncation prose and skips lastModified/lastModifiedBy lookup; content unchanged.
- concise: content="" and hints=["{path}: {N} lines, ~{T} tokens, branch {B}", drill-back]; warns verbosity-downgrade if caller passed fullContent=true; re-call basic with matchString or startLine/endLine.
</verbose_instructions>
```

**isConcise:** drop `content`; surface `totalLines` + token estimate; if `fullContent === true`, emit `verbosity-downgrade` warning and ignore.
**isCompact:** skip the optional `fetchFileTimestamp` call to save a round-trip; drop truncation-explanation hints.
**Limit policy:** unchanged. Cache: full-content cache shapes down to concise; concise cache (no content) cannot serve basic/compact.
**Files:** `src/tools/github_fetch_content/execution.ts` + `finalizer.ts`.

### 6.10 `githubViewRepoStructure` ❌ TODO

```
<verbose_instructions>
- basic (default): entries[] with path/name/type + pagination + monorepo recovery hints.
- compact: trims monorepo recovery prose and "truncated at depth=N" warnings; entries unchanged.
- concise: entries[]=[] and hints=["{F} files, {D} dirs (top: ...)", drill-back]; re-call basic with entryPageNumber + entriesPerPage.
</verbose_instructions>
```

**isConcise:** drop `entries[]`; emit summary + drill-back.
**isCompact:** drop monorepo-hint advisory; cap to 2 hints.
**Limit policy:** unchanged.
**Files:** `src/tools/github_view_repo_structure/execution.ts`.

### 6.11 `githubSearchRepositories` ❌ TODO

```
<verbose_instructions>
- basic (default): repositories[] with full_name + url + stars + language + pushed_at + short description + topics + license + recovery hints.
- compact: trims synonym-suggestion prose and "high star filters hide niche" advisory; repositories unchanged.
- concise: repositories[] becomes [{full_name, stars, language?}][≤3] plus drill-back; caps limit to 3 with verbosity-downgrade warning; re-call basic or narrow keywordsToSearch.
</verbose_instructions>
```

**isConcise:** project each repo to `{full_name, stars, language?}`; cap `limit ≤ 3`.
**isCompact:** drop synonym/recovery hints; cap to 2 hints.
**Limit policy:** concise caps `limit ≤ 3`. Cache key includes effective `limit`.
**Files:** `src/tools/github_search_repos/execution.ts`.

### 6.12 `githubSearchPullRequests` ❌ TODO

```
<verbose_instructions>
- basic (default): PRs with number/title/state/draft/merged/author/dates/labels/file_changes + optional comments/commits when requested + recovery hints.
- compact: trims "PR archaeology" tip prose and recovery hints; PR data unchanged.
- concise: forces type="metadata" unless caller passed prNumber + explicit type; returns [{number, title, state, merged}][≤3] + changed-file count + drill-back; emits verbosity-downgrade warning when collapsing; re-call basic with prNumber=<top> for diff/comments.
</verbose_instructions>
```

**isConcise:** project each PR; coerce `type → "metadata"` upstream call if `prNumber` absent; cap `limit ≤ 3`. When coercing `type`, also DROP `partialContentMetadata` (silently — it was tied to the original `type=partialContent`); emit a single `verbosity-downgrade` warning that lists the coerced/dropped fields: `{ kind: 'verbosity-downgrade', detail: 'type coerced to metadata; partialContentMetadata dropped', field: 'type' }`.
**isCompact:** drop archaeology advisory prose; drop `withComments`/`withCommits` cost notes; cap to 2 hints.
**Limit policy:** concise caps `limit ≤ 3` and may coerce `type`. Cache key includes effective `(type, limit)`; concise-metadata cache cannot serve fullContent compact.
**Files:** `src/tools/github_search_pull_requests/execution.ts`.

### 6.13 `githubCloneRepo` ❌ TODO

```
<verbose_instructions>
- basic (default): localPath + branch + cached flag + sparsePath + clone size stats + duration.
- compact: trims cache-detail prose and clone-time noise; localPath + branch + cached preserved.
- concise: returns only {localPath, cached, branch} + drill-back; re-call basic for clone metadata.
</verbose_instructions>
```

**isConcise:** strip size stats + duration; keep `localPath`, `cached`, `branch`.
**isCompact:** drop cache-detail prose; cap to 2 hints.
**Limit policy:** n/a (clone is single-shot).
**Cache:** UNCHANGED — clone cache key is `(owner, repo, branch, sparse_path)`; verbosity does not affect it.
**Files:** `src/tools/github_clone_repo/execution.ts`.

### 6.14 `packageSearch` ❌ TODO

```
<verbose_instructions>
- basic (default): packages[] with name + version + repository + license + short description + deprecation + optional NPM metadata when npmFetchMetadata=true.
- compact: trims alternative-spelling suggestions and "raise searchLimit" prose; packages unchanged.
- concise: returns [{name, version, repository, deprecated}][≤1] + drill-back; caps searchLimit to 1; auto-disables npmFetchMetadata with verbosity-downgrade warning if user passed true; re-call basic or npmFetchMetadata=true for repo URL.
</verbose_instructions>
```

**isConcise:** strip `description` + `license`; cap `searchLimit ≤ 1`; force `npmFetchMetadata=false`.
**isCompact:** drop alternative-spelling prose; cap to 2 hints.
**Limit policy:** concise caps `searchLimit ≤ 1`. Cache key includes effective `searchLimit`; concise cache that skipped `npmFetchMetadata` cannot serve a request that needs it.
**Files:** `src/tools/package_search/execution.ts`.

---

## 7. Implementation checklist

### 7.1 Shared

- [ ] Add `compactTrimHints(hints, isAdvisory, cap=2)` to `src/scheme/verbosity.ts`.
- [ ] Migrate `local_view_structure.ts:314` from raw `verbosity !== 'concise'` to `isConcise(verbosity)`.

### 7.2 Local + LSP (`isCompact` branch to add to 7 existing transformers)

- [ ] `ripgrepResultBuilder.applyRipgrepVerbosity` — drop large-payload + scan-stats hints.
- [ ] `findFiles.applyFindFilesVerbosity` — drop excludeDir recovery prose.
- [ ] `fetchContent.applyFetchContentVerbosity` — drop truncation prose + `lastModified*`.
- [ ] `local_view_structure.applyVerbosity` — drop monorepo advisory.
- [ ] `lsp_goto_definition/execution.applyGotoDefinitionVerbosity` — drop overload + fallback advisory.
- [ ] `lsp_find_references.applyFindReferencesVerbosity` — drop groupByFile advisory.
- [ ] `lsp_call_hierarchy/callHierarchy.applyCallHierarchyVerbosity` — drop depth advisory.

### 7.3 Remote (7 NEW transformers + handler wiring)

Each follows the pattern at `ripgrepResultBuilder.ts:171`:

- [ ] `github_search_code/finalizer.ts` → `applyGithubSearchCodeVerbosity`.
- [ ] `github_fetch_content/finalizer.ts` → `applyGithubFetchContentVerbosity`.
- [ ] `github_view_repo_structure/execution.ts` → `applyGithubViewRepoStructureVerbosity`.
- [ ] `github_search_repos/execution.ts` → `applyGithubSearchReposVerbosity`.
- [ ] `github_search_pull_requests/execution.ts` → `applyGithubSearchPullRequestsVerbosity`.
- [ ] `github_clone_repo/execution.ts` → `applyGithubCloneRepoVerbosity`.
- [ ] `package_search/execution.ts` → `applyPackageSearchVerbosity`.

Each handler additionally:
- Wires the limit-cap behaviour (§4) BEFORE the upstream fetch where applicable.
- Emits typed `warnings[].kind = "verbosity-downgrade"` when any cap fires.

### 7.4 Upstream description blocks

In `octocode-mcp-host/packages/octocode-core/src/resources/tools/*.ts`:

- [ ] Add the `<verbose_instructions>` block from §6 to each of the 14 tool descriptions, placed AFTER `<toTool>` and BEFORE `<gotchas>`.

### 7.5 Tests

#### Data-field mapping (test target per tool)

Use this table to write the assertion `expect(result.<dataField>).toEqual(basicResult.<dataField>)` under compact and `expect(result.<dataField>).<emptySentinel>` under concise.

| Tool | `dataField` | Concise sentinel |
|---|---|---|
| `localSearchCode` | `files` | `[]` |
| `localFindFiles` | `files` | `[]` |
| `localGetFileContent` | `content` | `''` |
| `localViewStructure` | `entries` | `[]` |
| `lspGotoDefinition` | `locations` | each `.content === ''` |
| `lspFindReferences` | `locations` (below threshold → `refs` flat strings; above → `topFiles` per-file rollup) | snippets dropped on each location |
| `lspCallHierarchy` | `incomingCalls` / `outgoingCalls` (depending on `direction`) | `[]` and node `.content === ''` |
| `githubSearchCode` | `results[].matches[].value` | `''` per match |
| `githubGetFileContent` | `content` | `''` |
| `githubViewRepoStructure` | `entries` | `[]` |
| `githubSearchRepositories` | `repositories` | array length ≤ 3 (`limit` cap), projected to `{full_name, stars, language?}` |
| `githubSearchPullRequests` | `pull_requests` | array length ≤ 3, projected to `{number, title, state, merged}`; `type` coerced to `metadata` |
| `githubCloneRepo` | n/a — passthrough fields | `cloneTimeMs`/size-stats removed; `localPath`/`cached`/`branch` preserved |
| `packageSearch` | `packages` | array length ≤ 1, `description`/`license` dropped, `npmFetchMetadata` forced false |

#### Per-tool assertions

For each of the 14 tools:

- [ ] omitted / `"basic"` → byte-identical to today's output (snapshot test).
- [ ] `"compact"` → `result.<dataField>` deep-equals basic's `<dataField>`; `result.hints` is a subset of basic's `hints` with `cap=2`; no new advisory strings appear.
- [ ] `"concise"` → `<dataField>` matches the sentinel in the table; `hints` contains the `Drill-back:` line.
- [ ] Where applicable: `verbosity-downgrade` warning fires when caps engage (`limit > N`, `fullContent=true`, `npmFetchMetadata=true`).
- [ ] Thin-cache rule: concise cache cannot serve basic/compact requests for `githubSearchPullRequests` (when `type` was coerced), `packageSearch` (when `npmFetchMetadata` was skipped), `githubGetFileContent` (when `content` was dropped).

### 7.6 Rollout order

Apply in this sequence to minimise blast radius:

1. **Phase A — shared scaffolding (1 PR).** Add `compactTrimHints` helper to `verbosity.ts`; extend `GroupedToolWarningSchema` with the `verbosity-downgrade` variant at `remoteSchemaOverlay.ts:150`; migrate `local_view_structure.ts:314` to `isConcise(verbosity)`.
2. **Phase B — local + LSP `isCompact` (7 PRs, parallel-safe).** Add the `isCompact` branch to the 7 existing transformers using the §3.1 predicates. No caps fire here — zero behaviour-change for callers not passing verbosity.
3. **Phase C — remote no-cap tools (2 PRs, low risk).** Wire `githubCloneRepo` and `githubViewRepoStructure` (no limit caps, no upstream behaviour change).
4. **Phase D — remote with caps (4 PRs).** Wire `githubSearchCode`, `githubSearchRepositories`, `packageSearch`, `githubFetchContent`. These cap `limit` (or skip metadata) under concise — gated by the `verbosity-downgrade` warning.
5. **Phase E — `githubSearchPullRequests` (1 PR).** Most complex: caps `limit`, coerces `type`, drops `partialContentMetadata`. Ship last; gate all three downgrades behind a single warning.

**Per-phase success criterion:** all snapshot tests under `basic` still pass (no behaviour change for non-verbosity callers); compact/concise tests pass for the tools in that phase.

### 7.7 Validating an upstream `<verbose_instructions>` edit took effect

1. Edit `octocode-mcp-host/packages/octocode-core/src/resources/tools/<tool>.ts`.
2. From `octocode-mcp-host/packages/octocode-core`: `yarn build` (regenerates `data/default.json` + the compressed blob).
3. In `octocode-mcp`: bump `@octocodeai/octocode-core` if you publish, or `yarn install` if it's a workspace link.
4. Verify: `grep -l "<verbose_instructions>" node_modules/@octocodeai/octocode-core/dist/data/default.json` returns a match.

---

## 8. Reference flow & per-tool uniformity contract

### 8.1 The one flow every tool follows

```
agent input ──► Zod overlay (verbosity accepted via baseSchema) ──► execution
                                                                       │
                                       ┌───────────────────────────────┘
                                       ▼
                          maybe cap upstream limit (concise only)
                          maybe coerce upstream args + emit
                          verbosity-downgrade warning
                                       │
                                       ▼
                                  upstream fetch
                                       │
                                       ▼
                            build result envelope { status, data, hints, warnings }
                                       │
                                       ▼
                          apply{Tool}Verbosity(result, query) ──► response
                                       │
                                       ├─ isConcise: project to concise shape; replace `hints` with summary + drillback
                                       ├─ isCompact: result.hints = compactTrimHints(result.hints, isAdvisory, 2)
                                       └─ isBasic (or undefined): return result unchanged
```

### 8.2 Uniformity contract every transformer MUST satisfy

Every `apply{Tool}Verbosity` function:

1. **Pure & total.** Takes `(result, query, ?totals)`; returns the same envelope type. No I/O, no logging, no mutation of inputs.
2. **Single switch, three branches, in this order.**
   ```ts
   export function applyXxxVerbosity(result, query, ...): R {
     if (isConcise(query.verbosity))   return shapeConcise(result, ...);
     if (isCompact(query.verbosity)) return shapeCompact(result, ...);
     return result; // basic / undefined — passthrough
   }
   ```
3. **Concise path produces a fixed shape:** clears the data field (per §7.5 table), sets `hints = [<one-line summary>, ...conciseDrillBackHint('<drillback>')]`, preserves `status` and `pagination` summary numbers (`totalEntries`, `totalMatches`, etc.).
4. **Compact path touches `hints` only.** Data field is byte-identical to basic. Implementation is a single call: `result.hints = compactTrimHints(result.hints, isAdvisoryForThisTool, 2)`.
5. **Empty/error envelopes pass through unchanged.** `if (result.status !== 'hasResults') return result;` is the second statement after the concise branch enters.
6. **No drift between transformers.** Same function name pattern (`apply{ToolName}Verbosity`), same arg order, same return type as input.

### 8.3 Skeleton for new transformers (5 remote tools without finalizer)

Drop this into the bottom of each tool's `execution.ts`, then invoke at the result-construction site:

```ts
import { isConcise, isCompact, conciseDrillBackHint, compactTrimHints } from '../../scheme/verbosity.js';

const ADVISORY = (h: string): boolean =>
  /^Pattern .* not found/i.test(h) || h.startsWith('Large result') /* ...tool-specific */;

export function applyXxxVerbosity(result: XxxResult, query: XxxQuery): XxxResult {
  if (isConcise(query.verbosity)) {
    if (result.status !== 'hasResults') return result;
    return {
      ...result,
      // clear data field per §7.5
      // build summary
      hints: [
        `<one-line summary, e.g. "${result.totalCount} matches in {N} files">`,
        ...conciseDrillBackHint('<drillback action, e.g. "re-call basic with prNumber=<top>">'),
      ],
    };
  }
  if (isCompact(query.verbosity)) {
    return { ...result, hints: compactTrimHints(result.hints, ADVISORY, 2) };
  }
  return result;
}
```

### 8.4 Anti-patterns the reviewer rejects

- ❌ Inline `verbosity === 'concise'` string checks (use `isConcise(...)` — see `local_view_structure.ts:314` migration TODO).
- ❌ Branching on verbosity outside the transformer (the cap-decision in execution.ts is allowed; the response-shape decision is not).
- ❌ Mutating `result` in place.
- ❌ Adding fields under compact that basic didn't have.
- ❌ Tool-specific drillback hint text outside the per-tool spec (drillback text is per-tool intentionally but MUST flow through `conciseDrillBackHint(...)` so the `verbosity:"concise" — detail dropped...` prefix is uniform).

All 14 tools land on the same shape after Phases A–E. Anything that doesn't match §8.2 is a review-block.
