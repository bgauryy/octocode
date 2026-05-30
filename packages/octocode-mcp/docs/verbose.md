# Verbosity — How Each Tool Shapes Output, and Why

The `verbosity` field is a cross-cutting input on every tool *except* `github_clone_repo`. It controls a single thing: **how much of the result the tool returns to the caller**. It never changes what work the tool performs server-side, only how the response is shaped after the work is done.

Three tiers, default is `basic`:

| Tier | Data | Hints | When to use |
|---|---|---|---|
| `basic` *(default; omitted ≡ basic)* | full payload | full hints | The agent will read the result and act on it. |
| `compact` | full payload | advisory hints trimmed (cap = 2) | The data matters but the hint chatter is wasting tokens. |
| `concise` | **dropped** — replaced by a one-line summary + drill-back hint | summary + drill-back only | Broad probes / counts / "does this exist?" — refetch precisely if needed. |

Three building blocks are shared across all implementations (`src/scheme/verbosity.ts`):

- `isConcise` / `isCompact` / `isBasic` — predicates over the input field.
- `conciseDrillBackHint(call)` — emits two lines: a verbosity marker + a "Drill-back: …" string telling the agent exactly how to refetch the dropped detail. Every concise payload **must** carry one.
- `compactTrimHints(hints, isAdvisory, cap)` — keeps non-advisory hints, drops advisory ones, caps the result. Built with `makeAdvisoryPredicate([...markers])` per tool.

Every non-clone tool exposes one function: `apply<Tool>Verbosity(...)`, importable from that tool's `execution.ts`. It is the **single point of truth** for verbosity shaping in that tool.

---

## Why `github_clone_repo` has no verbosity

Cloning is a one-shot side-effecting action. There's nothing to summarize: the result is a path on disk and a status. Returning "1 repo cloned (top: …)" instead of the path would be a worse interface, not a token-savings win. So the field is absent from `CloneRepoQuerySchema` and the tool's execution does no verbosity branching.

---

## Per-tool behavior

### `local_ripgrep` — `applyRipgrepVerbosity`

Input shape: a `files[]` array with per-file `matches[]`.

| Tier | What changes |
|---|---|
| `basic` | `files[]` with full matches and snippets. |
| `compact` | Same as basic; only `hints` filtered through advisory predicate (large-result-set warnings, pivot suggestions). |
| `concise` | `files: []`; hints become `["<total> matches in <fileCount> files (top: <path>:<line>)", + drill-back]`. |

**Why:** ripgrep typically returns the largest payload of any tool (hundreds of file/match objects). Concise collapsing to one summary line is the highest-leverage savings in the system. The "top: path:line" gives the agent enough to re-call with a scoped pattern.

---

### `local_find_files` — `applyFindFilesVerbosity`

Input shape: `files[]` (path-only entries with optional metadata).

| Tier | What changes |
|---|---|
| `basic` | Full `files[]`. |
| `compact` | Hints trimmed. |
| `concise` | `files: []`; hint = `"<n> files in <d> dirs (newest: <path>)"`. |

**Why:** find-files results can fan out into thousands of entries when the pattern is loose. Concise preserves the *count* and the *newest path* so the agent learns whether the candidate set is too broad before sinking tokens into the full list.

---

### `local_fetch_content` — `applyFetchContentVerbosity`

Input shape: a single `content` string for one file.

| Tier | What changes |
|---|---|
| `basic` | Full `content`. |
| `compact` | Hints trimmed. |
| `concise` | `content: ''`; hint = `"<path>: <totalLines> lines, ~<approxTokens> tokens raw"`. |

**Why:** fetching a file is binary — either you wanted the content or you didn't. Concise is for the case where the agent only needed to know *whether the file is big* before deciding to fetch a slice with `matchString` or `lineRange`. The token estimate is a budgeting signal.

---

### `local_view_structure` — `applyViewStructureVerbosity`

Input shape: an `entries[]` directory listing + a `summary` string.

| Tier | What changes |
|---|---|
| `basic` | Full `entries[]`. |
| `compact` | Hints trimmed. |
| `concise` | `entries: []`; hint = `"entries[] dropped. summary: <summary>"`. |

**Why:** the upstream tool already computes a one-line summary as part of its normal output. Concise simply drops the array, keeping the summary the tool would have emitted anyway. Drill-back nudges the agent toward `entryPageNumber`/`entriesPerPage` instead of re-asking for the same big listing.

---

### `lsp_goto_definition` — `applyGotoDefinitionVerbosity`

Input shape: a `locations[]` list, each with a code snippet around the definition.

| Tier | What changes |
|---|---|
| `basic` | Full locations + snippets. |
| `compact` | Hints trimmed. |
| `concise` | `locations[]` is **kept**, but each entry's `content` is blanked; hints carry `"<n> definition(s) (top: <uri>:<line>:<col>)"`. |

**Why:** unlike other tools, the `locations` themselves are small (URI + range). The expensive part is the code snippet attached to each. Concise strips the snippets but preserves the URIs so the agent can still jump back via `local_fetch_content` if it needs source context.

---

### `lsp_find_references` — `applyFindReferencesVerbosity`

Input shape: a `locations[]` list of every reference site.

This tool has the **most adaptive** concise implementation, because references can be 1 or 1,000+:

| Tier | What changes |
|---|---|
| `basic` (no `groupByFile`) | Full locations. |
| `compact` | Hints trimmed. |
| `concise` (refs < threshold) | `locations: []`; hints include `"<n> refs in <f> files"` AND a flat `"refs: uri:line, uri:line, …"` list. |
| `concise` (refs ≥ threshold) | `locations: []`; hints include the count summary + top-files-by-ref-count. |
| `groupByFile: true` (any tier) | Short-circuits the whole switch — returns count-by-file summary. |

**Why:** references are commonly the highest-cardinality LSP result. Below the threshold, the flat list is cheap enough to inline (and useful) — so we do. Above it, we degrade gracefully to top-N-files. `groupByFile` is treated as a tier-orthogonal product mode: if the agent explicitly asked for grouping, that intent overrides verbosity.

---

### `lsp_call_hierarchy` — `applyCallHierarchyVerbosity`

Input shape: a `root` symbol + `calls` / `incomingCalls` / `outgoingCalls` edge lists.

| Tier | What changes |
|---|---|
| `basic` | Full edges + symbol detail. |
| `compact` | Hints trimmed. |
| `concise` | All edge arrays emptied; `item.content` blanked; hints carry `"<n> <direction> edge(s) for <symbol> at depth=<d>"`. |

**Why:** call-hierarchy edges carry positions and snippets, both of which balloon under any non-trivial depth. The summary preserves shape (direction, depth, count, symbol) — enough for the agent to decide whether to drill into a specific edge.

One implementation subtlety: the LSP path emits `calls`, the pattern-fallback path emits `incomingCalls`/`outgoingCalls`. Concise preserves *whichever field was present* (as an empty array) so output-schema validation still passes either way.

---

### `github_search_code` — `applyGithubSearchCodeVerbosity` *(finalizer-based)*

Input shape: grouped results with per-group `matches[]`.

| Tier | What changes |
|---|---|
| `basic` | Full groups + match snippets. |
| `compact` | Hints trimmed. |
| `concise` | Groups capped to `CONCISE_SEARCH_CODE_LIMIT` (= 3), each match's `value` (snippet) blanked. Summary: `"<n> matches across <g> paths (top: owner/repo:path)"`. **Plus**: if the caller passed `limit > 3`, a structured `verbosity-downgrade` warning is appended. |

**Why:** GitHub code search returns rich per-match snippets, and callers commonly ask for `limit: 30`. Concise caps to 3 to keep the budget tight; the warning is explicit so the caller knows their `limit` was overridden.

This function is in `finalizer.ts` (re-exported from `execution.ts`) — it mutates `responseData` in place and returns a boolean controlling downstream TSV emission.

---

### `github_fetch_content` — `applyGithubFetchContentVerbosity` *(finalizer-based)*

Input shape: result groups with per-file `content`.

| Tier | What changes |
|---|---|
| `basic` | Full content. |
| `compact` | Advisory hints trimmed. |
| `concise` (all-queries-concise) | Every file's `content` blanked, `lastModified`/`lastModifiedBy` deleted; hints carry `"<files> files, <lines> lines, ~<tokens> tokens raw"` + drill-back. **Plus**: if any query had `fullContent: true`, a `verbosity-downgrade` warning is appended. |

**Why:** bulk fetch is the highest token consumer in the toolkit. Concise is the natural "did the file exist? is it big?" mode. The `fullContent` warning is explicit — silently honoring `fullContent` under concise would defeat the tier; silently ignoring it without telling the caller would be confusing.

Note: concise collapses **only when every query in the batch is concise**. A mixed batch keeps basic behavior — we don't want to surprise one query with another's tier choice.

---

### `github_search_pull_requests` — `applyGithubSearchPullRequestsVerbosity`

Input shape: `{ data, pullRequests, extraHints, downgradeFields }` (pre-aggregated by the execution pipeline).

| Tier | What changes |
|---|---|
| `basic` | Full PR list. |
| `compact` | Combined hints (downgrade + extra) trimmed through advisory predicate. |
| `concise` | PRs capped to top 3, each projected to `{number, title, state, merged}`; hint = `"<n> PRs (top: #<num>)"` + drill-back + any downgrade markers. |

**Why:** PR objects are heavy (titles, bodies, labels, reviewers). Concise preserves the *identity* of the top 3 (number is enough to re-fetch by `prNumber` later) and discards everything else. The signature is an input/output struct (not the simpler `result, query` shape) because pre-flight concise caps mutate the query and the post-call composition mixes downgrade markers, pagination, advisory recovery, and search-specific hints — keeping those concerns together in one call is cleaner than threading them through.

---

### `github_search_repos` — `applyGithubSearchReposVerbosity`

Input shape: `{ repositories, pagination }`.

| Tier | What changes |
|---|---|
| `basic` / `compact` | Pass through (compact hint-trim is applied later, at the bulk-finalizer pass). |
| `concise` | Top 3 repos projected to `{full_name, stars, language}`; hint = `"<n> repos (top: <full_name>)"`. |

**Why:** repo search aggregates across up to 3 query variants (combined / topics / keywords). The per-variant function focuses on concise; the compact advisory trim happens once, post-aggregation, in the bulk finalizer — so a single `compactTrimHints` call sees the merged hint set instead of being applied N times to partial sets. This is the **one tool where verbosity logic is not 100% inside the apply function**; the architectural asymmetry is intentional but worth knowing.

---

### `github_view_repo_structure` — `applyGithubViewRepoStructureVerbosity`

Input shape: `{ data, entryCount, summary, extraHints }`.

| Tier | What changes |
|---|---|
| `basic` | Full structure. |
| `compact` | Extra hints trimmed. |
| `concise` | Data shrinks to `{path, summary, entryCount}` (entries dropped). Hint = `"<n> entries (<summary>)"`. |

**Why:** repo-structure responses for big repos can be 1000+ entries. Concise collapses to the metadata the agent typically wants (how many, was anything filtered?) without sending the entries themselves. Drill-back points at `entryPageNumber`/`entriesPerPage` since paginating is usually a better recovery than re-fetching with `basic`.

---

### `package_search` — `applyPackageSearchVerbosity`

Input shape: `{ packages, totalFound }`.

| Tier | What changes |
|---|---|
| `basic` | Full package objects. |
| `compact` | Hints trimmed. |
| `concise` | Top 1 package projected to `{name, version, repository, deprecated}`; hint = `"<n> packages found"`. |

**Why:** package metadata is moderate-size but often only the top result matters (the agent searched by name and wants to confirm the version/repo). Concise is aggressive (top 1, not top 3) because package search is rarely used to *enumerate* — it's used to *resolve*.

---

## Design invariants

Read these before changing any `apply<Tool>Verbosity`:

1. **Basic is unchanged.** `basic` (and omitted) must return the input untouched. No filtering, no trimming, no surprises.
2. **Compact only touches hints.** Data payload is identical to basic. Only advisory hints (recovery suggestions, large-payload warnings, fallback notes) are trimmed. Data-bearing hints (pagination cursors, drill-back breadcrumbs) are preserved.
3. **Concise must shrink dramatically AND emit `conciseDrillBackHint`.** The drill-back is non-negotiable — an concise response with no path to refetch the dropped content is a dead end. Target: ≤ 2KB per `assertConcisePayload`.
4. **The advisory predicate is per-tool.** Each tool defines its own `isAdvisoryXHint` via `makeAdvisoryPredicate([...markers])`. Markers are distinctive phrases (e.g. `'large result set'`, `'pivot terms'`, `'lspMode'`) — distinctive enough not to false-positive on data-bearing hints.
5. **Downgrade warnings are explicit.** When concise caps a user-supplied option (e.g. `limit > 3` for code search, `fullContent: true` for fetch content), emit a structured `verbosity-downgrade` warning. Silent overrides confuse callers.
6. **The function is the single point of truth.** Every non-clone tool's `execution.ts` exports an `apply<Tool>Verbosity`. If you find verbosity branching anywhere else in a tool's pipeline, it's a smell — fold it into the function. (The one acknowledged exception is `github_search_repos`' bulk-finalizer compact trim.)

## Where the field lives

- **Schema:** `octocode-mcp/src/scheme/{local,remote,lsp}SchemaOverlay.ts` adds `verbosity: createVerbosityField()` to each tool's bulk query schema. Field description text lives upstream in `octocode-core/src/resources/global.ts` `baseSchema.verbosity` — never re-described in the overlay.
- **Helpers:** `octocode-mcp/src/scheme/verbosity.ts`.
- **Per-tool function:** `octocode-mcp/src/tools/<tool>/execution.ts` (some re-exported from `finalizer.ts` or the tool's result-builder module to avoid execution↔builder cycles).
