# Leftovers

Open findings for the 20.2.0 line (build `20.2.0+69024e9f`), newest round first. Each row says what is wrong,
where, how bad, and who owns the fix. Remove a row when its fix is verified.

Severity: **high** wrong evidence, lost evidence or money spent without consent; **med** wrong lead, slow, or
costly; **low** wording, size or polish. Owner: **core** (agent text and schemas in octocode-core),
**native** (Rust behavior), **release** (owner steps).

## Release blockers

| # | Item | Owner | Status |
|---|---|---|---|
| R1 | Published core 21.2.0 differs from the local core (191 dist files), but `contract/provenance.json` says the contract came from 21.2.0. Bump core, publish it first, repin it in `packages/octocode-config/package.json`, run `contracts:regen`, rebuild. | release | open |
| R2 | Nothing is committed: about 346 changed files here and 212 in octocode-mcp-host. They include another session's 5 files (`tool-lsp/src/lsp_search/prewarm.rs`, `prewarm/tests.rs`, `cli/tests/mcp_server/prewarm.rs`, `runtime/src/runtime/domain_dispatch.rs`, `runtime/tests/runtime_clasify/sources.rs`); decide whether they ship. | release | open |
| R3 | Only macOS arm64 was built and tested. Issues #477–#479 were reported on Windows; check the all-platform `build:publish` there. | release | open |
| R4 | npm `latest` tags differ: `octocode-mcp` 19.1.0, `octocode` 20.1.0. Confirm this is intended. | release | open |
| R5 | GitHub issues #473 and #477–#482 are fixed in this build but still open. Close them with a comment each, now or after publishing. | release | open |
| R6 | Running MCP sessions started before the rebuild serve the old build; restart them. | release | open |

## Rating round 2 (2026-11-20, independent raters)

Average overall **8.0/10** (round 1: 7.8), correct **9.4/10**. 58 findings below, 6 of them high.

| Tool | Round 1 → 2 | Correct | Tool | Round 1 → 2 | Correct |
|---|---|---|---|---|---|
| localFetch | 9 → 9 | 10 | ghSearchCode | 6 → 8 | 10 |
| ghSearchHistory | 8 → 9 | 10 | ghGetFileContent | 9 → 8 | 10 |
| astSearch | 8 → 9 | 9 | ghGetHistoryItem | 8 → 8 | 9 |
| structureSearch | 9 → 8.5 | 10 | artifactSearch | 8 → 8 | 9 |
| localSearch | 8 → 8.5 | 9 | securityCheck | 8 → 8 | 9 |
| ghSearchRepo | 8 → 8 | 10 | ghStructure | 8 → 7 | 9 |
| ghCloneRepo | 7 → 8 | 10 | lspSearch | 7 → 7 | 8 |
| graphBuild | 7 → 8 | 9 | clasify | 7 → 7 | 9 |
| | | | graphQuery | 7 → 7 | 9 |

### clasify (rated 7/10 overall, 9/10 correct; round 1: 7)
Accuracy: 56 of 57 labelled judgments right (precision 1.00, recall 0.95); 6 of 6 citations hold the deciding text;
an identical repeat gave the same verdicts. Untested: inline `classification` on search tools and
`next.nextCandidatePage` / `unfilteredUnknown` / `unfiltered` (the billed run was blocked by permissions).

| # | Finding | Evidence | Sev | Owner | Status |
|---|---|---|---|---|---|
| C1 | No default request gate: a multi-file source plan without `maxRequests` runs at once instead of being refused with `next.confirm` (or a small default cap). One call sent 19 billed requests (requestsCeiling 30) over 49 hits in about 16 files. | `{"queries":[{"operation":"evaluate","source":{"tool":"localSearch","query":{"path":".","pattern":"retry"}},"questions":{"q":{"type":"noul","instructions":"The text mentions a restart."}}}]}` → `"usage":{"calls":19,"requestsCeiling":30}` | high | native + core | open |
| C2 | localSearch reads the unknown field `pattern` as `matchString` with only a warning, so a mistyped call still runs, and here bills. | same call as C1; warning "read `pattern` as `matchString: ["retry"]`" | med | native | open |
| C3 | A source larger than every provider window is sent anyway and refused by the provider, although its size was known before sending. `maxRequests` bounds requests, not tokens; a 1-request plan has no free estimate. | `{"queries":[{"operation":"evaluate","maxRequests":1,"source":{"tool":"localFetch","query":{"path":"packages/octocode-config/contract/tool-contract.json"}},"questions":{"q":{"type":"noul","instructions":"The unit names a retry policy."}}}]}` → `classificationStateTooLarge`, `calls:1`, `inputTokens:0`; billing of a refused request undocumented | med | native | open |
| C4 | `maxRequests:1` still refuses a 1-file search: the ceiling counts 3 retries (ceiling 5), and `next.confirm` proposes 5. Content and URL rows with `maxRequests:1` now run. | localSearch source on `packages/octocode-native/crates/tool-packages/src/artifact/http/system.rs`, `matchString ["retry"]`, `maxRequests:1`; with `maxRequests:2` it runs at ceiling 2, calls 1 | med | native | open |
| C5 | `sections` and inline `classification` take no cost cap or estimate (`maxRequests` refused: "requires operation:\"evaluate\""). An 18.7 KB doc that fits one window was split into 3 billed fragments of about 7.8 KB, cut mid-section at line 194/195. | sections over `docs/OCTOCODE_CLASIFY.md` → calls 3, ceiling 6, 7,237 input tokens; refusal confirmed live | med | core + native | open (extends D7) |
| C6 | `perHit:true` on a localFetch source is accepted silently and billed, instead of refused. | perHit + localFetch source probe → 1 billed request | low | native | open |
| C7 | The token bound holds but runs 3–4× high; `remainingInputTokensEstimate` never appeared. | inputTokensAtMost vs actual: 5115 vs 1287; 13100 vs 3135; 12916 vs 4015 | low | native | open |
| C8 | Docs contradict behavior: `docs/OCTOCODE_CLASIFY.md:139` says a URL capture names no `lines`, but URL rows now cite capture lines (semver.org → 56–63). A gh row cited 33 lines for a 2-line sentence. | doc line confirmed live | low | docs + native | open |

Round 1 issues: URL citation **fixed**; token bound **fixed** (holds, but loose: C7); `maxRequests:1` **partly fixed** (C4);
short `perItem` items one request each **by design**.

### End-to-end flows (local 8, local 8, public 9, guidance 8; round 1: local 7, public 8)
All 3 research questions answered correctly and verified; routing was right in all 3 flows. Round 1 issues: the
`walk_builder` lead **fixed** (no lead points there; the walk.rs doc names `EntryRule::skips`); CONTRIBUTING.md
offered for an internals question **partly fixed** (ghSearchRepo `get` offers nothing; a root ghStructure tree
still offers README.md, 9.7K, as its only `related` call, whatever mainGoal says).

| # | Tool | Finding | Evidence | Sev | Owner | Status |
|---|---|---|---|---|---|---|
| F1 | all tools (`--help`) | The responseChars sentence ("page size for reads and remote pages; local walks page at the automatic window") reads backwards: localSearch honors responseChars for the envelope; only the walk window is fixed. | `{"responseChars":4000,"queries":[{"path":"packages/octocode-native/crates/runtime-base/src/response","matchString":["budget"]},{"path":"…/response","matchString":["snapshot"],"view":"files"}]}` → pages ≤ 3,949 chars, 24 pages | med | core | open |
| F2 | localSearch | `skippedPaths` gives counts with no executable drill-down to list them. | `{"queries":[{"path":".","matchString":["SENSITIVE_DIRECTORY_NAMES"],"view":"files"}]}` → `{"cause":"ignored","count":159,"path":"."}`, no next/related; git lists 163 top-level ignored entries (unreconciled) | low-med | native + core | open |
| F3 | ghGetHistoryItem | `status:"empty"` on a compare row is undocumented; the output reference lists only "absent" and "outOfRange". | compare tokio-rs/tokio `b2636752…` → `tokio-1.54.0`, include `tokio/src/runtime/time/wheel/mod.rs` → `status:"empty"`, `files:[]`, `aheadBy:54` | low-med | core | open |
| F4 | localSearch | `testCode` is not shown in `--help` or its examples; exclude globs miss repos whose test dirs are named `tool_tests/`, `policy_tests/` (5 of 11 files were tests). `"testCode":"exclude"` gives 6 clean files. | localSearch `--help` | low | core | open |
| F5 | ghGetFileContent | In a paged multi-range read, page 1 lacks owner/repo/path/commitSha while page 2 has them; `matchString` returns bare matching lines with no context. | `matchString:["Timer"]` on `tokio/src/runtime/mod.rs` → 20 bare lines separated by `--` | low | native | open |
| F6 | lspSearch | `callers` attaches noisy `related` localSearch regex calls on test and bench files (partialReasons `buildScriptsOff`) to a correct, complete answer. | callers of `admit` at `crates/runtime-base/src/policy/discovery/admission.rs:218` → 8/8 sites (matches rg) + 5 related | low | native | open |
| F7 | ghSearchRepo | `get` took 5.3 s for one repo and offered no next call. | one `get` call in the tokio flow | low | native | open |

### Graph (graphBuild 8/10, graphQuery 7/10; round 1: 7 and 7; both 9/10 correct)
Linked edges are accurate (e.g. `push_warnings` caller files 56/56 against rg; `use super::*` sites 8/8).
Round 1 issues: build failing on mid-run edits **fixed** (26 edits during a 19.7 s build: succeeded, warned, `stale`
listed exactly the 3 files); `use super::*` **fixed**; `impact` size **not fixed, now worse** (G1); `callees`
dropping unlinked calls **partly fixed** (G6).

| # | Tool | Finding | Evidence | Sev | Owner | Status |
|---|---|---|---|---|---|---|
| G1 | graphQuery `impact` | Summary lists are unbounded and arrive before the rows: 433,162 chars over 23 pages for 51 rows; page 1 holds 1 row (150 chars) beside a 10.7 KB summary. In a 5-query batch the other 4 answers wait until page 14. Expected: counts first, lists behind `limit`/`next.*`, grouped by file, rows first. | `{"queries":[{"operation":"impact","since":"HEAD"}]}` at the workspace root → `summary.testFunctions` 2,760 ids (341 KB), `testsToRun` 714 (57 KB), `affectedEntrypoints` 139; `impact` target `packages/octocode-native/crates/engine/src/num.rs` → 280,611 chars, 15 pages | high | native + core | open |
| G2 | graphBuild / graphQuery (Rust) | A file `#[path]`-included by a second crate loses its home-crate links: `deps` 0 rows, `use crate::num::saturating_u32` reported `unresolved-internal` (`issues`: unresolved-import), 15 calls unresolved, left out of `callers`. | `engine/src/text/utf8_offsets.rs` (module of `engine/src/text.rs`, also `#[path]` in `fuzz/fuzz_targets/utf8_offsets.rs:15`); `engine/src/lib.rs:33 pub mod num;` | med | native | open |
| G3 | graphQuery `walk` | `direction:"both"` rows carry no direction, so callers look like callees; the `rows[].from` doc ("the site starts at this node") is wrong for incoming rows. | `{"operation":"walk","target":"ts/src/app.ts#run","direction":"both","depth":1,"edges":["calls"]}` → `index.ts#main` shaped like a callee; `main` calls `run` at index.ts:6 | med | native + core | open |
| G4 | graphQuery | No handoff to `lspSearch` or `localFetch`, though the description says to confirm rows with lspSearch; none of ~48 related/next calls leads there. | ~48 collected related/next calls | med-low | native or core | open |
| G5 | graphQuery `issues` | `export-only-local` gets tier 90 ("no graph or code reference") although tests use the export through a namespace re-export of the built output. Expected: tier 60 with `mentionedIn`. | agents-communication `metadataPath` (used `tests/paths-validation-catalog.test.ts:112`), `COMPACT` (used `tests/wire.test.ts:25`), via `export * as` in `src/entries/runtime.ts` | low-med | native | open |
| G6 | graphQuery `callees` | Calls inside user macro arguments are neither linked nor counted; external calls (`chalk.red`, `leftPad`, `[].map`) leave no count. | `common!(q, page, saturating_u32(page.get()))` at tool-local `local_search_query.rs:70`; fixture `wrap!(alpha())` → only `wrap`, no warning | low | native | open |
| G7 | graphQuery | `"since":"nosuchref"` returns errorClass `unservable`; should be `correctInput`. | `impact` with `since:"nosuchref"` | low | native | open |
| G8 | graphQuery | Output noise: a snapshot-wide `argumentCoverage` block (~2.5 KB on the workspace) repeats on `callees`, `unresolved`, `witnesses`; `witnesses` repeats identical rows (5 for `node:fs` at line 5); scoped snapshots mix workspace-relative file ids with scope-relative symbol ids (see D1); `node` says "with its edges" but returns counts; "1 call sites". | per rater | low | native + core | open |
| G9 | graphBuild | Incremental rebuild saves only ~4%: a 1,411-file copy took 19.7 s cold, 19.0 s with 3 files changed (1,358 reused, 53 parsed). Whole workspace 61.8 s cold; unchanged reuse 1.25 s. Only 1 query per call, and neither description nor `--help` says so. The linkage ratio (0.43) counts std calls such as `seen.insert` as eligible. | timings per rater | low-med | native + core | open |

### GitHub history (ghSearchHistory 9, ghGetHistoryItem 8, ghCloneRepo 8; round 1: 8, 8, 7)
Every count and walk matched `gh api` (e.g. TypeScript#51387: 656/656 file rows; tokio#6001: 28 comments, 110
review comments, 72 reviews). Round 1 issues: `commentsCount` **fixed** (conversation, review and total are separate
fields); sparse checkout not marked partial **partly fixed** (H1); one bad path failing the clone **fixed**.

| # | Tool | Finding | Evidence | Sev | Owner | Status |
|---|---|---|---|---|---|---|
| H1 | ghCloneRepo → local tools | A sparse scope is invisible to local tools: a path outside it gives "Path does not exist", and a root search says nothing about the scope. Expected: a coverage warning on the clone row, and local tools saying "outside this checkout's sparse paths" with a re-clone call. No `related[]` to local tools after a clone. | `{"queries":[{"owner":"sharkdp","repo":"hexyl","paths":["src"]}]}` then localSearch `<location.path>/tests` `["squeeze"]` → notFound; `tests/integration_tests.rs` at 6ecc29b has 7 hits | med | native | open |
| H2 | ghGetHistoryItem | False warning on a commit whose message names a PR: "This commit's message names no pull request". | `{"queries":[{"operation":"commit","owner":"BurntSushi","repo":"ripgrep","ref":"1a50324013708e3c73bfa986d273af2f8e8e3360"}]}`; message says "submitted as part of PR #2483"; merged by #2610 | low-med | native or core | open |
| H3 | ghGetHistoryItem | `patchPagination {offset,length,total,hasMore}` on split patch rows is missing from `schema ghGetHistoryItem --view output`. | 0 matches in the output view | low | core | open |
| H4 | all tools (cursors) | A cursor is tied to the working directory: a ghSearchHistory cursor fails from another directory with `responseSnapshotExpired`/`restart`, even for a GitHub-only search. | cursor reused from a second cwd | low | native | open |
| H5 | ghSearchHistory | `{"queries":[{"operation":"pullRequest"}]}` is accepted and returns the 50 newest PRs across all of GitHub with no warning. Expected: require keywords, owner or a filter without a repo, or warn. | as given | low | core | open |
| H6 | ghCloneRepo | `paths:["src","does/not/exist"]` makes a new sparse checkout although one with the same effective scope `["src"]` exists; `historyDepth:100000` doesn't say it got the full history (490 commits). The description doesn't say when not to clone (one file → ghGetFileContent; a few paths → ghStructure `materialize:true`). | as given | low | native + core | open |
| H7 | ghGetHistoryItem | The PR-not-found hint says "for a path, list its parent directory", which does not apply to a PR number. | PR-not-found response | low | core | open |

### Packages (artifactSearch 8, securityCheck 8; round 1: 8 and 8; both 9/10 correct)
Refs right for 20 of 21 packages across 8 ecosystems; securityCheck counts and `upgradeTo` right for 12 packages
(OSV alias groups exact). Round 1 issues: `expresss` lookalike **fixed**; lodash 4.99.0 `critical` **fixed**
(`versionEvaluated:false`, no severity); CVE gave only its alias **partly fixed** (P6).

| # | Tool | Finding | Evidence | Sev | Owner | Status |
|---|---|---|---|---|---|---|
| P1 | securityCheck | When the GitHub advisory read fails during an OSV failure, the `answer` says "GitHub's advisory database matches none to it" although `sources[github]` is `partial / transport / "Advisories not read"`. Expected: "was not read for it". | `{"queries":[{"operation":"package","ecosystem":"pypi","packageName":"flask","version":"0.12"}]}` during a network failure; `gh api '/advisories?ecosystem=pip&affects=flask@0.12'` → 4 GHSAs; `next.retry` returned the same 4. Code: `tool-packages/src/security_check/package.rs:280` passes `github.is_some()` as `github_read` (text at :381) | high | native | open (code path confirmed) |
| P2 | artifactSearch | A wrong-case npm name suggests a security holding package: `React` → "npm has eact … one edit away" with a resolve call for `eact` (`0.0.1-security`). Expected: suggest `react` (npm names are lowercase). | `{"queries":[{"operation":"resolve","ecosystem":"npm","packageName":"React"}]}` | med | native | open |
| P3 | securityCheck | The details view repeats identical advisory text per source (OSV, GitHub global, GitHub repository) and repeats `answer`, `packageName`, `version`, `cautions` on every part: axios 0.21.0 → 52 pages, ~593K chars for 25 advisories (`details` alone 494K). tensorflow 2.0.0: 109 parts of ~40K chars, 69 near-identical "databases disagree" warnings (15.9K chars). | `{"queries":[{"operation":"package","ecosystem":"npm","packageName":"axios","version":"0.21.0","view":"details"}],"responseChars":15000}` | med | native + core | open |
| P4 | artifactSearch | The documented "tag at another commit" warning is missing: react 18.2.0 reports `verification: registryRef` at be229c5 while tag v18.2.0 points to 9e3b772. | `{"queries":[{"operation":"resolve","ecosystem":"npm","packageName":"react","version":"18.2.0"}]}` (react-dom 18.2.0 the same) | low-med | native | open |
| P5 | artifactSearch | rails 7.1.3 returns `ref: "v7.1.3"` (a tag name) with `registryRef` although the tag exists (commit 36c1591); `golang.org/x/text` gets no lead to its GitHub mirror github.com/golang/text. | `{"queries":[{"operation":"resolve","ecosystem":"rubygems","packageName":"rails","version":"7.1.3"}]}`; `{"queries":[{"operation":"resolve","ecosystem":"go","packageName":"golang.org/x/text"}]}` | low | native | open |
| P6 | securityCheck | A CVE answer names packages and fixes now, but has no follow-up call, and its warning points to a GHSA alias row the summary view doesn't show; CVE-2022-24999 summary drops the express GIT range (fixed 4.17.3). Garbled caution: "names substack/minimist, which GitHub does not show this credential" (`trust.rs:329`). Hint example shows npm `zod` for a Maven name error; `@types/nodee`'s only lead is a search for `@types`. | `{"queries":[{"operation":"advisory","id":"CVE-2021-23337"}]}`; `pkg:npm/minimist@1.2.5` | low | native + core | open |
| P7 | securityCheck | Slow under load: a batch of 5 takes 6–12 s; tensorflow 2.0.0 took 37.9 s cold; one batch took 68 s when OSV, the registry and GitHub all timed out. | timings per rater | low | native | open |

### Local (structureSearch 8.5, localSearch 8.5, localFetch 9; round 1: 9, 8, 9; correct 10 / 9 / 10)
Every page walk rebuilt the evidence exactly once (e.g. 14 pages = 5,430 files, the same set as `rg --files`; a
1,040,682-byte one-line JSON byte-identical under `cmp`). Round 1 issues: localFetch `pub fn apply(` **partly fixed**
(L5); relative dates **not fixed** (L3); sorted search speed **partly fixed** (L2) and `total` counting windows
**documented, unchanged** (L6).

| # | Tool | Finding | Evidence | Sev | Owner | Status |
|---|---|---|---|---|---|---|
| L1 | localSearch | Secret masking gives a false empty row ("No match in 1 searched file") when the only match is in a masked span, and the mask placeholders themselves are matchable (phantom hits). Expected: the empty row says the file has masked spans; placeholders don't match. | `{"queries":[{"path":"packages/octocode-native/crates/runtime-base/src/providers/github/auth/credential_store","matchString":["synthetic"]}]}` → empty; rg finds `credential_store/tests.rs:8`. `matchString:["REDACTED"]` over `…/github/auth`, `view:"countLines"` → 10 lines in 6 files; source holds 2 lines in 1 file | med | native | open |
| L2 | localSearch | Sorted content search on huge result sets is far slower than the count view: 9.18 s vs 0.81 s (rg -c 0.18 s). Normal sorted searches take 0.37–0.45 s (round 1: 47 s). | `{"queries":[{"path":".","matchString":["e"],"sort":{"by":"matchCount","order":"desc"},"limit":3}]}` | low-med | native | open |
| L3 | structureSearch | Relative dates are still refused: "yesterday", "today", "2 days ago", "2026-11-19", "P1D", "-1d" all give `invalidInput`; only `-P1D` works. The error's example shows no `modifiedAt` value. | `{"queries":[{"operation":"files","path":"docs","modifiedAt":{"from":"yesterday"}}]}` | low-med | native + core | open |
| L4 | structureSearch | `sort {"by":"traversal","order":"desc"}` says "traversal is outside the allowed enum", though traversal asc works and the schema lists it. localSearch gets this right ("use asc"). | `{"queries":[{"operation":"files","path":"docs","sort":{"by":"traversal","order":"desc"}}]}` | low | native | open |
| L5 | localFetch | A declaration-prefix miss still costs an extra call: `"fn add_hints("` is empty when the source is `pub fn add_hints<I>(`; it now warns "5 lines match `add_hints`" and offers a wholeWord block call. Expected: return the single declaration directly. | `{"queries":[{"path":"packages/octocode-native/crates/runtime-base/src/response/result.rs","matchString":["fn add_hints("],"block":true}]}`; same for `"const parseEnv"` vs `export function parseEnv(` | low | native | open |
| L6 | localSearch | `pagination.total` counts long-line windows while its unit says "lines" (91 vs `matchedLineCount` 1 on a 1 MB one-line file). Smaller: a `mainGoal` over 500 chars rejects the whole call; the compact view split 450 + 1 lines across two pages; nested `.git` folders are skipped without appearing in `skippedPaths`. structureSearch `skippedPaths` takes 29% of a result and repeats per row in a batch. | `localFetch` in `packages/octocode-config/contract/cli-presentation.json` | low | core + native | open |

### GitHub read (ghSearchRepo 8, ghSearchCode 8, ghStructure 7, ghGetFileContent 8; round 1: 8, 6, 8, 9)
About 45 claims checked against `gh api` and raw reads; all right except ghStructure's `retry` (GR1). A 16,520-line file came back
exactly over 43 pages. Round 1 issues: widened search "every Rust repo" **fixed**; ghSearchCode per-file cap and total
**fixed** (31 files in ~18 KB, readHits rebuild each file once); ghStructure batch leads merged **partly fixed** (GR4);
"cache busy" leak **fixed**; CONTRIBUTING.md orientation **fixed**; ghGetFileContent empty-match picks **partly fixed** (GR4).

| # | Tool | Finding | Evidence | Sev | Owner | Status |
|---|---|---|---|---|---|---|
| GR1 | ghStructure | A tree walk stopped by its fixed 300-folder read budget returns `next.retry` with the same cursor and the same partial result: an endless loop, no `terminalLimit`. Expected: a `nextPage` that resumes from the unread folders, labelled a budget stop; no fixed cap. | `{"queries":[{"operation":"tree","owner":"torvalds","repo":"linux","include":["**/sched/fair.c"]}]}` → empty, `partialTreeFailures`, "1560 folder(s) were not read"; chromium `**/zz_not_a_real_file.cc` → 79 unread; `kernel/sched` listing shows `fair.c` | high | native | open |
| GR2 | ghGetFileContent | `block:true` returns only the matched line for Ruby, PHP, Kotlin and Markdown, with no next step; Ruby `view:"symbols"` gives `symbolsUnsupported`. The description doesn't list supported languages. Java, Go, Python, Rust, C are correct. | sinatra `lib/sinatra/base.rb` `"def route("`; rails `actionpack/lib/action_dispatch/routing/mapper.rb` `"def match("`; laravel `src/Illuminate/Support/Str.php` `"public static function slug("`; okhttp `OkHttpClient.kt` `"class Builder"`; ripgrep `GUIDE.md` `"### Configuration file"`; bubbletea `README.md` `"## Tutorial"` | med | native + core | open |
| GR3 | ghGetFileContent (batches) | A batch splits the page budget equally per query, so one block is cut at 68 of 122 lines with a `nextPage` though the whole response is 4.7 KB; alone it returns in one page. Expected: unused budget is shared. | 5-query batch with requests `src/requests/sessions.py` `"def resolve_redirects("` `block:true`; httpx `class Timeout` 68 of 85 lines | low-med | native | open |
| GR4 | ghGetFileContent, ghStructure (batches) | In a batch, `related[]` is hoisted to the top level, breaking per-row hints ("related[] in order: this file's declarations…" now starts with ghSearchCode; row 0 has neither); ghStructure success-row orientation leads merge into one call. | ghGetFileContent batch with two empty matches on cobra `command.go`; ghStructure tokio + rails | low-med | native | open |
| GR5 | ghSearchRepo, ghSearchCode | Capped searches (1000 reachable) disclose the cap but offer no split lead (stars, date range, per repo, per extension). | ghSearchRepo `"http client"` → 18,792 matches; ghSearchCode `TODO` `user:microsoft` → 5,164 | low-med | native | open |
| GR6 | ghSearchCode | Owner-wide path rows repeat `commitSha` (12.8 KB for 100 paths). | owner-wide path search | low | native | open |
| GR7 | ghStructure, ghSearchRepo | No repair of a `.git` suffix: `"repo":"flask.git"` / `"ripgrep.git"` → `notFound` with a lead searching the keyword `"ripgrep.git"`. | as given | low | native | open |
| GR8 | ghStructure | Huge-repo walks are slow: linux 40 s, chromium 59 s, llvm 25 s; a batch of 5 takes 5.9 s. | timings per rater | low | native | open |

### Code intelligence (astSearch 9, lspSearch 7; round 1: 8 and 7; correct 9 / 8)
astSearch counts equal rg once text-only hits are removed (e.g. 1,514 `.expect`); lspSearch identity is precise
(same-named methods excluded, anchor 5 lines off repaired). Round 1 issues: astSearch near-empty pages **partly
fixed** (K6); lspSearch `implementation` 29 of 51 **fixed** (51/51 from both anchors, now over-flagged: K3).

| # | Tool | Finding | Evidence | Sev | Owner | Status |
|---|---|---|---|---|---|---|
| K1 | lspSearch | Says `coverage.state:"complete"` with no partialReasons while missing uses in modules gated by `#[cfg(feature)]` on the `mod` declaration (only in-file `#[cfg]` is detected). | `{"queries":[{"operation":"references","path":"test-utils/fixtures/demo-repo/advanced-local/rust/src/lib.rs","symbolName":"target","line":1}]}` → 3 rows; `bright_feature.rs`, `dim_feature.rs`, `custom_cfg.rs` each call `super::target()`; `callers` the same | high | native | open |
| K2 | lspSearch (TS) | References silently drop uses that bind through the package's declaration output (`dist/types/*.d.ts`, outDir set in `tsconfig.types.json`); `definition` from a test lands in the `.d.ts` with no `next.sourceDefinition`. | `{"queries":[{"operation":"references","path":"packages/octocode-agents-communication/src/runtime/validation.ts","symbolName":"asObject","line":30}]}` → 45, expected 51 (missing `tests/communication-boundary.test.ts:13` ×2, `command-results.test.ts:23`, `records.test.ts:148,149`, `service-transports.test.ts:18`) | high/med | native | open |
| K3 | lspSearch | False or noisy partial flags: `cfgGatedFiles` for a file that only calls a same-named method; `implementation` 51/51 flagged partial with `textOnlyFiles:162` (`&dyn` uses) and `cfgGatedFiles:3` naming no file. | references/callers on `octocode_io::atomic::write_atomic` (`io/src/atomic.rs:75`) flag `runtime-base/src/private_file/windows/tests.rs`; implementation on `CancellationCheck` (`cancel.rs:39`) | med | native | open |
| K4 | lspSearch (Rust) | `supertypes` of a trait with std bounds is empty (`typeHierarchyExpansionFailed`), though labelled partial. Expected: Send and Sync. | `{"operation":"supertypes","path":"packages/octocode-native/crates/tools-shared/src/selection.rs","symbolName":"SelectionJudge","line":54}` (`pub trait SelectionJudge: Send + Sync`) | med | native | open |
| K5 | lspSearch | A hot symbol is slow and every page is re-paid: 78 s first call, then 2.4–2.9 s per cursor page (result exact: 826 files, 6,733 refs). Content-view `implementation` rows carry whole impl bodies (16.9 KB for 45 rows). Kind names differ: Rust traits are `interface` in documentSymbols but `trait` in supertypes; type aliases are `typeParameter`; `view:"files"` has two shapes. | `{"operation":"references","path":"packages/octocode-native/crates/test-support/src/json.rs","symbolName":"json_fixture","line":18,"view":"files"}` | low-med | native | open |
| K6 | astSearch | Page-shape leftovers: a split first row leaves a near-empty `rowPart 2/2` page (888 chars) and the warning says 20 pages for a 21-page walk; `skippedGrammars`/`partialReasons` repeat per page; `isPartial:true` beside `hasMore` (the contract says not both); `syntaxTree` of a big file gives 224,584 nodes (~600 pages) with no hint to pass `line`; a regex-only rule returns 30 nested enclosing nodes (92 KB repeated text) without a warning. | `{"operation":"match","path":"packages/octocode-native/crates","rule":{"kind":"function_item"},"view":"countMatches","sort":{"by":"matchCount","order":"desc"}}`; `syntaxTree` on `contract/tool_types.rs`; rule `{"regex":"load"}` | low | native + core | open |

## Deferred from fix round 3

| # | Tool | Finding | Why deferred | Owner |
|---|---|---|---|---|
| D1 | graphBuild, graphQuery | Graph ids mix two path roots; `id`, `path` and `symbolName` repeat on every row. | Changes every graph id; needs its own round | native |
| D2 | lspSearch | `staleSnapshot` checks every file in the workspace instead of the answer's scope. | Design change | native |
| D3 | lspSearch | TypeScript `documentSymbols` lists are noisy. | Design change | native |
| D4 | ghStructure | No default-branch SHA; the tree summary repeats; commitSha grouping. | Contract change | core + native |
| D5 | ghGetFileContent | No `symbols` rows for a remote file. | Contract change | core + native |
| D6 | artifactSearch, securityCheck | Ecosystem spellings (`pypi` / `PyPI`) not unified. | Renaming an input value breaks existing calls; both spellings are accepted | core |
| D7 | clasify | `sections` has no `maxRequests` or price check. | Needs a contract change | core + native |
| D8 | ghSearchHistory | Route text for `keywords` + `path` doesn't fit the surface budget. | Core size budget | core |

## By design (not to fix)

| Item | Reason |
|---|---|
| clasify packs no short items into one request | A billed A/B test changed 2 of 15 verdicts |
| ghCloneRepo with `historyDepth` gets its own object store | A shallow history needs its own boundary; branches, SHAs and sparse paths share one store |
| Descriptions name opt-in tools conditionally ("ghCloneRepo, if listed") | They stay correct when the tool is not served |
| About 10 low rater findings from round 1 | Already documented in core |

## Housekeeping

| Item | Status |
|---|---|
| `packages/octocode-native/scripts/test-binaries.cjs` and `tests/test-binaries.test.cjs` are not Prettier-formatted (they weren't before either; native lint doesn't enforce it) | open, low |
| Two clone checkouts from 2026-11-09 under `~/.octocode/clones/octocat/hello-world` predate the shared store and have their own `.git` | will age out of the cache |
