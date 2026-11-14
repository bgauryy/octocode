# Final verification and improvements — 2026-11-14

**Completed: all 17 tools returned usable results on the rebuilt CLI and fresh MCP. Overall scoped rating: 4.51/5.** Exact selected evidence and executable handoffs passed. LSP still discloses three omitted call lines; its pagination is complete for the reported rows, while semantic coverage remains partial.

The final host build is Octocode 20.2.0, macOS arm64, SHA-256 `7a4d84e20056ccb31ce5cd34c077df003b59d6910c3b0e290356527fd2bc9efb`. Its embedded core contract fingerprint is `ad0d80ae7c8cb17f304542b49f6e9bad2b86e3a176cd700706a5c4f90440ccf7`. This is a working-tree build with concurrent changes, not a clean-commit release certification. [Binary receipt](../../../campaigns/20.2.0-2026-11-14-final-verification/final-binary.json).

## What changed


| Finding | Affected boundary | Reproduction and repair | Verification |
|---|---|---|---|
| Registry 429 could bypass origin revalidation requirements | artifactSearch registry transport; registry-backed security metadata | Cached ETag answers with no-cache/expired must-revalidate were served after 429. Reuse existing shared SavedAnswer policy; retain ordinary permitted fallback. | Two new tests failed before fix and pass afterward. |
| Superseded registry bodies survived restrictive or malformed replacements | Same shared registry client | 304 no-store, uncacheable 200 and malformed successful replacement left old data available. Invalidate at each owning replacement boundary. | Three new tests failed before fix and pass afterward, including independent review's malformed-body counterexample. |
| Workflow shaping could remove later preferred batches | Shared tool response shaping | Later batches with identities already shown were dropped. Keep all ordered batches for the first preferred available tool. | Generic all-tool/fallback regression red→green. |
| Related-call identities could collide with substrings or selectors | Shared workflow shaping | A path prefix or path literally named content was mistaken for an already-stated identity/enum. Compare structured scalars exactly; enumerate selector values by property. | ASCII/Unicode and enum-spelling regressions red→green; prose warning references remain supported. |
| A duplicate failed sibling could remove an own retry | Shared related-call deduplication | An identical sibling query suppressed each failed row's own retry. Preserve the own query before sibling matching. | Failed-sibling regression red→green; next unchanged. |
| Repeated full paths inflated LSP warnings | Semantic answer-gap disclosure | Group line positions by file while keeping total site counts, confirmed omissions, unverified spellings and recovery queries. | Multi-file/Unicode regression red→green. Fresh output saves 108 characters (5.5%) with the same sites and recovery lead. |
| Every page scanned/sorted every fragment | Shared response pager | Placement already orders page, row, part. Select a borrowed range with partition_point; remove temporary vector and repeated sorting. | Independent producer-invariant review; existing page-order, exact-union, lazy-materialization and 1,200-shape tests pass. Isolated wall-time speedup not claimed. |
| Compact-reader guidance implied matching was supported | Canonical core shared local/GitHub read descriptions | Explicitly distinguish content matching from compact whole-file/range reads. Correct core README's normalization fact. | Core regression red→green; final core has 1,586 tests, context budgets, typecheck/lint passing; regenerated text verified. |
| A loading-era empty LSP answer could become final after loading ended | LSP readiness judgment | Record both begin count and active work at question time under the existing token lock. A load ending before judgment must invalidate the old empty answer; a later idle observation cannot validate it retroactively. | Two deterministic tests failed before repair; both now pass. The original deadline/retry test also passes, without changing its timeout. |
| GitHub tag output and schema disagreed | Canonical core output schema and native refs producer | Native returns tag name plus resolved commit SHA; the schema still required bare strings. Reuse the strict named-ref schema for branches and tags, retain collection routing, and migrate the stale round-trip fixture. | Two schema regressions failed before repair; full core now has 1,586 passing tests. Generated contract checks pass. |
| CLI repeated graph facts already stated by a sibling row | CLI text renderer | An external owner added a comparison-only reconstruction from canonical batchStatedOnce metadata. Original rows still own their rendered facts; unrelated missing values are not inferred. | Fresh Astra source review accepted it; the previously failing same/mixed-snapshot regression passes in the final full run. |
| New Rust API uses exceeded the shrink-only lint baseline | Seven native source paths plus one refs test | Replace unchecked string indexing with checked byte/string access or trim/split APIs. Keep justified temporary-buffer cuts and cancellable OS-lock polling with statement-scoped explanations; prove the polling runs on blocking workers. Collapse repeated character replacement in the test. | Both Clippy phases pass. Baseline refreshed with six crate totals lowered and no increase. See astra-api.md for source and dispatch proof. |

The initial 10 new native regressions failed on actual execution before production edits; all 112 selected native/field-effect checks then passed. The subsequent LSP repair adds two deterministic red-to-green regressions. No limits, coverage floors or context budgets were weakened. No new cache layer, dependency, wire type or generated-file hand edit was introduced. Changes preserve prior dirty work.


## Verification and workflow

- Full canonical Rust suite: **8,603 passed, 16 skipped** (6,943 native/CLI/provider; 1,660 engine). The prior renderer, tag-schema and LSP failures all pass. The 35 tracked source fingerprints stayed unchanged during the final suite. [Full receipt](../../../campaigns/20.2.0-2026-11-14-final-verification/rust-final-all.log).
- Rust lint: both Clippy passes and the shrink-only API ratchet pass; six crate warning totals were lowered. Targeted formatting check passes for all 27 tracked Rust files. No warning ceiling or coverage floor was raised.
- Core: **1,586 tests**, context-budget checks, lint and typecheck pass. Contract regeneration and both tool/config consistency checks pass. All 17 served schemas and descriptions agree across CLI, MCP and generated presentation; the identical LSP server-discovery insertion is intentional. Independent review also verified 34 typed invalid-input refusals without provider work.
- Non-Rust `test:ci`, `lint:ci`, `typecheck:ci`, skills and documentation checks pass. The final host `build:dev` and `check-outputs` pass. Documentation was checked again after the final comparison guidance edit.
- Root exercised seven local/graph tools; Sol exercised ten remote/classification tools; Astra independently verified schema parity, response pages, tool pages, stale restart and cross-surface replay. Relevant local skills covered development, orchestration, Rust, contracts/Zod, agent behavior, workflows, debugging, engineering and documentation.

```mermaid
flowchart LR
  C[Core schemas and descriptions] --> G[Generate owning contract]
  G --> R[Test and build host Rust]
  R --> P[Pin executable and start fresh MCP]
  P --> D[Discover exact source identity]
  D --> E[Read or navigate evidence]
  E --> Q{More selected evidence?}
  Q -->|Response pages first| E
  Q -->|Then tool continuation| D
  Q -->|Exhausted| A[Verify source and rate result]
```

Observed workflows include local search→LSP→disclosed text recovery, AST→source ranges, graph build→stats→hubs, GitHub symbol search→pinned function, bounded history→scoped patch→before/after files, package→repo→versioned source, security summary→details, clone→local search/read, and classification→saved deciding read. Emitted calls were copied unchanged.

## Ratings for every tool

Scale: 5 excellent in the exercised scope; 4 good with caveats; 3 material friction; 2 weak; 1 failed. Scores are reviewer judgments. Schema covers correctness and ease of requesting the exercised operation; parity itself passed for every tool. Cache scores use observed replay/invalidation, snapshot reuse or explicit cached-observation markers. **N/A means unobserved or inapplicable**, not unsupported or passed. Overall is the mean of 122 scored cells; broader operation coverage could change it.

| Tool | Workflow | Schema | Results | Output efficiency | Speed | Quality | Pagination | Caching | Mean |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| structureSearch | 5 | 4 | 5 | 4 | 5 | 5 | 5 | 4 | 4.6 |
| localSearch | 5 | 4 | 5 | 3 | 4 | 5 | 5 | 4 | 4.4 |
| localFetch | 5 | 5 | 5 | 4 | 5 | 5 | 5 | 5 | 4.9 |
| astSearch | 4 | 3 | 5 | 4 | 5 | 4 | 5 | 4 | 4.2 |
| lspSearch | 4 | 3 | 3 | 3 | 2 | 4 | 5 | 4 | 3.5 |
| graphBuild | 4 | 4 | 4 | 4 | 4 | 4 | N/A | 5 | 4.1 |
| graphQuery | 5 | 3 | 4 | 4 | 5 | 4 | 5 | 4 | 4.2 |
| ghSearchRepo | 5 | 5 | 5 | 4 | 3 | 4 | N/A | N/A | 4.3 |
| ghStructure | 5 | 5 | 5 | 4 | 4 | 5 | 5 | N/A | 4.7 |
| ghSearchCode | 5 | 5 | 5 | 4 | 4 | 5 | N/A | 5 | 4.7 |
| ghGetFileContent | 5 | 5 | 5 | 4 | 5 | 5 | 5 | N/A | 4.9 |
| ghSearchHistory | 5 | 5 | 5 | 4 | 4 | 5 | 5 | N/A | 4.7 |
| ghGetHistoryItem | 5 | 5 | 5 | 4 | 5 | 5 | 5 | N/A | 4.9 |
| artifactSearch | 5 | 5 | 5 | 4 | 4 | 5 | N/A | N/A | 4.7 |
| securityCheck | 5 | 5 | 5 | 4 | 4 | 5 | N/A | 5 | 4.7 |
| ghCloneRepo | 5 | 5 | 5 | 4 | 4 | 5 | N/A | 5 | 4.7 |
| clasify | 5 | 5 | 4 | 5 | 4 | 4 | N/A | N/A | 4.5 |

## Measured calls and output

Milliseconds, first→repeat. These are sequential observations, not percentiles or origin-cold measurements. CLI includes process startup; MCP uses a fresh server but may share existing provider/disk caches. Corrected remote CLI measurements ran after earlier MCP calls. JSON sizes are compact result characters, including navigation. Both text streams must be counted: optional related commands and counters are on stderr.

| Tool | CLI ms | MCP ms | JSON chars CLI first→repeat | JSON chars MCP first→repeat |
|---|---:|---:|---:|---:|
| structureSearch | 101→57 | 9→4 | 353→353 | 353→353 |
| localSearch | 78→60 | 23→12 | 716→716 | 716→716 |
| localFetch | 53→53 | 5→2 | 1381→1381 | 1381→1381 |
| astSearch | 75→57 | 21→7 | 556→556 | 556→556 |
| lspSearch | 3298→1608 | 3392→1508 | 1866→1866 | 1866→1866 |
| graphBuild | 314→99 | 21→17 | 849→625 | 625→625 |
| graphQuery | 65→56 | 12→4 | 1247→1247 | 1247→1247 |
| ghSearchRepo | 3116→77 | 10→3 | 486→486 | 486→486 |
| ghStructure | 416→84 | 20→7 | 743→743 | 743→743 |
| ghSearchCode | 932→81 | 13→6 | 620→656 | 656→656 |
| ghGetFileContent | 86→90 | 6→3 | 697→697 | 697→697 |
| ghSearchHistory | 456→78 | 22→11 | 938→938 | 938→938 |
| ghGetHistoryItem | 123→120 | 24→16 | 1291→1291 | 1291→1291 |
| artifactSearch | 780→338 | 319→256 | 398→398 | 398→398 |
| securityCheck | 1911→125 | 14→8 | 564→600 | 600→600 |
| ghCloneRepo | 667→197 | 65→57 | 558→558 | 558→558 |
| clasify | 458→— | 272→— | 281→— | 281→— |

Clasify's row is one paid evaluation per pinned surface, with no paid repeat. Nonbilling model discovery measured 240→250 ms on CLI and 167→166 ms on MCP. The graphBuild first CLI call rebuilt the small snapshot; later calls reused it. The fresh owned depth-3 clone measured 4,661 ms, followed by 81 ms MCP reuse; the normal clone row measures an existing checkout.

## Pros, cons and gotchas

| Tool | Pros | Cons / gotchas |
|---|---|---|
| structureSearch | Compact grouped paths; exact 80-file walks. | Defaults select a scope; details/line-count sorts read files. Raw rg listing is smaller/faster here. |
| localSearch | Exact counts and ranked source; LSP handoff; agrees with rg. | Full path prefixes inflate file/count output. Match occurrences differ from matching lines. |
| localFetch | Exact ranges and complete declarations; strong repeat reuse; Unicode recovery. | Understand response fragments, source-line pages and byte windows; totalLines is whole-file length. |
| astSearch | Seven call expressions exclude the declaration; useful read ranges. | Rule grammar adds complexity; syntax matches need source/LSP identity checks. |
| lspSearch | Cross-package callers, explicit gaps, working recovery. | About 3.3 s first / 1.5–1.6 s repeat; three omitted call lines remain outside semantic results. |
| graphBuild | Snapshot identity, linkage and diagnostics; reuse is cheap. | Small three-file scope only; five outside-root imports are disclosed. Build receipts are atomic here. |
| graphQuery | Freshness and ranked hubs; shared graph facts print once. | Many operations; syntax edges do not prove runtime behavior or dead code. |
| ghSearchRepo | Exact repo identity and executable discovery leads. | First observed CLI lookup 3.1 s; get was tested, broad search pagination was not. |
| ghStructure | Exact tree and name/SHA tag collections; cross-surface replay. | Tiny pages add overhead. Exact ref resolution differs from listing a tag collection. |
| ghSearchCode | Source-pinned hit→function workflow; explicit cached observation. | GitHub index/provider coverage limits remain; broad search pagination not exercised. |
| ghGetFileContent | Complete selected source matches GitHub decoding exactly. | Tool/response pages differ; block reads and compact range reads have different selectors. |
| ghSearchHistory | Exact ordered bounded history and patch handoff. | Nine one-row pages are a stress case; commits outside 2026 were outside the selected scope. |
| ghGetHistoryItem | Complete selected patch and before/after reads. | include selected one of 246 changed files; the other 245 were not requested. |
| artifactSearch | Version→repo/tag/commit agrees with registry and package.json. | npm resolve only; extra verification costs more than a projected registry request. |
| securityCheck | OSV advisory set agrees; summary/details and cached observation work. | A selected package check is not a universal security certification; withdrawn entries need context. |
| ghCloneRepo | Clean reuse and fresh checkout both verified; local tools work immediately. | New download is much slower than reuse; preserve pre-existing or dirty checkouts. |
| clasify | Small answer with confidence and an exact deciding source read. | Paid, nondeterministic judgment; one public fact cannot establish broad classifier accuracy. |

## Pagination evidence

| Selected scope | Complete walk evidence |
|---|---|
| structureSearch | Twelve final MCP walks, six response pages each: all 80 Unicode-named files exactly once. |
| localFetch | CLI and MCP: 29-page mixed content/compact/symbols batch; four-page long Unicode value; staleSnapshot followed by exact 11-page restart. |
| localSearch | CLI and MCP: five tiny response pages for nine hits; four tool pages at limit 2 for eight known hits. |
| astSearch | CLI and MCP: four tool pages at limit 2, seven call expressions exactly once. |
| lspSearch | CLI and MCP: five tool pages at limit 1, all five reported callers once; semantic partial status preserved. |
| graphQuery | CLI and MCP: two hubs pages at limit 2, three ordered rows; equals unlimited baseline. |
| ghStructure | Each surface: seven tree pages / 34 entries and two tag pages / two name-SHA objects. |
| ghGetFileContent | Each surface: eight pages reconstruct all 28 selected lines. |
| ghSearchHistory | Each surface: nine pages reconstruct nine commits in the selected date range. |
| ghGetHistoryItem | Each surface: two patch pages reconstruct 478 selected patch characters. |
| Other cases | Exact repo lookup, package resolve, security summary, build/clone receipts and one classifier answer had no collection walk to score. Broad ghSearchCode paging was not exercised. |

All selected walks finished response pages before tool continuations. Long indivisible/Unicode values retain their evidence and oversized disclosure. Stale old pages were discarded before the emitted restart. Local and GitHub cursors resume unchanged across CLI/MCP when binary, effective configuration and workspace identity match.

## Cache and speed comparison

Same 11,970,000-byte file and 80 Unicode-named files; three fresh MCP processes per storage mode, with isolated Octocode homes. No user cache was cleared. Each process performed first/repeated range reads and two complete listing walks. Final ranges match the baseline source exactly; all 12 final listing walks reconstruct 80 files once. Every final walk is six pages / 6,552 compact result characters.

| Mode / whole operation | Baseline first→repeat ms | Final first→repeat ms |
|---|---:|---:|
| Memory / large-file range | 777.01→14.21 | 575.56→11.14 |
| Persistent / large-file range | 759.93→13.96 | 575.40→11.41 |
| Memory / complete listing walk | 21.95→16.17 | 14.06→10.52 |
| Persistent / complete listing walk | 38.18→37.01 | 27.54→23.17 |

The repeated range is about 50× faster than its first read in this final setup. Baseline/final wall times are shared-host development-build observations with other source changes; they do not isolate the page-slice optimization or establish release performance. The page selection's removal of a full scan/allocation/sort is established by source and invariant tests. Persistent page replay still costs about twice the memory walk here. [Complete samples and unions](../../../campaigns/20.2.0-2026-11-14-final-verification/performance-comparison.json).

Output improvements: the same LSP result is 1,866 versus 1,974 compact characters, saving 108 (5.5%) without losing sites or recovery. The two-row graph stats text is 636 stdout + 964 stderr = 1,600 characters, versus 2,366 JSON characters; all four related commands remain on stderr. This is a different representation, not a wire-equivalent compression claim.

## Expert comparisons and next optimizations

- **rg:** same 80 file identities, 160 occurrences and 80 matching lines. Five-run medians: listing 8.70 ms / 3,280 characters; matching-file list 11.38 ms / 3,280; occurrence counts 10.96 ms / 3,440. Final Octocode MCP matching-file and count calls took 19.04/17.26 ms and 10,661/11,879 characters. These include much longer repository-relative paths and navigation. Raw rg wins this lexical workload; use `--no-config`, literal `-F`, matched ignore/case/word policies and correct line-versus-occurrence counts. Remove prefix caps when checking complete sets.
- **gh:** project needed fields and finish real pages. Lean repo lookup took 426 ms / 204 characters; selected raw file read took 325 ms / 417 characters; scoped patch 663 ms / 684 characters. Octocode adds source identity, labels and executable handoffs. Tiny one-row stress walks are not efficient production page sizes. Exact evidence comparisons are in the remote report.
- **Caching:** retain origin revalidation/no-store policy, invalidate superseded bodies, and bind reuse to identity and source freshness. Cache replay and immutable graph/clone reuse are useful; blanket stale LSP answers or vulnerability verdicts are not justified.
- **Next measured target:** profile persistent replay's disk/decode path and move work through the existing bounded, cancellable worker ownership where necessary. Evaluate retaining decoded plans within the existing memory budget before adding another cache or TTL. The current microbenchmark does not isolate disk I/O from decoding/materialization.
- **Output target:** file/count views repeat long path prefixes. Evaluate an explicit base-path representation across core, leads, renderers and pagination tests together. The current full paths remain correct; no ad-hoc path stripping was introduced.
- **Measure before expanding:** trace origin-request counts, CPU/allocation and peak memory; profile repeated GitHub tree selection and registry search tails. These remain candidates rather than claimed fixes.

The comparison owner now documents rg/gh practices and capturing both CLI streams or `--json`: [COMPARISONS](../../packages/octocode-benchmark/COMPARISONS.md). Primary references: [ripgrep guide](https://github.com/BurntSushi/ripgrep/blob/master/GUIDE.md), [gh api manual](https://cli.github.com/manual/gh_api), [HTTP cache stale policy](https://www.rfc-editor.org/rfc/rfc9111.html#section-4.2.4), [Rust partition_point](https://doc.rust-lang.org/std/primitive.slice.html#method.partition_point).

## Provenance, limits and cleanup

The earlier Rust Performance Best Practices session was retrieved and its leads checked. Previous scan-proof reuse and low-memory budget repairs remain covered; this round repairs fragment selection and cache correctness. All 17 field inventories had no unresolved field-effect classification. No unused-code claim relies only on a textual zero-hit count.

Two measurement mistakes were resolved before scoring: a Node REPL closure retained a pre-pin environment, and a text-output observation inspected stdout without stderr. The affected remote CLI calls were rerun with explicit bound environment; actual executable/cwd/environment equality and identical cross-surface cursors were independently verified. Unpinned receipts are quarantined and excluded. Three tiny public-source classifier evaluations were made in total, including the excluded first CLI call: reported aggregate usage 1,623 input / 60 output tokens. No further paid repeat was performed.

Remaining limits: selected operations on one macOS development build; 16 Rust tests skipped by the suite; no exhaustive provider/auth/grammar/platform or release-load certification. LSP still omits call lines 56, 60 and 76 in native-fast-path.test.ts; the emitted text lead was read, which does not turn those into returned semantic callers. Provider request counts and peak memory were not instrumented. The extra diagnostic detail on invalid compact+matchString remains a usability candidate; canonical guidance now distinguishes the valid forms.

Cleanup completed: owned pins, performance fixtures and isolated homes removed; no process using either owned pin remains. Agents verified their MCP/daemon/language-server processes and clean temporary clones/fixtures were removed. Safe `prune:rust` passed, removing 6,484 obsolete entries / **48.0 GiB** while retaining current build/test/lint outputs. Disk has **494 GiB free**. The normal staged build still matches the audited SHA; existing user caches/clones and other sessions' work were preserved. [Cleanup receipt](../../../campaigns/20.2.0-2026-11-14-final-verification/cleanup-final.json).

Evidence: [independent Astra review](../../../campaigns/20.2.0-2026-11-14-final-verification/astra-pinned.md), [Sol tool report](../../../campaigns/20.2.0-2026-11-14-final-verification/sol-acceptance/report.md), [cache/readiness review](../../../campaigns/20.2.0-2026-11-14-final-verification/astra-runtime.md), [Rust API proof](../../../campaigns/20.2.0-2026-11-14-final-verification/astra-api.md), [full scorecard](../../../campaigns/20.2.0-2026-11-14-final-verification/final-scorecard.json), [local evidence checks](../../../campaigns/20.2.0-2026-11-14-final-verification/root-acceptance/evidence-checks.json).

Full evidence directory: [final-verification](../../../campaigns/20.2.0-2026-11-14-final-verification/). Temporary executable pins referenced by historical receipts have been removed; the normal local build remains installed.
