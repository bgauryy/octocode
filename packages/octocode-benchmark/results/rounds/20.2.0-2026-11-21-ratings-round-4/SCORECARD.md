# Scorecard: Octocode `20.2.0`, `2026-11-21` (rating round 4)

Eight independent rater agents scored all 17 tools again after fix round 5, on the
[12-dimension rubric](../../../RATINGS.md#agent-experience-rubric-12-dimensions), checking every claim against a source
outside Octocode. Rater scores average **8.6/10** (round 3: 8.2) and **9.5/10** for correctness; the gated average is
**8.1** (round 3: 7.6). Of the 47 round-3 issues the lanes covered, 34 are fixed, 12 partly fixed and 1 not re-tested as stated; none got
worse. lspSearch
is no longer capped. Two new findings fail the correctness gate, so astSearch (R4-K1, a silent miss) and graphQuery
(R4-G1, wrong in both directions) stay capped at 4, and core tools stay **Not ready**. A third high finding, R4-C1,
lets the search `classification` option spend money with no cap or consent. All 42 new findings are in
[FINDINGS.md](FINDINGS.md); each lane's report is in [lanes/](lanes/). Rating only; nothing was fixed during the round.

| Round | Value |
|---|---|
| Version | `octocode 20.2.0+56f67471` |
| Binary | `packages/octocode-native/npm/darwin-arm64/octocode`, SHA-256 `e246ff42592b8192da65589ef39a7f573f04f4643b15765e9d98fb5cff6a405f`, dev build of 2026-11-21 00:00 (the same SHA-256 as the build the final gates ran beside). The same binary for every lane, unchanged at the end of the round. |
| Source | Dirty tree (fix round 5, uncommitted); HEAD `e9309aa35` |
| Previous round compared | [Round 3, previous day](../20.2.0-2026-11-20-ratings-round-3/SCORECARD.md), the same lanes and rubric |
| Surfaces | Raters: CLI. The end-to-end lane also checked the MCP default surface on this binary (empty `OCTOCODE_HOME`) |
| Agents and models | 8 rater lanes, one fresh Claude Opus 5.5 agent each, briefed with [independent raters](../../../briefs/independent-raters.md); a Claude Opus 5.5 coordinator merged the lanes and re-ran each high finding itself |
| Checked by / signed off by | Rater lanes / coordinator. Neither fixed anything during the round. |
| clasify requests billed | 158 against a budget of 40: 27 capped clasify calls, plus 131 from one search `classification` call that takes no cap (R4-C1). The end-to-end lane billed 0. |
| Conditions | 2026-11-21, 00:01-00:27 local; lanes took 9-15 minutes. Machine load was 3-4 per CPU from other sessions (one lane saw load 31-40): timings are indicative. macOS arm64 only. |

## How it was measured

The [independent raters](../../../briefs/independent-raters.md) method, unchanged from round 3: one pinned binary; the
served instructions saved for the end-to-end lane; eight lanes (packages, clasify, local, code intelligence, GitHub read,
GitHub history, graph, end to end); fresh questions first; every claim checked against rg, ast-grep, find, git, sed,
cmp, `gh api` (GET), registry APIs, api.osv.dev or hand labels; a cold and warm call, a stress test, a batch of up to 5
and a full page walk per tool; then a re-test of each round-3 issue and of each fix round 5 claimed. The coordinator
re-ran each high finding on the pinned binary before recording it (R4-C1 without billing, from the schema and the
limit's source).

## Verdict

| Area | Verdict | Blocking items |
|---|---|---|
| Core tools | **Not ready** | R4-K1 (astSearch), R4-G1 (graphQuery) |
| Security | **Fail (narrow)** | R4-C1 (spend without consent), R4-K1 (redaction changes parsing) |
| Skills | not checked | — |
| Release gates | Ready on this tree except `packages/octocode-chrome-devtools`, which other sessions are editing | — |
| Benchmark | not run | — |
| **Overall** | **Not ready** | R4-K1, R4-G1, R4-C1 |

## Core tools

| Tool | Desc | Schema | Workflow | Clarity | Time | Tokens | Paging | Smart | Short | Long | Stress | Hints | Overall | Gated | Correct | Round 3 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| structureSearch | 9 | 9 | 9 | 8 | 9 | 9 | 10 | 9 | 9 | 9 | 9 | 8 | **9** | 9 | 10 | 9 |
| ghSearchRepo | 9 | 8 | 9 | 9 | 8 | 9 | 10 | 9 | 9 | 9 | 9 | 7 | **9** | 9 | 10 | 9 |
| ghSearchHistory | 9 | 9 | 9 | 9 | 9 | 9 | 10 | 9 | 9 | 9 | 9 | 9 | **9** | 9 | 10 | 9 |
| ghCloneRepo | 9 | 9 | 9 | 9 | 9 | 10 | – | 9 | 9 | 9 | 9 | 9 | **9** | 9 | 10 | 9 |
| localSearch | 9 | 9 | 9 | 8 | 9 | 8 | 10 | 8 | 9 | 9 | 9 | 9 | **9** | 9 | 10 | 8.5 |
| localFetch | 9 | 9 | 9 | 8 | 10 | 8 | 10 | 9 | 9 | 10 | 10 | 9 | **9** | 9 | 10 | 8 |
| ghStructure | 9 | 8 | 9 | 9 | 9 | 9 | 10 | 9 | 9 | 9 | 9 | 9 | **9** | 9 | 10 | 8 |
| ghSearchCode | 9 | 8 | 8 | 8 | 8 | 8 | 10 | 7 | 9 | 9 | 9 | 7 | **8.5** | 8.5 | 10 | 8 |
| ghGetFileContent | 9 | 9 | 9 | 8 | 9 | 8 | 9 | 8 | 9 | 9 | 9 | 9 | **8.5** | 8.5 | 9.6 | 8.5 |
| ghGetHistoryItem | 8 | 9 | 9 | 8 | 8 | 8 | 7 | 9 | 9 | 8 | 8 | 9 | **8.5** | 8.5 | 9.5 | 8 |
| artifactSearch | 9 | 9 | 9 | 9 | 7 | 9 | 9 | 9 | 9 | 8 | 8 | 8 | **8.5** | 8.5 | 10 | 8.5 |
| securityCheck | 9 | 8 | 9 | 8 | 8 | 7 | 9 | 9 | 9 | 8 | 8 | 8 | **8.5** | 8.5 | 9.5 | 8 |
| graphBuild | 8 | 9 | 9 | 8 | 8 | 9 | – | 8 | 9 | 8 | 8 | 9 | **8.5** | 8.5 | 9 | 8 |
| clasify | 8 | 8 | 8 | 8 | 9 | 8 | 8 | 8 | 8 | 7 | 9 | 8 | **8** | 8 | 8.9 | 8 |
| lspSearch | 8 | 8 | 9 | 8 | 6 | 8 | 9 | 7 | 9 | 9 | 7 | 8 | **8** | 8 | 9.3 | 7 (gated 4) |
| astSearch | 9 | 8 | 9 | 8 | 9 | 8 | 9 | 9 | 9 | 9 | 8 | 7 | **8** | **4** (R4-K1) | 9.3 | 8 (gated 4) |
| graphQuery | 8 | 8 | 8 | 8 | 9 | 7 | 9 | 8 | 8 | 8 | 8 | 9 | **8** | **4** (R4-G1) | 7 | 7 (gated 4) |
| **Average** | 8.7 | 8.5 | 8.8 | 8.3 | 8.5 | 8.4 | 9.3 | 8.5 | 8.9 | 8.6 | 8.6 | 8.4 | **8.6** | 8.1 | 9.5 | 8.2 (gated 7.6) |

"–" means the tool has no paging of its own. Correct is the share of the rater's verified claims that were right, on
a 10 scale. The search `classification` option was rated apart from the tools that carry it: **6** overall, 8.5
correct, held down by R4-C1 (Schema 5, Stress 3); it does not lower the localSearch or ghSearchCode rows, whose lanes
rated their own behavior.

Round-3 issues re-tested, by lane:

| Lane | Fixed | Partly fixed | Not fixed / not re-tested |
|---|---|---|---|
| Code intelligence | K1, K3, K4, K5, K6, K7 | K2 (round-3 input 3/3; R4-K1 in the same file) | — |
| Graph | G1, G4, G5, G6 | G3 (file linked, call unresolved: R4-G3) | G2 not re-tested as stated (a similar `since` check was exact, 99 of 99) |
| clasify | C2, C3, C4, C6 | C1 (R4-C2), C5 (no top-level `next.read` for a glob resource) | — |
| Local | L1, L4, F3; L2 fixed by disclosure (`maskedSpans`) | L3 (R4-L2), L5 (R4-L3) | — |
| GitHub read | GR3, GR4, GR5, F1 | GR1 (R4-GR4), GR2 (PHP: R4-GR1), GR6 (R4-GR3) | — |
| GitHub history | H3, H4, H5, H6, H7 | H1 (R4-H1), H2 (R4-H2) | — |
| Packages | P1, P2, P3, P4, P5, P6 | P7 (`reqeusts`, securityCheck `@Angular`, bare Maven name) | — |
| End to end | F1 (`refs`), F2 (first-release route) | — | — |

## Workflows

| Workflow | Calls this round | Answer right? | Score | Note |
|---|---|---|---|---|
| Local: which tools `octocode mcp` serves by default and where that is decided | 11 (+2 config, +2 help), 22.9K chars, 117 s | yes (`runtime-base/src/execution/availability.rs`) | 8 (new question) | localSearch with `keyPath` → localFetch → config list |
| Public: first axios release with the fetch adapter (PR #6371) | 7 (+3), 5.2K chars, 78 s | yes (v1.7.0-beta.0; stable v1.7.0) | 9 (new question) | ghSearchHistory → ghStructure `refs` glob → ghGetHistoryItem compare |
| Round 3's: serde_derive struct visitor and the release that last changed it | 8, 7.7K chars, 72 s | yes (v1.0.227, unchanged through v1.0.229) | 9 (round 3: 8) | artifactSearch → ghGetFileContent → ghSearchHistory per tag |
| Guidance (instructions and `--help`) | – | routing right first time in all 3 | 9 (8) | R4-F1 error examples drop the bad field; R4-F2 empty-LSP lead |

11 of 11 end-to-end claims verified. The MCP default surface on an empty home serves 12 of 17 tools; the other 5
refuse with `toolUnavailable` naming the setting that enables each, and the served instructions match the pinned copy
byte for byte.

## Security

| Check | Result | Evidence |
|---|---|---|
| Provider egress (billing) | **Fail** | R4-C1: one search with `classification` billed 131 requests with no cap, price or confirm; clasify's own plans still cost 0 |
| Redaction vs parsing | **Fail (narrow)** | R4-K1: an unterminated key block in a file hides every later astSearch match |
| Partial redaction | Pass | R3-L1 fixed (counts equal rg); R3-L2 disclosed per row as `maskedSpans` |
| Credential files | Pass | Sparse clones and code search withhold credential-named files and say so |
| Malware advisories | Pass | R3-P1 fixed: unconfirmed names go to OSV, disclosed with `registryConfirmed: false` |

## Release gates

From fix round 5's final gates on this tree (2026-11-20 21:44-21:46): clippy ratchet clean (baseline lowered),
`docs:rust`, `docs:verify`, `check-outputs`, `skills:check` pass; crate tests 0 failures (runtime-base 1,130,
tool-github 795, tool-packages 682, tools-shared 665, tool-graph 898, tool-local 764, engine 1,837, tool-ast 406,
tool-clasify 123); core lint and 1,902/1,902 tests pass. `verify` and `test:ci` fail only in
`packages/octocode-chrome-devtools` (oxlint findings and a build error in files other sessions were editing).

## Defects

All 42, with exact inputs, are in [FINDINGS.md](FINDINGS.md). The ones that block or cost the most:

| ID | Area and tool | Severity | Repro call (exact, on the pinned binary) | Expected | Observed | Owner |
|---|---|---|---|---|---|---|
| R4-K1 | astSearch | blocker | `{"operation":"match","path":"packages/octocode-native/crates/runtime-base/src/security/content/tests/key_blocks.rs","rule":{"pattern":"$X.clone()"}}` | 5 matches (ast-grep) | `empty`, no warning (re-run by the coordinator); an unterminated key block at :136-139 is redacted through the end of the file before parsing | native |
| R4-G1 | graphQuery | blocker | `{"operation":"impact","path":"packages/octocode-native","target":"crates/engine/src/num.rs#saturating_u32","kind":"symbol","depth":1}` | the files that call `saturating_u32`, including `tool-local/src/structure_search/listing.rs:176` | 104 rows (re-run by the coordinator): lists `publish.rs`, which uses only `saturating_usize`; omits `listing.rs`, whose call sits in a `macro_rules!` body, with no warning | native |
| R4-C1 | localSearch / ghSearchCode `classification` | major (spend) | `{"path":"packages/octocode-native/crates","matchString":["Mutex::new"],"regexEngine":"literal","classification":{"predicates":["The hit guards process-wide state."]}}` | a cap field, or a free price and confirm before billing | 131 billed requests in one call; the schema has no `maxRequests` (checked by the coordinator) | core + native |
| R4-H1 | ghGetHistoryItem | major | kubernetes#141224 `sections:["patches"]`, every page | a read for each withheld patch | none past page 1; 22 files marked unreachable that ghGetFileContent reads | native |
| R4-GR1 | ghGetFileContent | major | laravel/framework `src/Illuminate/Support/Str.php`, `matchString:["function slug("]`, `block:true` | lines 1665-1688 | 1665-1665, marked an estimate | native |
| R4-P1 | securityCheck | major | Go `github.com/gin-gonic/gin/binding` v1.9.0 | the module's advisories, or a lead to them | "none known to OSV" for the module | native |

## Not checked

- Skills, the benchmark comparison, Windows and Linux.
- clasify `next.nextCandidatePage` and `next.unfilteredUnknown` (they bill; the clasify lane stopped billing after the
  R4-C1 overrun).
- R3-G2 as stated (the fixture guard refused git commits).
