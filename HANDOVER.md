# Handover

State of the 20.2.0 line on 2026-11-20, and everything left to do. Fix round 4 fixed every high finding of rating
round 2. Rating round 3 then found 3 new blocking defects (astSearch, lspSearch, graphQuery), so the line is **Not ready**.
Nothing is committed or published.

Evidence lives in the benchmark package; this file is the to-do list:
- Method: [packages/octocode-benchmark/briefs/independent-raters.md](packages/octocode-benchmark/briefs/independent-raters.md)
  and the 12-dimension rubric in [RATINGS.md](packages/octocode-benchmark/RATINGS.md#agent-experience-rubric-12-dimensions).
- Round 2 (before fix round 4): [scorecard](packages/octocode-benchmark/results/rounds/20.2.0-2026-11-20-independent-ratings/SCORECARD.md),
  [58 findings](packages/octocode-benchmark/results/rounds/20.2.0-2026-11-20-independent-ratings/FINDINGS.md).
- Round 3 (current build): [scorecard](packages/octocode-benchmark/results/rounds/20.2.0-2026-11-20-ratings-round-3/SCORECARD.md),
  [48 findings with exact inputs](packages/octocode-benchmark/results/rounds/20.2.0-2026-11-20-ratings-round-3/FINDINGS.md).

## Current state

| Item | Value |
|---|---|
| Build | `octocode 20.2.0+4476eb38`, `packages/octocode-native/npm/darwin-arm64/octocode`, SHA-256 `cb4b7786…a422`, built 2026-11-20 13:08 (macOS arm64 only). Later edits were test comments and docs only. |
| Contract | fingerprint `4476eb38`, generated from the local octocode-core (not the published 21.2.0) |
| Tree | HEAD `6717848eb`; about 239 uncommitted files here (fix round 4 plus another session's reorganization of `packages/octocode-benchmark`), 212 in `../octocode-mcp-host` |
| Context surface | default 51,993 / clasify 66,488 / all tools 84,631 bytes, under the 52,000 / 66,500 / 84,700 ceilings. Default has 7 bytes of headroom: any new default-tool text needs a cut in the same regen. |
| Gates on this build | `verify`: 9,385/9,385 unit, 481/482 process (see T1); `lint:rust` clean; `docs:verify`, `docs:rust`, `skills:check`, `test:ci`, `check-outputs` pass; core lint and 1,866/1,866 tests pass |
| Ratings (round 3) | rater average 8.2/10 (round 2: 8.0), correct 9.4; astSearch, lspSearch, graphQuery capped at 4 by the correctness gate |

## Owner steps (release blockers)

| # | Step |
|---|---|
| O1 | Publish octocode-core first: the local core differs from the published 21.2.0, but `packages/octocode-config/contract/provenance.json` says 21.2.0. Bump core, publish, repin it in `packages/octocode-config/package.json`, run `contracts:regen`, rebuild. |
| O2 | Commit: fix round 4 (this repo and the core repo). Another session's benchmark reorganization is in the same tree: decide what ships together. |
| O3 | Build and check Windows and Linux (`build:publish`); issues #477–#479 were reported on Windows. |
| O4 | Confirm the npm `latest` tags: `octocode-mcp` 19.1.0, `octocode` 20.1.0. |
| O5 | GitHub issues #473 and #477–#482 are fixed in this build but open: close them with a comment each, now or after publishing. |
| O6 | Restart running MCP sessions; they serve an older build. |
| O7 | Human review of L1 (localSearch redaction-safe matching, `engine/src/search/text_search/written.rs`, `runtime-base/src/security/judged/text.rs`): it changes the redaction path. |
| O8 | Decide P6f: reading a scope's package list (371 KB for `@types`) on every scoped npm miss, to lead `@types/nodee` to real names. |
| O9 | Write the release notes from the behavior-change table below. |

## Behavior changes to announce (fix round 4)

| Change | Effect on callers |
|---|---|
| clasify cost gate | Without `maxRequests`, a row sends at most one first-round request per item it names; a plan that expands (files, hits, globs, sections, an oversized source) sends nothing and returns its price plus `next.confirm` |
| clasify nested queries | A mistyped field in a billed call's `source`/`resources` query is refused, not repaired |
| ghSearchHistory | A `pullRequest` search with no repository needs an owner, keywords or a filter |
| graph snapshots | `ANALYSIS_VERSION` 40 → 41: older snapshots are refused once with a rebuild call |
| graph ids | Symbol ids are named from the workspace root, like file ids; old spellings and cursors still resolve |
| graphQuery `impact` | Summary lists are counts; lists page through `next.testsToRun`, `next.affectedEntrypoints`, `next.unmapped` |
| localSearch masking | Placeholder text the redaction writes is never matched; empty rows say masked secrets are not searched |
| securityCheck details | A later record of the same advisory drops identical `summary`/`details` and points to the earlier one with `textSameAs` |
| ghStructure walks | No fixed read cap; a walk pauses at its time budget and resumes through `next.nextPage` (`walkOffset`) |
| GitHub cursors | GitHub-family cursors resume from any working directory; local-tool cursors stay tied to their workspace |

## Next: fix round 5, in this order

1. **Blockers (correctness gate):** R3-K1, R3-G1, R3-K2.
2. **Medium:** R3-L1, R3-L2, R3-H1, R3-C1, R3-C2, R3-GR1, R3-G2, R3-K3, R3-P1, R3-P2.
3. **The rest** of the round-3 list, then the deferred items below that have a decided design.
4. Re-rate with the same 8 lanes ([independent raters](packages/octocode-benchmark/briefs/independent-raters.md)) and
   record round 4 in `packages/octocode-benchmark/results/rounds/`.

How the last round ran (reuse it): one lane per tool group, one owner per file, shared files only by grant from the
coordinator, compile-complete edits (`node packages/octocode-native/scripts/build-slot.cjs -- cargo check -p <crate>
--all-targets --offline`, with every dependent of a shared crate), focused tests through
`node skills-dev/octocode-dev/scripts/dev.mjs test:dev --crate <crate> -- <filter>` (the runtime crate is `native`),
agent text only through one core lane that runs `contracts:regen`, and the coordinator alone runs `build:dev` and the
gates (`verify`, `lint:rust`, `docs:rust`, `skills:check`, `test:ci`, `check-outputs`, core `yarn lint` and `yarn test`).
Two lessons: every new `next.*` key must be declared in the tool's output contract (assert it with `assert_row_contract`),
and run `verify` (the cross-cutting guard tests) before calling a lane done.

## Open findings: rating round 3 (48)

Exact inputs and evidence: [FINDINGS.md](packages/octocode-benchmark/results/rounds/20.2.0-2026-11-20-ratings-round-3/FINDINGS.md).
Severity **H** high, **M** medium, **L** low. "Gate" marks a finding that caps the tool at 4.

| # | Tool | Problem | Sev |
|---|---|---|---|
| R3-K1 | lspSearch | TS references silently drop uses through `await import('../dist/…')` (`isUuid` 7 of 14). Gate | H |
| R3-G1 | graphQuery | `impact depth:1` omits path-call callers; depth-1 count depends on the depth asked (24 vs 42; rg 41). Gate | H |
| R3-K2 | astSearch | Key-block redaction before parsing hides a real match (`key_blocks.rs` `.expect` 2 of 3), no warning. Gate | M |
| R3-L1 | localFetch | `matchString` matches redaction placeholders (2 hits where rg has 0) | M |
| R3-L2 | localSearch, localFetch | Hits inside masked spans drop silently from rows with other hits; every empty row claims masking even when none happened | M |
| R3-H1 | ghGetHistoryItem | Withheld patches get a read for the first file only; the rest are marked unreachable | M |
| R3-C1 | clasify | A 170 KB source with `maxRequests:1` spends a request, then dead-ends with a circular hint and no `next.confirm` | M |
| R3-C2 | clasify | A GitHub-file citation leaves out the deciding line (ripgrep `Cargo.toml`: cites 12–28, value at 52) | M |
| R3-GR1 | ghSearchCode | A `ref` search misses files that match only at that ref; the warning implies otherwise; no lead | M |
| R3-G2 | graphQuery | `since:"HEAD"` seeds a whole package from a one-line `package.json` scripts edit; seeded rows look like edits | M |
| R3-K3 | lspSearch | `callers` on an enum/trait: 27 s, empty, wrong diagnosis; should lead to `references`/`implementation` | M |
| R3-P1 | securityCheck | A package removed from its registry is never sent to OSV (OSV lists malware for it); reads as a typo | M |
| R3-P2 | securityCheck | A yanked RubyGems version (`rest-client` 1.6.13) isn't evaluated though OSV's ranges cover it | M |
| R3-K4 | lspSearch | Python anchor picks an occurrence inside a string literal | M-L |
| R3-K5 | lspSearch | Python with no project marker scans the whole monorepo (49 s cold), no hint to narrow | L-M |
| R3-G3 | graphBuild | `#[path]` on an inline module never links the nested module; false tier-90 dead code | L-M |
| R3-GR2 | ghGetFileContent | Ruby/PHP/Kotlin `block` reads from the hit to the end of the file (up to 2,000 lines) | L-M |
| R3-GR3 | GitHub tools | A tag spelled with or without `v` gets no repair; leads read the default branch | L-M |
| R3-H2 | GitHub history | "Who merged" can't be answered: no `mergedBy` | L-M |
| R3-P3 | artifactSearch | A Go package path inside a module loses owner/repo; the warning contradicts the given ref | L-M |
| R3-F1 | instructions | Name a ghStructure operation `tags` that doesn't exist (`refs`), which is refused | L-M |
| R3-K6 | astSearch | String quote style silently decides a match | L |
| R3-K7 | astSearch | `isPartial` beside `hasMore` with no reason on 18/20 countMatches pages; a near-empty page; `skippedGrammars` repeats; `inferredLanguage` undocumented | L |
| R3-G4 | graphQuery | `impact` on a directory returns `notFound` | L |
| R3-G5 | graphQuery | `mentionedIn` names a line without the path | L |
| R3-G6 | graphQuery | `testsToRun` rows without `functions` are unexplained | L |
| R3-L3 | localFetch | `view:"symbols"` skips the body of an item-level macro call | L |
| R3-L4 | localSearch | Empty under `testCode:"exclude"` doesn't offer `include` | L |
| R3-L5 | local tools | Undocumented omissions (`matchedLineCount`, match-only `value`); `testCodeExcluded` backwards under `only`; generic glob hint on sparse clones; `P1D` example | L |
| R3-C3 | clasify | `responseChars` ignored on answer rows | L |
| R3-C4 | clasify | Nested-field error examples show the wrong form; no name suggestions | L |
| R3-C5 | clasify | Quiet sections hidden on page 1; no top-level `next.read` for a resource with a yes | L |
| R3-C6 | clasify | Price text "at most 19" vs ceiling 22; `inputTokensAtMost` sometimes missing | L |
| R3-GR4 | GitHub tools | URL or `owner/repo` forms not repaired; examples name nonexistent repos | L |
| R3-GR5 | ghStructure | Empty refs glob not marked empty, no lead | L |
| R3-GR6 | output reference | `returnedLines`, `defaultBranch` undocumented; byte-paged read switches unit on its last page; jargon | L |
| R3-H3 | ghGetHistoryItem | Commit patches view drops `committer` | L |
| R3-H4 | GitHub history | Invalid regex: generic "Check the query fields." error | L |
| R3-H5 | ghSearchHistory | Keyword `author:x` not pointed to `filters.author` | L |
| R3-H6 | ghGetHistoryItem | Compare default view lists no commits | L |
| R3-H7 | ghCloneRepo | Description lacks "a few paths: ghStructure materialize" | L |
| R3-P4 | artifactSearch | Go vanity modules `registryRef` though the tag matches | L |
| R3-P5 | securityCheck | Distro packages (Debian, Alpine, Ubuntu) never get `upgradeTo` | L |
| R3-P6 | securityCheck | A version with surrounding spaces is called a range | L |
| R3-P7 | packages | No `oversized` at small `responseChars`; missing leads (`reqeusts`, `@Angular/core`, bare Maven names); `golang/protobuf` deprecation; long `mainGoal` rejects the call | L |
| R3-F2 | instructions | No route for "the first release that contains a change" | L |
| R3-F3 | localSearch | Placeholder-empty rows also get a case-insensitive rerun hint | L |
| R3-F4 | ghStructure | One cold tree of a 9-entry folder took 5.4 s | L |

## Round-2 fixes that are only partly done (from round 3's regression checks)

| Item | What is still left |
|---|---|
| L1 masking | Fixed in localSearch; localFetch still matches placeholders (R3-L1); masked-hit disclosure (R3-L2) |
| K2 TS references through `dist/` | Static imports fixed; dynamic imports drop uses (R3-K1) |
| K6 astSearch page shape | `isPartial` beside `hasMore` on most countMatches pages; one near-empty page at a loaded-window edge (R3-K7) |
| C3 oversized clasify source | 1.7 MB is priced for free; a 170 KB source (inside the long window) dead-ends (R3-C1) |
| P3 securityCheck details | 586K → 346K chars, but 3 records per advisory remain (one per source) and each part repeats answer, packageName, version and cautions (6.6K over 29 parts) |
| GR8 huge-repo walks | Complete now; slow (deferred GR8) |
| GR2 Ruby/PHP/Kotlin `block` | Gives a read to the end of the file instead of the block (R3-GR2) |
| H6 ghCloneRepo text | Behavior fixed; description still lacks the materialize route (R3-H7) |

## Smaller observations from rating round 3 (not in FINDINGS.md)

| Tool | Observation |
|---|---|
| securityCheck | With only GitHub failing, its source shows `status:"partial"` although nothing was read, and the row has no `next.retry` |
| securityCheck | A transport failure suggests `octocode auth login` "for a higher quota", which doesn't apply |
| securityCheck | Summary rows carry pinned-dependency checks in `related` with no explanation (tensorflow → `gast 0.3.3`) |
| artifactSearch | Maven keywords `["jackson","yaml"]` miss `jackson-dataformat-yaml` (Central's own search returns 3 rows): no warning that the provider limits it |
| ghSearchRepo | The capped-search split bisects by date, not by count (tokio: 177 and 7,958), so several split calls are needed |
| several tools | A `mainGoal` over 500 chars rejects the row instead of being trimmed (by design per L6; raised again by 2 raters) |
| ghStructure | A 5-query batch took 14.9 s; `materialize` of 3 files took 8.1 s; one root listing 6.4 s cold |
| GitHub history | Two unexplained latency spikes (8.9 s, 5.4 s) with no `clientWaits` shown |
| lspSearch | Depth-2 callers carry `via` but no `level`; `category` is undocumented; Rust callees say "1 hierarchy items … omitted" without naming it; `outOfReach` warnings are verbose |
| lspSearch | Cold times: Rust fixture 12.6 s, first call on a hot symbol 20.7 s, a cold `callers` in tool-local 11.8 s |
| astSearch | `skippedGrammars` mixes real grammars with toml, markdown and json; an unknown kind (`function_decl`) gets no nearest-kind suggestion |
| graphQuery | Call-reached `impact` rows say `via:"contains"`; a row's `risk` changes with `depth`; `argumentCoverage` (~600 chars) still repeats on walk, unresolved and witnesses answers |
| graphQuery | `impact since:"HEAD"` walk: 588 KB over 72 pages in 30.8 s; an incoming depth-2 walk on a hub: 783 rows, 17 pages, 28 s |
| graphBuild | Whole workspace 94.5 s cold, 77.8 s warm (round 2: 61.8 s, lower load) |
| localSearch | A walk over a 700k-entry scope ended in an honest `staleSnapshot`, but the refusal itself took 28.8 s |

## Not verified in round 3

- clasify `next.nextCandidatePage` and `next.unfilteredUnknown` (need more than 131,072 chars of hits: several billed
  requests); the images / `box` path.
- securityCheck `next.retry` across a configuration change (the cursor is tied to the configuration).
- ghCloneRepo with a withheld credential file in a directory-only scope (N2).
- The MCP surface on this binary (last checked on the round-2 build); skills; the benchmark comparison; security as a
  full area; Windows and Linux.

## Deferred from fix round 4 (need a design or contract change)

| # | Tool | What is left | Why deferred |
|---|---|---|---|
| C5a | clasify `sections` | Section size is fixed in tools-shared `source_sections` (`PAGE_SHARE`) | Shared design |
| C5b | search tools | No cost cap or estimate for inline `classification` on the 7 search tools | Contract change on each tool |
| G6 | graphQuery | Calls inside user macro arguments unlinked; external calls uncounted | Macro arguments need per-macro admission; counts need a snapshot format change |
| G8 | graphQuery | Identical `witnesses` rows (one per imported name) | Facts schema needs a span or name field |
| G9 | graphBuild | Incremental rebuild relinks everything (~87% of cold) | Needs an incremental-linking design |
| GR6 | ghSearchCode | Owner-wide rows repeat `commitSha` | Needs a core field; no surface headroom |
| GR8 | ghStructure | Huge-repo walks are slow (chromium 78 s, llvm 22 s) | Correct now; speed is its own work |
| D4 | ghStructure | No default-branch SHA in `refs` | Changes the field format or needs a new field |
| L2 | localSearch | Sorted content search over huge result sets prices every row first (~10 s) | Needs a two-phase scan |
| L6a | structureSearch | `skippedPaths` repeats per batch row | Belongs to the runtime-base `shared.data` hoist |
| P5b | artifactSearch | No GitHub lead for Go vanity modules (`golang.org/x/text`) | New outbound host class; needs an SSRF guard |
| P6f | artifactSearch | `@types/nodee` lead | Owner decision (O8) |
| P7 | securityCheck | Huge packages take 10–19 s (provider round trips) | No general fix without a cap |
| K5 | lspSearch | Kind names differ (`interface` vs `trait`, `typeParameter`) | Needs one vocabulary decision |
| D2 | lspSearch | Every continuation page re-proves staleness over the whole scope (1.0–1.5 s per page) | Proposal: prove later pages against the files they cite; design review |
| D3 | lspSearch | Noisy TypeScript `documentSymbols` | Core must redefine the outline, plus an input field for locals |

## Follow-ups found in fix round 4

| # | Tool | Problem |
|---|---|---|
| N1 | securityCheck | When both GitHub reads fail, the source's `why` states the transport reason twice |
| N2 | ghCloneRepo | A withheld credential file in a directory-only scope drops the root's files, while the description says they're kept |
| N3 | localSearch | Outside-scope rows of a sparse clone also carry the generic path-listing hint |
| N4 | localSearch | With `invertMatch`, a line whose only match is a placeholder's spelling counts as matching |
| N5 | graphQuery | On a fresh snapshot, `node` keeps only the lspSearch lead; its graph leads are dropped by the guidance stage |
| N6 | cleanup | Test-only methods kept after their production callers moved: `Scope::in_reach` (tool-lsp), `Registry::new` (runtime cursors) |
| N7 | graphQuery | A `graph` id without `path` is refused (`correctInput`, with the path in the message) |

## Test and tooling issues

| # | Item |
|---|---|
| T1 | `engine` `lsp::config::tests::catalog::a_rustup_proxy_resolves_only_when_the_toolchain_rustup_selects_ships_the_component` fails inside a full `verify` under heavy load and passes alone; unchanged since 2026-11-19. Likely a time budget on the fake rustup. |
| T2 | `packages/octocode-benchmark/tool-checks/` (the `run.mjs` MCP check suite) was removed by another session's reorganization; the default-surface MCP check now has no script. |
| T3 | `packages/octocode-native/scripts/test-binaries.cjs` and `tests/test-binaries.test.cjs` are not Prettier-formatted (native lint doesn't enforce it). |
| T4 | Two clone checkouts from 2026-11-09 under `~/.octocode/clones/octocat/hello-world` predate the shared store; they age out of the cache. |
| T5 | A round-3 rater's `materialize` left 3 flask files in the tool's cache under `~/.octocode/tmp/materialize/v5/pallets/flask/49943b5…`; the cache manages them. |

## Rejected (by design or not reproduced): don't reopen without new evidence

| # | Finding | Reason |
|---|---|---|
| C3 (part) | Refuse an oversized source before its first request | The window counts tokens; bytes only bound them, so a fit can't be predicted |
| C7 | Tighter token estimate | `inputTokensAtMost` must stay a hard bound |
| C8 (part) | 33-line cited part | Parts are cut at block edges by the cost balance |
| D6 | One ecosystem spelling across tools | One per tool; the other is repaired; every cross-tool lead runs as sent (tested) |
| F2 | `skippedPaths` drill-down on a page with hits | Pages with hits don't re-offer skipped paths; empty pages offer `includeSkipped` |
| F5 | Page 1 vs page 2 identity fields | Page 2's request is a cursor alone, so the row restates owner, repo, path |
| F7 | ghSearchRepo `get` slow | Not reproduced (0.57 s cold) |
| GR4 (part) | Success-row leads merged in a batch | The merged call names each repo; executable and traceable |
| H1 (part) | `related[]` local call after every clone | The description routes to `location.path`; the guidance stage drops a lead that restates it |
| K5 (part) | `view:"files"` shapes; impl bodies in content view | Documented; by design |
| L3 | Accept "yesterday" / "P1D" | Would be aliases; the error shows `-P1D`, `-PT6H`, `-P2D` |
| L6 (rest) | `total` counts long-line windows; 500-char `mainGoal`; nested `.git` | Documented; bound stated; `.git` is metadata |
| P3 (part) | Identity fields repeat on every row part | Row parts repeat identity and disclosure by design |
| D5 | ghGetFileContent `symbols` rows | Already works |

## Constraints for whoever continues

- AGENTS.md rules apply: no `git commit` or `git stash` (a person owns commits), Rust only through the wrappers and
  build slots, one owner runs full builds, stop processes by PID only, edit core text only in octocode-core and regen.
- Never read, print or log `~/.octocode/.env` or any key. clasify bills the configured account: give raters a fixed
  budget and require an explicit `maxRequests` on every billed call.
- No posting to GitHub (issues, comments, PRs) without the owner's go.
- Another session is reorganizing `packages/octocode-benchmark`; coordinate before editing its files.
