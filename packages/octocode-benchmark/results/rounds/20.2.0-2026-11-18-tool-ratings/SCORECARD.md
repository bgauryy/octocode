# Scorecard: Octocode `20.2.0`, `2026-11-18`

All 17 tools rated on the newest build. Two tools are capped at 4 by the rating rule:
- **ghSearchCode:** required pages are unreachable after 625 files.
- **securityCheck:** a wrong "no fix" claim when the only fix is a prerelease.

Every other tool scores 6.7–9.4, and no other answer was wrong. Rating only; nothing was fixed.

| Round | Value |
|---|---|
| Version | `octocode 20.2.0` |
| Binary | `campaigns/20.2.0-2026-11-18-tool-ratings/bin/octocode`, SHA-256 `3d6915966ca51124156ffd5f50e3fb5e426a6c581081f4210bb60f18192e14a4`. It is a copy of round 3's `bin-r4l`: a release build of a snapshot of the working tree taken 2026-11-18 02:35. Every lane saw the same hash before and after its checks. |
| Launcher / MCP | `packages/octocode/out/octocode.js` `1996109d…`; `packages/octocode-mcp/dist/index.js` `b0afad5b…` |
| Source | HEAD `173754242`; tree dirty (uncommitted round-3 and build-split work) |
| Previous version compared | [20.2.0 full round, 2026-11-15](../20.2.0-2026-11-15-full-round/SCORECARD.md), "After" column. That round used an 8-dimension mean, so this is a reference only, never a carried score. |
| Surfaces | CLI and MCP (stdio). At least one parity call per tool; cursors resumed across surfaces. |
| Agents and models | 5 check lanes (local, intel, gh-read, gh-history, pkg), Claude Opus 5.5. Orchestrator re-verified both caps, intel-4 and the weighted means. |
| Checked by / signed off by | lanes above / orchestrator. Neither fixed anything. |
| clasify requests billed | 42 (pkg 36, gh-history 6; configured key, `jev-1.14.0`) |
| Conditions | load average 42–61 throughout: timings are indicative only |

## Verdict

| Area | Verdict | Blocking items |
|---|---|---|
| Core tools | **Not ready** | gh-read-1, pkg-1 |
| Security | not checked as an area (partial evidence below) | — |
| Skills | not checked | — |
| Release gates | not checked | — |
| Benchmark | not checked | — |
| **Overall** | **Not ready** | gh-read-1, pkg-1 |

## Core tools

Weights per [RATINGS.md](../../../RATINGS.md): correctness 30%, schema/input 15%, output 15%, pagination 10%, workflow 10%, efficiency 15%, cache/freshness 5%. Evidence is in `campaigns/20.2.0-2026-11-18-tool-ratings/<lane>/raw/`.

| Tool | Correctness | Schema/input | Output | Pagination | Workflow | Efficiency | Cache/freshness | Overall | 11-15 after | Confidence / weight | Questions |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|---|
| localSearch | 9 | 8 | 7 | 9 | 9 | 7 | 10 | **8.3** | 8.6 | high (timing medium) / 100% | LS-01–07, checkout walks, stress |
| localFetch | 10 | 8 | 8 | 8 | 9 | 7 | 10 | **8.7** | 8.8 | high (timing medium) / 100% | LF-01–07, 5 MB line, minified, redaction |
| structureSearch | 10 | 9 | 8 | 10 | 9 | 8 | 10 | **9.2** | 8.8 | high (timing medium) / 100% | ST-01–07 |
| astSearch | 10 | 9 | 8 | 9 | 9 | 6 | 8 | **8.7** | 8.4 | high / 100% | AS-01–07, real TS/Rust |
| lspSearch | 7 | 8 | 7 | 6 | 7 | 4 | 9 | **6.7** | 8.3 | medium / 100% | LSP-01–05, Python, Rust |
| graphBuild | 8 | 9 | 8 | 8 | 8 | 7 | 9 | **8.1** | 8.3 | medium / 100% | GB questions, config + io |
| graphQuery | 8 | 9 | 7 | 10 | 8 | 7 | 9 | **8.1** | 4 (capped) | medium-high / 100% | GQ questions, shallow cochange |
| clasify | 9 | 8 | 7 | 6 | 7 | 7 | 9 | **7.8** | 8.0 | medium / 100% | CL-01–06 + source kinds |
| ghSearchRepo | 9 | 9 | 7 | 9 | 9 | 8 | 8 (provisional) | **8.5** | ~8.3 | medium / 100% | GHR-01–05 |
| ghSearchCode | 5 | 9 | 8 | 3 | 8 | 6 | 9 | **4.0** (6.5 uncapped) | 8.5 | high / 100% | GHC-01–05, walk lead, atRef |
| ghStructure | 9 | 9 | 9 | 10 | 9 | 8 | 9 | **9.0** | 8.8 | high / 100% | GHS-01–06 |
| ghGetFileContent | 10 | 9 | 8 | 10 | 9 | 8 | 9 | **9.1** | ~9.3 | high / 100% | GHF-01–06, 8 MB file |
| ghSearchHistory | 9 | 9 | 8 | 10 | 9 | 8 | 9 | **8.8** | ~8.6 | high / 100% | GHH-01–04, cap walk, K1 |
| ghGetHistoryItem | 10 | 9 | 8 | 8 | 9 | 8 | 9 | **8.9** | ~8.9 | high / 100% | GHI-01–06 |
| artifactSearch | 9 | 9 | 8 | 9 | 9 | 8 | 9 | **8.7** | 8.5 | high / 100% | ART-01–08 (migration not run) |
| securityCheck | 6 | 8 | 7 | 10 | 9 | 8 | 7 | **4.0** (7.5 uncapped) | 8.7 | high / 100% (SEC-07 unverified) | SEC-01–06, tokenless |
| ghCloneRepo | 10 | 9 | 9 | NA | 9 | 9 | 10 | **9.4** | 9.0 | high / 90% | GCL-01–04 |

The graphBuild and graphQuery overalls are the weighted means (8.05 and 8.10); the lane reported 8.2 for both.

**Per tool: the strength, then the main deduction.**

- **localSearch:** every answer matched rg, including counts over 5,238 files, a 21-page walk and a 5-query batch.
  - Deduction: long-line hits cost 701 characters against 132 for `rg -o -b`.
  - Deduction: the `matchCount` sort means lines in some views and occurrences in others.
- **localFetch:** 27 KB and 5 MB lines rebuild byte-exact with three secrets redacted; credential, binary and symlink reads are refused.
  - Deduction: long-line pages ignore `responseChars` below about 4K.
  - Deduction: a 5 MB line takes 281 calls at the default page size.
- **structureSearch:** every listing matched find, fd, stat and wc; a 631-row sorted walk was identical including ties; it used fewer characters than the rival.
  - Deduction: `--json` embeds the metric in the name string.
- **astSearch:** matches ast-grep exactly; the extra Rust hits are real calls inside macro bodies.
  - Deduction: about 3.4× the characters and 2× the time of ast-grep.
- **lspSearch:** correct sets against the direct server (references, callers and callees to depth 2, shadowing, hover columns, Python, Rust).
  - Deduction: without `workspaceRoot`, `references` scans the whole repository: 85 s cold on a 2-file fixture, partial forever, with false importer claims (intel-2). With a root it takes 0.39 s.
  - Deduction: a scan-only `implementation` row from another project (intel-4).
- **graphBuild:** accurate receipts, stable ids, near misses repaired.
  - Deduction: false `generated` and `entry` roles hide real findings (intel-7).
- **graphQuery:** last round's cap is lifted: the shallow-clone `cochange` case is now right and full history matches `git log`. Pagination over 45 calls is complete.
  - Deduction: `drift` fails on every call (intel-1).
- **clasify:** right answers and deciding lines on every source kind; an edit is judged anew; replays cost 0 calls.
  - Deduction: the `next.unfiltered` lead fails at once with `staleSnapshot` (pkg-2).
  - Deduction: `next.read` is broader than the deciding lines.
- **ghSearchRepo:** result order and totals match the API.
  - Deduction: `pushedAt`, the sort key, is never shown.
- **ghSearchCode:** every served page matched the oracle (paths, hit lines, `alsoAt`, `atRef:false`); CLI = MCP.
  - **Cap:** a walk is refused after about 625 files (gh-read-1).
- **ghStructure:** trees, tag peels and materialized bytes are exact.
  - Deduction: small costs only.
- **ghGetFileContent:** reads rebuild byte-exact, including a minified 8 MB file.
  - Deduction: continuation pages repeat about 450 characters of fields.
- **ghSearchHistory:** the 1,000-result cap is disclosed, and two date windows return all 1,198 rows exactly once. K1 is fixed: PR bodies are judged.
  - Deduction: two misleading texts (gh-history-2, gh-history-3).
- **ghGetHistoryItem:** byte-exact reconstruction of bodies, files, reviews, a 27 KB patch and 14–35 KB comments.
  - Deduction: `next.comments` is printed again with the same cursor (gh-history-1).
- **artifactSearch:** one call maps a version to its repository, directory and commit in 8 ecosystems, all correct; the cache obeys no-store, `Vary: *`, revalidation and 304.
  - Deduction: `ref` and `releaseTag` are handled differently across ecosystems.
- **securityCheck:** verdicts, alias groups and severities match OSV on every target; a tokenless run discloses the rate limit in 3.5 s.
  - **Cap:** pkg-1.
- **ghCloneRepo:** checkouts match the remote; depth and paths give distinct checkouts; a dirty refresh refuses and keeps the edit; reuse takes 0.1–0.3 s.
  - Deduction: `branch` is not repaired to `ref`.

**Workflows** (calls this round; last round not compared):

| Workflow | Calls | Answer right? |
|---|---|---|
| localSearch → localFetch range | 2 | yes (range = ast-grep's) |
| artifactSearch resolve → ghGetFileContent at ref | 2 | yes (zod, requests, serde) |
| clasify evaluate → `next.read` | 2 | yes; read broader than needed |
| ghCloneRepo sparse → batched localSearch | 2 | yes (3 definitions, 0.24 s) |
| ghSearchHistory, 1,198 rows past the cap | 2 date windows | yes, each row once |
| ghSearchCode walk of 749 files | 14, then refused | **no**: 99 unreachable |

## Security

| Check | Result | Evidence |
|---|---|---|
| Partial redaction | Pass | local: three `ghp_` tokens in a 5 MB line, only the token redacted, the same in every view; columns counted after redaction |
| Large-file masking | Pass (local only) | the same 5 MB line |
| Credential files withheld | Pass | localFetch refuses `.env` and `id_rsa`; ghCloneRepo skips and lists credential files |
| Symlinks | Pass (localFetch) | symlink read refused |
| Path policy, provider egress, clone sandbox, prompt injection | not checked | — |
| Tokens and refusals | Pass | tokenless securityCheck gives the same verdicts and names the fix |
| Processes | Pass | lanes stopped their own servers by PID; the shared `serve` (PID 38779) was left to its idle exit |

## Defects

Repro calls run on the pinned binary. `<lane>` = `packages/octocode-benchmark/campaigns/20.2.0-2026-11-18-tool-ratings/<lane>`.

| ID | Tool | Severity | Repro | Expected | Observed |
|---|---|---|---|---|---|
| gh-read-1 | ghSearchCode | **blocker** | walk `{"owner":"vercel","repo":"next.js","keywords":["useState"]}` through every `next.nextPage` | all 749 files | call 15: `invalidInput` "Parameter listed exceeds maximum length (10000 characters)". 650 walked, 99 unreachable; vscode `path:src/vs/platform` `Disposable`: 627 of 1,000. **Cause (verified):** the private `listed` field holds a 16-hex digest per file (`tool-github/src/gh_search_code/listed.rs:5`), and the input policy caps strings at 10,000 characters (`runtime-base/src/security/input.rs:20`), so a walk breaks at 625 files. The code comment assumes 1,000 × 16 fits. |
| pkg-1 | securityCheck | **blocker** | `{"operation":"package","ecosystem":"npm","packageName":"next","version":"13.4.0"}`; parse-server 5.0.0 | `upgradeTo` 15.5.28 / 8.6.84, or "only a prerelease fix is listed" | no `upgradeTo`, and the note "GHSA-c59h-r6p8-q9wc states no fixed version above 13.4.0", while OSV states `13.4.20-canary.13`. **Cause (verified):** `tool-packages/src/security_check/upgrade.rs:80` `candidate()` drops non-plain-dotted fix versions. |
| intel-1 | graphQuery | major | `{"operation":"drift","path":"<intel>/fx-sem","baseline":"<id>"}` | a drift answer | `outputContractViolation`: `baseline.root` and `baseline.createdAt` missing, on every call, CLI and MCP |
| intel-2 | lspSearch | major | `{"operation":"references","path":"test-utils/fixtures/lsp/cjs-noconfig/a.cjs","symbolName":"stage","line":4}` | both files, quickly | 85 s cold; b.cjs only after `nextImporterWindow`; partial forever, with "40 files import it" and an unrelated HTML file "uses this name". With `workspaceRoot`: 0.39 s, complete. |
| intel-4 | lspSearch | major (judgment: not capped) | `{"operation":"implementation","path":"<intel>/fx-sem/types.ts","symbolName":"Shape","line":1}` | Circle, NamedCircle | adds `test-utils/fixtures/demo-repo/advanced-local/code/model.ts:5 Square` (another project's `Shape`), outside the reported `workspaceRoot`. Labeled `native-graph-facts` with `partialReasons: ["scanOnlyImplementations"]`, so it is disclosed as a scan candidate, not silent. Re-run by the orchestrator: same output. |
| pkg-2 | clasify / localSearch classification | major | localSearch with `classification`, then the printed `next.unfiltered` at once | the unclassified hits | `staleSnapshot` "nothing shown remains valid", CLI and MCP; `next.restart` recovers in 1 more call |
| intel-3 | lspSearch | minor | references on a parameter or nested function (`<intel>/fx-sem/shadow.js:2`) | 2 rows, complete | right rows, but `importerScanCapped` and "31 import it"; the window lead adds nothing |
| intel-5 | lspSearch | minor | first `callees` on a freshly opened file | error diagnostics stated | first call omits them and claims complete; a repeat reports `errorCount 1` |
| intel-6 | lspSearch | minor | paged references, two on one line | distinct rows | `column` dropped: rows look identical |
| intel-7 | graphBuild/graphQuery | minor | `packages/octocode-config` build | real roles | `rust.ts` marked generated (marker inside a string); every `scripts/` module is an entry; 3 of 8 export-only-local findings |
| intel-8 | graphBuild | minor | `parserGlobs {"javascript":["*.custom"]}` | language javascript | reported as `js` with "linking unsupported" |
| intel-9 | graph, lspSearch | minor | various | lean rows | full paths beside ids; witness `path` = `from`; `language: ""`; leads with `path:"."` |
| intel-10 | astSearch | minor | any call with leads | `view` kept | `related` drops `view`; diagnostics printed as raw JSON |
| gh-read-2 | ghSearchRepo | minor | `list` sorted by pushedAt | `pushedAt` shown | never shown, even when requested |
| gh-read-3 | ghSearchRepo | minor | `sort updatedAt` | order explained | provider order, unexplained |
| gh-read-4 | ghSearchCode | minor | an empty result | scope stated | `coverage: complete` without "default-branch index only" |
| gh-read-5 | ghGetFileContent | minor | `responseChars:300` walk | lean continuations | `matchedLines: []`, `blocks` and `totalLines` repeated (~450 chars a page) |
| gh-history-1 | ghGetHistoryItem | minor | `{"operation":"issue","owner":"microsoft","repo":"vscode","number":93814,"sections":["comments"]}` | each lead printed once | `next.comments` repeated with the same cursor at each long-comment chain's end (cli/cli#9370: 5 times) |
| gh-history-2 | ghSearchHistory | minor | commit search with `path` and a `committedAt` window that is empty | "none in that window" | "No commit touches <path> on the default branch" |
| gh-history-3 | ghSearchHistory | minor | PRs sorted by `commentsCount` | the sorted value shown | issue comments only (84, 58, 1); GitHub ranks by issue + review comments |
| gh-history-4 | ghGetHistoryItem | minor | discussion `matchString` | lines around the hit | whole 12,000-char body window |
| gh-history-5 | ghCloneRepo | minor | `{"owner":"cli","repo":"cli","branch":"trunk"}` | `branch` read as `ref`, or `ref` named | "Unknown field 'branch'" |
| local-1 | localSearch | minor | a `skippedPaths` group of 2 | both names or the parent | only `bigline.min.js` |
| local-2 | localSearch | minor | `sort.by:"matchCount"` | one meaning | lines in content/countLines; occurrences elsewhere |
| local-3 | localFetch | minor | long line, `responseChars:500` | ~500-char pages | 4,096 chars, no `oversized` flag |
| local-4 | localSearch | minor | long-line hit (LS-04) | lean row | path ×3, description restated (701 vs 132 chars) |
| local-5 | all repairs | minor | `sort:"bytes"` etc. | names the corrected value | names the field already sent |
| local-6 | localSearch | minor | output reference | localSearch fields only | documents astSearch-only fields; `value` wrong with `contextLines` |
| local-7 | localSearch | minor | `keyPath` on a .txt file | a reason | empty, no reason, a lead that repeats the same keyPath |
| local-8 | localFetch | minor | overlapping and unordered `ranges` | documented | merge and sort undocumented |
| local-9 | CLI text | minor | countLines, matchOnly, skippedPaths | consistent text | count table, pagination and skippedPaths inconsistent; `# filesScanned` before an error |
| pkg-3 | securityCheck | minor | lodash 4.17.20 `details`; a commit's `details` | ranges and references, as the view states | missing; fixedIn merges records unattributed; commit details omit the package |
| pkg-4 | securityCheck | minor | advisory `details` | GitHub's references as given | all labelled `WEB` |
| pkg-5 | securityCheck | minor | tokenless batch past the quota | one statement | rate-limit text repeated in a row and on every row |
| pkg-6 | securityCheck / docs | minor | lodash twice within 5 min | as documented | OSV reused ~5 min (`observedAt … cached`); `docs/STORAGE_AND_LIMITS.md` says "fetched fresh" |
| pkg-7 | securityCheck | minor | `name` field | repaired, as artifactSearch does | `Unknown field 'name'` |
| pkg-8 | securityCheck | minor | commit 6879efc; Go x/net v0.17.0 | CVSS-only severity counted; consistent source | `highestSeverity: medium` despite a 7.5 vector; "github found" + "not a GitHub repository" |
| pkg-9 | artifactSearch | minor | NuGet, Packagist and Go versions; PyPI requests | tag verification, `releaseTag` | `registryRef` though tags exist; tag name as `ref` with no `releaseTag` |
| pkg-10 | artifactSearch, securityCheck | minor | not-found package or advisory | package hint | file-path advice |
| pkg-11 | artifactSearch | minor | answer reused under max-age | `observedAt` | missing |
| pkg-12 | clasify | minor | file, GitHub and URL sources | read at the deciding lines | lines 1–95 for 60–61; the whole 9.9 KB capture |
| pkg-13 | clasify | minor | output reference | every `next.*` branch | generic `next` only |

42 defects: 2 blockers, 4 major, 36 minor.

## Not checked

- Areas: skills, release gates, the benchmark against stored baselines, and most of the security area (path policy, egress, clone sandbox, prompt injection).
- A fresh-agent understanding test for any tool.
- The published release comparison (`npx -y octocode@latest`).
- A cold CLI first call: the shared warm `serve` could not be isolated without breaking other lanes' cursors.
- Per lane:
  - **local:** `classification`, `testCode`, `fileType` and dotall against an oracle; other sorts; structureSearch permissions, access, bytes and minDepth filters; UTF-16; Windows.
  - **intel:** LSP after the idle timeout; Go, Java, C and Scala servers; rust-analyzer on a real crate; Graphify; large or incremental graph builds; multi-MB astSearch input.
  - **gh-read:** GHR-06/07; the walk refusal over MCP (quota); Enterprise and private repositories; truncated trees; `continueMaterialize`.
  - **gh-history:** redaction in bodies; ghGetHistoryItem classification; a full clone of a large repository; rate-limit behaviour (none occurred).
  - **pkg:**
    - ART-08 migration;
    - SEC-07 (OSV has no base-URL override);
    - artifactSearch page walks;
    - clasify images and box questions, multi-window sources, `nextCandidatePage` and `unfilteredUnknown`.
- Whether gh-read-1 and pkg-1 are new in this build: the 11-15 round's walks and targets did not cover them.

## Re-rating on `bin-r4o` (2026-11-18, after the round-3 fixes)

The six P0/P1 defects above were retested by hand on round 3's final pin, apart from the session that fixed them. Only the dimensions those defects touched were re-scored; every other score stands as rated above.

| Round | Value |
|---|---|
| Binary | `campaigns/20.2.0-2026-11-16-round3/bin-r4o/octocode`, SHA-256 `924638a5b8f711dfc73347494e4d021e2134dcb2f647bcf3643a93efc9f34a8e` |
| Surface | CLI only (`--json`); `lspSearch` in `OCTOCODE_STORAGE_MODE=memory` so no warm server was left behind |
| Evidence | `campaigns/20.2.0-2026-11-18-tool-ratings/rerate-r4o/raw/` |
| Conditions | a full native test gate ran beside the checks: timings are indicative only |

| ID | Retest | Result |
|---|---|---|
| gh-read-1 | the next.js `useState` walk through every `next.nextPage` | **fixed**: 16 pages, no refusal; 681 distinct files plus 68 `alsoAt` locations = GitHub's final 749 results, no file twice. GitHub's count moved 792 → 749 mid-walk; page 14 discloses it (`providerResultsShifted`). |
| pkg-1 | next 13.4.0 and parse-server 5.0.0 | **fixed**: `upgradeTo` 15.5.28 and 8.6.84, the expected values |
| intel-1 | `drift` against a baseline after a one-file edit and rebuild (copy of `intel/fx-sem`) | **fixed**: answers, `filesChanged: 1`, one row; no `outputContractViolation` |
| intel-2 | the `cjs-noconfig` `references` call, cold | **fixed**: both files, 4 references (= `rg -w stage`), `state: complete`, no "40 import it", 6.6 s against 85 s. Cost found: without `workspaceRoot` the call peaks at 395 MB footprint and 37.6 G instructions, against 18.7 MB and 0.37 G with the root set (`/usr/bin/time -l`) |
| intel-4 | `implementation` of `Shape` on `intel/fx-sem/types.ts` | **fixed**: Circle and NamedCircle; the other project's `Square` is gone |
| pkg-2 | classified localSearch, then `next.unfiltered` | round 3's own check only (`round3/final-validate/rows.jsonl`: no `staleSnapshot`); **not retested here**, to avoid billed judge calls without the round's exact query |

| Tool | Changed dimensions | Overall | Was | Confidence |
|---|---|---:|---:|---|
| ghSearchCode | correctness 5 → 9, pagination 3 → 9, workflow 8 → 9 | **8.4** | 4.0 (capped) | high (CLI; MCP not rechecked) |
| securityCheck | correctness 6 → 9 | **8.4** | 4.0 (capped) | high (SEC-07 still unverified) |
| lspSearch | correctness 7 → 8, pagination 6 → 7, workflow 7 → 8, efficiency 4 → 6 | **7.5** | 6.7 | medium: intel-3, intel-5 and intel-6 not rechecked; the no-root peak above holds efficiency at 6 |
| graphQuery | correctness 8 → 9, output 7 → 8, workflow 8 → 9 | **8.7** (8.65) | 8.1 | medium-high |
| clasify | pagination 6 → 8 | **8.0** provisional | 7.8 | medium: pkg-2 rests on round 3's check alone |

**Verdict after re-rating:** both blockers are fixed and retested, so no cap remains. Core tools: **ready, provisional** until pkg-2 is retested independently and intel-3, intel-5 and intel-6 are rechecked. The other areas stay unchecked, as above.
