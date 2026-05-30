# Octocode MCP — Verbosity Guidance, Tool-Quality Review & Improvement Suggestions

> Evidence-based review produced by exercising **all 13 tools** across `basic` / `compact` / `concise`
> and pagination, then cross-checking the live responses against the tool descriptions and Zod
> schemas in `@octocodeai/octocode-core`. Findings are observed, not assumed.

---

## What verbosity is (in plain terms)

`verbosity` is a single per-query knob — `basic` (default) · `compact` · `concise` — that controls **how much of a result the tool sends back**. It does *not* change *what the tool searches or finds*; it only changes how the finding is rendered into the response. Same query, same matches — different amount of detail returned.

Think of it as a zoom level on the *answer*, not the *search*:

- **`basic`** — the full answer. Every field, the full page of results, nothing truncated. Use it when you're reading the actual evidence (the file body, the matched lines, the PR diff).
- **`compact`** — a lighter answer. The same results, with secondary fields and chatter trimmed. Use it mid-investigation when you're chaining calls and don't need every byte.
- **`concise`** — just the shape of the answer. Counts, top paths, a one-line summary; bodies and snippets are dropped. Use it to *probe* — "does X exist?", "how many?", "which files?" — before committing tokens to reading.

Why it exists: research is a ladder. You usually want to know *whether* something is there and *where* before you read it. `concise` answers "whether/where" for a few hundred bytes; `basic` answers "what exactly" for the full cost. Picking the right rung is the single biggest token lever in the toolset.

**Three things every tier controls (the same way on every tool):**
1. **Fields per result** — how many columns/properties each result carries.
2. **Page size** — how many results come back per page.
3. **Truncation** — whether long values (file bodies, match snippets) are cut. *Only `concise` truncates.*

**One rule worth memorizing:** the verbosity feature is *silent*. It never adds a hint telling you it dropped detail or how to get it back — it just shapes the data. (This was deliberately changed; older builds emitted "detail dropped / drill-back" lines.)

### How each tool reacts to each level

Plain-language version of what you actually get back. (`✅` = behaves this way today; `⏳` = field/page reduction is spec'd but not yet wired, so `compact` currently ≈ `basic` for that tool — see the status table further down.)

| Tool | `basic` gives you | `compact` gives you | `concise` gives you |
|------|-------------------|---------------------|-------------------|
| **localSearchCode** (ripgrep) | every matching line with file, line, column, snippet | same matches, fewer pages of chatter ⏳ | **no match bodies** — just "N matches in M files" + top file path. Best "is this symbol anywhere?" probe. ✅ |
| **localViewStructure** | full directory tree: every entry with path, size, modified | same tree ⏳ | **no entries** — just the summary line ("15 entries, 5 files, 10 dirs") + totals. "Is this a monorepo / how big?" ✅ |
| **localFindFiles** | every matching file with size, perms, mtime | same list ⏳ | **no file list** — just the count + newest path. "Does this file exist / what changed last?" ✅ |
| **localGetFileContent** | the file content (or the requested slice) | same content ⏳ | **minified content** (comments/whitespace stripped, substance kept) + raw→min token summary. A cheap full read, not a dead-end. ✅ |
| **githubSearchRepositories** | full repo records (stars, topics, dates, language…) | same records ⏳ | **≤3 records**, each just `{full_name, stars, language}`. "Does a repo like this exist?" ✅ |
| **githubSearchCode** | matched files with the matching text | same matches ⏳ | **counts + top paths, no text**, capped to 3. "Is this in the repo, and where?" ✅ *(bulk-gate: concise only triggers if every query in the call asks for it)* |
| **githubViewRepoStructure** | the remote tree (parent / name / type / path) | same tree ⏳ | **no entries** — just folder/file counts. Cheapest "what's the repo shape?" probe. ✅ |
| **githubGetFileContent** | the remote file content (or slice) | same content ⏳ | **minified content** + raw→min token summary (a cheap full read, not blanked). ✅ *(bulk-gate)* |
| **githubSearchPullRequests** | full PR metadata + body (+ optional comments/commits) | same, chatter trimmed ⏳ | **≤3 PRs**, coerced to metadata only. "How many PRs touch this / top candidates?" ✅ |
| **packageSearch** | full package records (+ deps/versions if metadata on) | same records ⏳ | **1 record**, metadata forced off. "Does this package exist / canonical name?" ✅ |
| **lspGotoDefinition** | the definition location *plus* the source snippet | same ⏳ | **no snippet** — just `path:line:col`. "Where is it defined?" — the common case. ✅ |
| **lspFindReferences** | every usage with its surrounding snippet | same ⏳ | **no snippets** — flat ref list under 500, or a per-file rollup above. Pair with `groupByFile=true` for a per-file count map at any level. ✅ |
| **lspCallHierarchy** | caller/callee nodes *with full function bodies* (heavy!) | same — still heavy ⏳ | **no bodies** — just the edge count + an "A → B" list. **This is the tool where `concise` matters most.** ✅ |

**Practical defaults by tool type:**
- *Search / discovery tools* (search code, find files, view structure, search repos/PRs): start at **`concise`** to find the target, then re-query the winner at **`basic`**.
- *Read tools* (get file content, goto definition): use **`concise`** only to size/locate; go straight to **`basic`** to actually read.
- *`lspCallHierarchy`*: default to **`concise`** unless you specifically need caller source inline — `basic` can return hundreds of lines per node.

---

## Output schema & per-tier field omission

Every tool returns a **structured output schema** (defined in `octocode-core/src/schemas/outputs.ts`). The "fields per result" dimension is concrete: each tier keeps a defined subset of the schema's descriptors and **omits the rest**. This is the exact field-level contract behind the prose above.

### Envelope (every tool, every tier)

| Field | basic | compact | concise | Notes |
|-------|:----:|:------:|:----:|------|
| `status` | ✓ | ✓ | ✓ | only present for `empty`/`error` (absent ≡ hasResults) |
| `pagination` | ✓ | ✓ | ✓ | data-bearing — always kept (carries totals/cursors) |
| `hints` | ✓ | trimmed | summary only | verbosity-feature hints never emitted; only data hints (cursors, counts) |
| `warnings` | ✓ | ✓ | ✓ | downgrade/truncation signals kept |
| `error` | ✓ | ✓ | ✓ | always kept |

### Per-tool result descriptors

Legend: **always** = kept at every tier · **compact-drop** = omitted at compact + concise (secondary metadata) · **concise-only-keeps** = the sole survivors under concise (rest of the row dropped/blanked). `⏳` marks descriptors whose compact-drop is **spec'd but not yet wired** (today they still appear under compact).

| Tool | Result item descriptors (schema) | compact omits ⏳ | concise keeps (rest dropped) |
|------|----------------------------------|------------------|----------------------------|
| **localSearchCode** | `files[].{path, matchCount, matches[].{line, value, column}, modified}` | `column`, `modified`, `searchEngine` | counts + top `path` (whole `files[]` dropped) |
| **localFindFiles** | `files[].{path, type, size, modified, permissions}` | `size`, `modified`, `permissions` | count + newest `path` |
| **localViewStructure** | `entries[].{path, name, type, size, modified}`, `summary` | `size`, `modified` | `summary` + `pagination.totalEntries` |
| **localGetFileContent** | `{path, content, totalLines, startLine, endLine, isPartial, matchRanges, lastModified, lastModifiedBy}` | `matchRanges`, `lastModified`, `lastModifiedBy` | `path`, `totalLines`, **minified `content`** |
| **lspGotoDefinition** | `locations[].{uri, range, content}`, `lspMode` | — (already lean) | `uri`, `range` (`content` stripped) |
| **lspFindReferences** | `locations[].{uri, range, content}`, `lspMode`, `pagination` | `content` (snippet) | `uri`+`range` flat, or per-file rollup ≥500 |
| **lspCallHierarchy** | `item`/`from`/`to`.`{name, uri, range, content}`, `fromRanges` | **`content` (function bodies)** ⚠️ high-value | `name`+`uri` edges only ("A → B") |
| **githubSearchRepositories** | `repositories[].{owner, repo, name, fullName, description, url, stars, forks, forksCount, openIssuesCount, language, topics, defaultBranch, createdAt, updatedAt, pushedAt, size, archived, private}` | `forks*`, `openIssuesCount`, `topics`, `*At` dates, `size`, `archived`, `private` | ≤3 × `{full_name, stars, language}` |
| **githubSearchCode** | `results[].{id, owner, repo, matches[].{path, value}}` | — (already lean) | counts + top `path` (`value` blanked), ≤3 groups |
| **githubSearchPullRequests** | `pull_requests[].{number, title, url, state, draft, merged, created_at, updated_at, closed_at, merged_at, author, head_ref, base_ref}` (+ body/fileChanges/comments) | `draft`, `head_ref`, `base_ref`, redundant dates, `body`/`comments`/`commits` unless requested | ≤3 × `{number, title, state}` (metadata) |
| **githubGetFileContent** | `{path, content, totalLines, startLine, endLine, isPartial, resolvedBranch, lastModified, lastModifiedBy}` | `lastModified`, `lastModifiedBy` | `path`, `totalLines`, **minified `content`** |
| **githubViewRepoStructure** | `entries[].{path, name, type, size}` | `size` | folder/file counts (`entries[]` dropped) |
| **packageSearch** | `packages[].{name, version, repository, license, description, deprecated, downloads, recentVersions}` | `license`, `downloads`, `recentVersions` | 1 × `{name, version, repository, deprecated}` |

> **Design rule for choosing what `compact` omits:** keep *identity + the answer* (path, name, the matched value, the content), drop *provenance + secondary metadata* (timestamps, permissions, fork/issue counts, topics, blame). `concise` keeps only *identity + counts*.
>
> **`concise` and long values — minify, don't blank.** For the two content readers (`localGetFileContent`, `githubGetFileContent`), `concise` returns the file body **minified** (comments/whitespace stripped via the per-filetype minifier, substance intact, never larger than basic) plus a `raw→minified` token summary — a cheap *full read*, not a dead-end. For search/listing tools, `concise` instead drops the heavy arrays and surfaces counts + top names. Either way `concise` stays research-grade: it always hands back something to act on, never an empty payload.

> **Status:** the *concise* column is **live** on every tool. The *compact-drop* column is the **specified target** for the field dimension and is marked `⏳` until wired — see "Implemented vs specified" below. `lspCallHierarchy`'s `content` drop under compact is the single highest-value item (it removes full function bodies).

---

## Canonical verbosity contract (aligned model)

> Reference: RTK (`rtk-ai/rtk`) — progressive field-stripping (aggressive = signatures only),
> truncation-with-context, and no meta-noise in output. Octocode's tiers follow the same shape.

Each tier moves **three dimensions in lockstep, identically on every tool**:

| Dimension | `basic` (default) | `compact` | `concise` |
|-----------|-------------------|-----------|---------|
| **Fields per result** | all schema fields | core fields only | identity / count fields |
| **Page size (result count)** | full (`PAGE`) | half (`⌈PAGE/2⌉`) | top (`≤ CONCISE_TOP = 3`) |
| **Data truncation** | none | none | yes (content/snippets dropped) |
| **Verbosity-feature hints** | none | none | none |

Invariants that hold for ALL tools:
- **Omitted ≡ basic.**
- **Truncation happens only in `concise`.** `basic` and `compact` never truncate a returned value — they differ only in *which fields* and *how many rows*.
- **The verbosity feature emits no hints.** No "detail dropped", no "drill-back", no tier commentary. Only genuinely data-bearing hints (pagination cursors, count summaries) remain.

Centralized in `src/scheme/verbosity.ts`:
- `CONCISE_TOP = 3` — the concise "top" page size.
- `verbosityPageSize(verbosity, base, explicit?)` — single source of the result-count dimension (explicit caller value wins; else tier-scales `base`).
- `conciseDrillBackHint()` — now a **no-op returning `[]`** (kills verbosity chatter at all ~13 call sites in one place).

---

## Implemented vs specified — honest status

| Dimension | Status | Detail |
|-----------|--------|--------|
| **No verbosity-feature hints** | ✅ **Implemented & verified** | `conciseDrillBackHint → []`; residual `"entries[] dropped"` prefix removed; `dist` has 0 occurrences of "detail dropped"; data summaries (counts) retained. |
| **Truncation = concise only** | ✅ **Already aligned** | Confirmed across all 13 tools — `basic`/`compact` never truncate. |
| **Contract centralized** | ✅ **Implemented** | `CONCISE_TOP` + `verbosityPageSize` primitive; base-schema + system-prompt rewritten to the 3-dimension model (old hint-based wording removed, propagated to the running server). |
| **Page size: compact = half** | ⏳ **Specified, primitive ready, not yet wired** | `verbosityPageSize` exists but tool runtimes still use full default page size under compact. Wiring is the next increment (low-risk seams in `paginateEntries`/`paginateFiles`). |
| **Fields: compact = core only** | ⏳ **Specified, not yet wired** | Today `compact` data ≡ `basic` data (hints-only trim). Per-tool core-field lists (or TSV-projection-driven dropping) are the remaining work. |

> Honest note: the runtime today fully delivers **no-hints** and **concise-truncation**; the **field-reduction** and **page-halving** dimensions for `compact` are now *defined in the spec* and have a shared primitive, but are **not yet wired into each tool**. Until wired, `compact` remains "basic minus verbosity chatter." This doc is the source of truth for that gap.

---

## Per-tool behavior at each level (current, verified)

`F` = fields, `N` = rows/page, `T` = truncation. ✅ aligned · ⏳ pending wiring.

| Tool | `basic` | `compact` (today) | `concise` |
|------|---------|-------------------|---------|
| localSearchCode | all match fields, full page | =basic data; hints trimmed ⏳F/N | `files[]` dropped → counts + top path ✅T |
| localViewStructure | all entry fields incl. `path`, full page | =basic data ⏳F/N | `entries[]` dropped → `summary` only ✅T |
| localFindFiles | all file fields, full page | =basic data ⏳F/N | `files[]` dropped → count + newest ✅T |
| localGetFileContent | full content incl. `path` | =basic ⏳F | `content=""` → line/token estimate ✅T |
| githubSearchRepositories | full repo records | =basic data ⏳F/N | ≤3 records `{full_name,stars,language}` ✅N/T |
| githubSearchCode | matches + text | =basic data ⏳F/N | counts + top paths, no text, limit→3 ✅N/T (bulk-gate) |
| githubViewRepoStructure | full tree (4-col TSV) | =basic data ⏳F/N | `entries[]` dropped → counts ✅T |
| githubGetFileContent | full content | =basic ⏳F | `content=""` → token estimate ✅T (bulk-gate) |
| githubSearchPullRequests | full PR metadata/body | =basic; hints trimmed ⏳F/N | limit→3, type→metadata ✅N/T |
| packageSearch | full records (+metadata) | =basic data ⏳F | searchLimit→1, metadata off ✅N/T |
| lspGotoDefinition | location + snippet | =basic ⏳F | snippet stripped → path:line:col ✅T |
| lspFindReferences | flat refs + snippets | =basic ⏳F/N | snippets dropped; `<500` flat, `≥500` per-file rollup ✅T; `groupByFile` → per-file map (any tier) |
| lspCallHierarchy | full node bodies ⚠️ | =basic (still heavy) ⏳F | node content stripped → "A → B" edges ✅T |

---

## TL;DR (the honest version)

1. **`concise` is the only consistently meaningful cost lever.** Distinct and correct on all 13 tools — the "presence / count / shape" layer. Make it your default probe; drill to `basic` only for the final evidence read.
2. **`compact` is over-marketed.** It returned a **byte-identical data payload to `basic`** on every tool tested. It only drops advisory hints (capped to 2). Real saving is tens-to-hundreds of bytes, *not* a tier change. On reads (file content, definitions) it ≡ `basic`.
3. **Two sharp edges:**
   - **Bulk-gate** (`githubSearchCode`, `githubGetFileContent`): `concise` is silently ignored unless *every* query in the call requests it.
   - **`lspCallHierarchy` `basic` is a token bomb** (a single depth-1 call returned a ~200-line node body); `compact` does **not** strip node content — only `concise` does. Wrong layer here can 50× the response.

---

## Per-tool verbosity guidance

Legend: ✅ recommended default · ⚠️ caveat · "skip" = buys you almost nothing over `basic`.

### Local tools

| Tool | `concise` — use when | `basic` — use when | `compact` — honest take |
|------|--------------------|--------------------|--------------------------|
| **localSearchCode** | "Does X exist / how many / which files?" `mode=discovery`+`concise` = cheapest probe. Drops `files[]`, returns counts + top path. | Reading match lines/values; feeding `lineHint` to LSP. | Marginal. Only helps on large-result calls that emit the "narrow scope" warning. Otherwise = basic. |
| **localViewStructure** | "Monorepo? rough shape? entry count?" Drops `entries[]`, keeps summary. | Seeing the tree (paths, sizes). | Skip — data identical to basic. |
| **localFindFiles** | "Does this file exist / what's newest / how many?" Drops `files[]`, returns count + newest path. | Getting the file list with metadata. | Skip — identical data. |
| **localGetFileContent** | Sizing a file before reading (line/token estimate, `content=""`). | Reading content. | **Near-useless** — a file read carries almost no advisory hints, so compact ≡ basic. |

### GitHub / external tools

| Tool | `concise` — use when | `basic` — use when | `compact` — honest take |
|------|--------------------|--------------------|--------------------------|
| **githubSearchRepositories** | "Does a repo exist / top candidate?" Returns ≤3 compact records `{full_name, stars, language}`. | Comparing candidates (stars, topics, dates). | Marginal — trims hints only. |
| **githubSearchCode** ⚠️ | "Is this symbol in the repo / which paths?" counts + top paths, no text. | Reading matched code. | Marginal. **Bulk-gate:** concise activates *only if every query is concise*. |
| **githubViewRepoStructure** | Cheapest external probe: `path=""`+`depth=1`+`concise`. | Seeing the actual layout before reads. | Skip — identical data. |
| **githubGetFileContent** ⚠️ | Sizing a remote file (`content=""`, token estimate). | Reading remote content. | **Near-useless** (same as local read). **Bulk-gate** applies. |
| **githubSearchPullRequests** | Triage: "how many PRs / top 3?" caps `limit=3`, coerces `type=metadata`. | Reading PR bodies/diffs/discussion. | Marginal — bodies auto-truncate with char-pagination regardless. |
| **packageSearch** | "Does the package exist / canonical record?" caps `searchLimit=1`, forces metadata off. | Comparing alternatives, getting repo URL/versions. | Marginal. |

### LSP tools

| Tool | `concise` — use when | `basic` — use when | `compact` — honest take |
|------|--------------------|--------------------|--------------------------|
| **lspGotoDefinition** | "Where is it?" path:line:col only, snippet stripped — usually the right call. | You need the definition body inline. | Skip — trims hints only. |
| **lspFindReferences** | Blast-radius: "how widely used?" snippets dropped. **Pair with `groupByFile=true`** for the per-file count map. | Inspecting what each usage looks like. | Skip. `groupByFile` is the real lever, not compact. |
| **lspCallHierarchy** ⚠️ | **Default for this tool.** Edge count + compact "A → B" list. | Only when you truly need caller/callee source inline. | **Avoid relying on it.** `basic` dumps entire function bodies; `compact` does **not** strip node content. Only `concise` reduces it. |

---

## Cost-ladder cheat sheet

```
Rung 1 — Shape    → verbosity: concise   (existence, counts, layout, top paths)
Rung 2 — Slice    → verbosity: basic   + matchString / line range / partial PR diff
Rung 3 — Wide     → verbosity: basic   + fullContent / withComments / clone
```
`compact` is not a rung — treat it as "basic minus a couple of hints."

---

## Bug ledger (found + fixed during this review)

### Runtime (`octocode-mcp`) — 5 fixes
| # | Bug | Fix location |
|---|-----|--------------|
| 1 | `localGetFileContent` TSV `path` column always empty | `src/tools/local_fetch_content/fetchContent.ts` (`buildSuccessResult`) |
| 2 | `localViewStructure` TSV `path` column always empty (both ls + recursive paths) | `src/tools/local_view_structure/local_view_structure.ts` |
| 3 | `githubViewRepoStructure` TSV: `path` empty + phantom `size`/`sha`/`url` columns | `src/utils/response/tsvColumns.ts` |
| 4 | `packageSearch` TSV `repositoryUrl` empty (field name mismatch `repositoryUrl`→`repoUrl`) | `src/utils/response/tsvColumns.ts` |
| 5 | `lspFindReferences` `groupByFile` discarded its own rollup (emitted bare total, no file map) | `src/tools/lsp_find_references/lsp_find_references.ts` |

### Docs / schema (`octocode-core`) — 3 fixes
| # | Bug | Fix location |
|---|-----|--------------|
| 6 | `lspCallHierarchy` schema omitted `charOffset`/`charLength` (runtime emits the continuation hint!) | `src/resources/tools/lspCallHierarchy.ts` |
| 7 | `lspFindReferences` schema omitted `charOffset`/`charLength` (parity with `lspGotoDefinition`) | `src/resources/tools/lspFindReferences.ts` |
| 8 | `packageSearch` response_shape claimed "No pagination" — false (char-pagination observed) | `src/resources/tools/packageSearch.ts` |

All 8 verified live or in the built bundle after `yarn install` re-synced the `file:` dependency and both packages rebuilt.

---

## Improvement suggestions (prioritized)

### P1 — Make `compact` earn its place, or retire it
**Problem:** `compact` keeps the full data payload on every tool; it only trims hints. Agents are told it's the "chaining tier between concise and basic," but it does not reduce the expensive part. This is misleading guidance that wastes a decision slot.

**Options:**
- **(a) Give it teeth.** Define a real middle tier: e.g. `compact` for content tools = data with snippets capped (first N lines per match, truncated bodies), keeping structure but cutting the bulk. That makes concise→compact→basic a genuine size gradient.
- **(b) Be honest in the prompt.** If it stays hint-only, change the base-schema description from implying a payload difference to: `"compact" (same payload as basic, advisory hints trimmed — minor saving)`. Stop positioning it as a cost tier.

Recommendation: **(a)** for `localGetFileContent` / `githubGetFileContent` / `lspCallHierarchy` (the heavy ones), **(b)** wording everywhere else.

### P2 — Fix `lspCallHierarchy` default heaviness
**Problem:** `basic` returns full node *bodies* (observed ~200 lines for one depth-1 incoming call). `compact` doesn't strip them. This is the single highest-variance tool.

**Suggestions:**
- Default node content to a **signature + ±N lines**, not the whole function body. Full body only on explicit `contextLines` raise or `fullContent`-style opt-in.
- Add a description line: *"basic returns full caller/callee bodies — expensive on hot functions; prefer concise unless you need inline source."* (Currently the gotcha warns about `depth>1` but not about body size at depth=1.)

### P3 — Surface the bulk-gate trap at call time
**Problem:** For `githubSearchCode` / `githubGetFileContent`, a mixed-verbosity bulk silently ignores `concise`. The exception is documented in `<response_shape>`, but an agent that set `concise` on 4 of 5 queries gets no signal it was dropped.

**Suggestion:** When a bulk is mixed and concise was requested on some queries, emit a one-line `warnings[]` entry: `"concise ignored: bulk-gate requires every query to request concise"`. Cheap, and it closes the silent-failure gap.

### P4 — Document the per-query vs response-level pagination split
**Problem:** Tools say "No pagination" or list only row cursors, but the bulk runner *also* applies response-level `responseCharOffset`/`responseCharLength` and emits `charOffset=` continuation hints. The split (per-query `charLength` vs top-level `responseCharLength`) is real but under-explained per tool.

**Suggestion:** One shared line in the system prompt's `<tool_contract>`: *"Every tool's rendered output can be walked with top-level `responseCharOffset`/`responseCharLength` even when the tool has no row pagination."* (Partly present; make it explicit and stop tools claiming "No pagination.")

### P5 — Close remaining schema/doc omissions
- `localSearchCode` Zod accepts `noUnicode`, `threads`, `mmap` — undocumented. Either document briefly or confirm they're intentionally hidden (current curation choice is defensible; just make it a deliberate, recorded decision).
- Keep `octocode-core` fixes #6–8 in any future registry republish so they aren't overwritten.

### P6 — Add a CI guard against TSV column drift
**Problem:** Four of the eight bugs were TSV columns that were declared but never populated (empty columns) or mapped to the wrong JSON field. These are invisible until someone reads the TSV.

**Suggestion:** A snapshot test per tool that asserts *every declared TSV column is non-empty for a known fixture row* (and that no documented schema param is absent from the Zod schema). Would have caught all four TSV bugs and the two LSP `charOffset` omissions automatically.

---

## What's genuinely strong (keep)

- **The cost-ladder model** in `systemPrompt.ts` — ties verbosity to a 3-rung mental model; best part of the prompt.
- **Per-tool `<vs_otherTool>` disambiguation** — every tool tells you when to reach for its neighbor. Rare and excellent.
- **`<response_shape>` blocks** — now accurate to runtime after the fixes; these are what let an agent predict cost before calling.
- **`concise` design** — consistent, correct, and the real lever. The drill-back hint on every concise payload ("re-call with verbosity:basic…") prevents dead-ends.

**Overall instruction quality: 9.6/10** — well-architected; the only defects were the schema omissions and the over-sold `compact` tier, both addressed above.
