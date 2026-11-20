# Scorecard: Octocode `20.2.0`, `2026-11-20` (rating round 3)

Eight independent rater agents scored all 17 tools again after fix round 4, on the
[12-dimension rubric](../../../RATINGS.md#agent-experience-rubric-12-dimensions), checking every claim against a source
outside Octocode. Rater scores average **8.2/10** (round 2: 8.0) and **9.4/10** for correctness. Every round-2 issue
the raters re-tested is fixed or partly fixed. Three new findings fail the correctness gate (two silent misses and
one wrong count), so astSearch, lspSearch and graphQuery are capped at 4 and core tools stay **Not ready**. All 48
new findings are in [FINDINGS.md](FINDINGS.md). Rating only; nothing was fixed during the round.

| Round | Value |
|---|---|
| Version | `octocode 20.2.0+4476eb38` |
| Binary | `packages/octocode-native/npm/darwin-arm64/octocode`, SHA-256 `cb4b778676fb78e0135c60e7f79d20523de1b32bf1f97e4de151ecfc1ca1a422`, dev build of 2026-11-20 13:08. The same binary for every lane. |
| Source | Dirty tree (fix round 4, uncommitted); HEAD `6717848eb` |
| Previous round compared | [Round 2, same day](../20.2.0-2026-11-20-independent-ratings/SCORECARD.md), the same lanes and rubric |
| Surfaces | Raters: CLI. The MCP default surface was checked on the round-2 build; the final gates of fix round 4 ran on this binary |
| Agents and models | 8 rater lanes, one fresh Claude Opus 5.5 agent each, briefed with [independent raters](../../../briefs/independent-raters.md); a Claude Opus 5.5 coordinator merged the lanes and re-ran the gating findings itself |
| Checked by / signed off by | Rater lanes / coordinator. Neither fixed anything during the round. |
| clasify requests billed | 20 (the clasify lane's full budget, each with an explicit `maxRequests`); the end-to-end lane billed 0 |
| Conditions | 2026-11-20, 15:12–15:43 local; lanes took 9–22 minutes. Other sessions edited the repository and reorganized this package during the round: timings are indicative. macOS arm64 only. |

## How it was measured

The [independent raters](../../../briefs/independent-raters.md) method, unchanged from round 2: one pinned binary; the
served instructions saved for the end-to-end lane; eight lanes (packages, clasify, local, code intelligence, GitHub read,
GitHub history, graph, end to end); fresh questions; every claim checked against rg, find, git, sed, cmp, `gh api` (GET),
registry APIs, api.osv.dev or hand labels; a cold and warm call, a stress test, a batch of up to 5 and a full page walk
per tool; then a re-test of each round-2 issue reported fixed. Items still open by design were named to the raters as
"known open" so they would only report them if worse. The coordinator re-ran each finding that fails the gate on the
pinned binary before recording it.

## Verdict

| Area | Verdict | Blocking items |
|---|---|---|
| Core tools | **Not ready** | R3-K1 (lspSearch), R3-K2 (astSearch), R3-G1 (graphQuery) |
| Security | not checked as an area (partial evidence below) | — |
| Skills | not checked | — |
| Release gates | Ready on this binary, with one load-sensitive process test | — |
| Benchmark | not run | — |
| **Overall** | **Not ready** | R3-K1, R3-K2, R3-G1 |

## Core tools

| Tool | Desc | Schema | Workflow | Clarity | Time | Tokens | Paging | Smart | Short | Long | Stress | Hints | Overall | Gated | Correct | Round 2 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| structureSearch | 9 | 9 | 9 | 8 | 9 | 8 | 9 | 9 | 9 | 8 | 8 | 8 | **9** | 9 | 10 | 8.5 |
| ghSearchRepo | 9 | 8 | 8 | 9 | 9 | 9 | 10 | 8 | 9 | 9 | 8 | 8 | **9** | 9 | 10 | 8 |
| ghSearchHistory | 9 | 9 | 9 | 8 | 8 | 9 | 10 | 9 | 9 | 9 | 9 | 8 | **9** | 9 | 10 | 9 |
| ghCloneRepo | 8 | 9 | 8 | 9 | 9 | 10 | – | 9 | 9 | 9 | 8 | 9 | **9** | 9 | 10 | 8 |
| localSearch | 8 | 9 | 8 | 8 | 8 | 8 | 9 | 8 | 9 | 9 | 8 | 7 | **8.5** | 8.5 | 9.5 | 8.5 |
| ghGetFileContent | 8 | 9 | 8 | 8 | 9 | 7 | 10 | 8 | 9 | 9 | 9 | 8 | **8.5** | 8.5 | 10 | 8 |
| artifactSearch | 9 | 8 | 9 | 8 | 7 | 9 | 9 | 8 | 9 | 8 | 8 | 8 | **8.5** | 8.5 | 9.6 | 8 |
| localFetch | 9 | 9 | 8 | 8 | 9 | 8 | 10 | 9 | 9 | 9 | 9 | 7 | **8** | 8 | 8.8 | 9 |
| clasify | 8 | 8 | 8 | 8 | 9 | 9 | 7 | 8 | 8 | 8 | 7 | 8 | **8** | 8 | 9 | 7 |
| ghSearchCode | 9 | 8 | 7 | 7 | 8 | 8 | 10 | 8 | 9 | 8 | 8 | 7 | **8** | 8 | 9.4 | 8 |
| ghStructure | 9 | 8 | 9 | 8 | 6 | 9 | 10 | 9 | 9 | 8 | 8 | 7 | **8** | 8 | 10 | 7 |
| ghGetHistoryItem | 8 | 9 | 8 | 8 | 8 | 8 | 7 | 9 | 9 | 9 | 8 | 8 | **8** | 8 | 9.5 | 8 |
| securityCheck | 9 | 8 | 8 | 8 | 7 | 7 | 8 | 7 | 9 | 8 | 8 | 7 | **8** | 8 | 9.6 | 8 |
| graphBuild | 8 | 9 | 9 | 8 | 6 | 9 | – | 8 | 9 | 8 | 8 | 8 | **8** | 8 | 9 | 8 |
| astSearch | 9 | 9 | 9 | 7 | 9 | 8 | 8 | 7 | 9 | 9 | 8 | 7 | **8** | **4** (R3-K2) | 9 | 9 |
| lspSearch | 8 | 8 | 8 | 7 | 6 | 8 | 9 | 6 | 9 | 8 | 7 | 6 | **7** | **4** (R3-K1) | 8 | 7 |
| graphQuery | 8 | 8 | 8 | 7 | 8 | 7 | 9 | 8 | 8 | 8 | 7 | 8 | **7** | **4** (R3-G1) | 8.8 | 7 |
| **Average** | 8.5 | 8.5 | 8.3 | 7.9 | 7.9 | 8.3 | 9.0 | 8.1 | 8.9 | 8.5 | 8.0 | 7.6 | **8.2** | 7.6 | 9.4 | 8.0 |

"–" means the tool has no paging of its own. Correct is the share of the rater's verified claims that were right, on
a 10 scale. localFetch is not capped: its phantom matches (R3-L1) show the placeholder text in the returned lines, so
they mislead no reader of the rows, but the count is wrong and it is the first fix to make.

Of the round-2 issues re-tested, every one is fixed or partly fixed: partly are L1 (localFetch), K2 (dynamic imports),
K6 (isPartial beside hasMore on some pages), C3 (a 170 KB source), P3 (records per advisory) and GR8 (speed).

## Workflows

| Workflow | Calls this round | Answer right? | Score (round 2) | Note |
|---|---|---|---|---|
| Local: where clasify may spend without `maxRequests` | 8 (+5 help), 31K chars, ~76 s | yes | 9 (new question) | localSearch → localFetch → clasify (free refusal) |
| Local: how search never matches redaction text | 12 (+1), 45K chars, ~110 s | yes | 9 (new question) | localSearch → localFetch → lspSearch → localSearch behavior batches |
| Public: serde `Deserialize` expansion and the release that last changed the visitor | 10 (+5), 12K chars, ~92 s | yes (1.0.227) | 8 (new question) | artifactSearch → ghStructure → ghGetFileContent → ghSearchHistory per tag |
| Guidance (instructions and `--help`) | – | routing right in all 3 | 8 (8) | names a ghStructure operation `tags` that doesn't exist; no route for "first release with a change" |

17 of 17 end-to-end claims verified.

## Security

| Check | Result | Evidence |
|---|---|---|
| Partial redaction | Fail (narrow) | R3-L1: localFetch matches placeholder text; R3-L2: hits inside masked spans are dropped from rows with other hits without a note |
| Redaction vs parsing | Fail (narrow) | R3-K2: a redacted key block changes how astSearch parses a file, hiding a real match |
| Provider egress (billing) | Pass | clasify's uncapped plans cost 0 and return a price and `next.confirm`; 20 billed requests, all capped |
| Malware advisories | Gap | R3-P1: a package removed from its registry is never sent to OSV, although OSV lists it as malicious |

## Release gates

From fix round 4's final gates on this binary (2026-11-20 13:20): `verify` 9,385/9,385 unit tests and 481/482 process
tests (the one failure, a rustup-proxy test unchanged since 2026-11-19, passes alone and fails only under heavy load);
clippy ratchet clean; `docs:verify`, `docs:rust`, `skills:check`, `test:ci`, `check-outputs` pass; core lint and 1,866/1,866
tests pass.

## Defects

All 48, with exact inputs, are in [FINDINGS.md](FINDINGS.md). The ones that block or cost the most:

| ID | Area and tool | Severity | Repro call (exact, on the pinned binary) | Expected | Observed | Owner |
|---|---|---|---|---|---|---|
| R3-K1 | lspSearch (TS) | blocker | `{"operation":"references","path":"packages/octocode-agents-communication/src/runtime/validation.ts","symbolName":"isUuid","line":84}` | 14 uses, including tests that reach it through `await import('../dist/runtime.js')` | 7 in 4 files, no partial reason (re-run by the coordinator; rg finds 14) | native |
| R3-G1 | graphQuery | blocker | `impact` on `packages/octocode-native/crates/engine/src/num.rs#saturating_u32`, `kind:"symbol"`, at `depth:1` and `depth:2` | the same depth-1 rows whatever depth is asked | `byDepth.1` 24 at depth 1, 42 at depth 2 (re-run by the coordinator); path calls `crate::num::f()` drop out | native |
| R3-K2 | astSearch | blocker | `{"operation":"match","path":"packages/octocode-native/crates/runtime-base/src/security/content/tests/key_blocks.rs","rule":{"pattern":"$X.expect($Y)"}}` | 3 matches | 2, no warning (re-run by the coordinator): key-block redaction swallows a closing quote before parsing | native |
| R3-L1 | localFetch | major | `{"path":"skills/octocode-scraping/scripts/tests/har-ingest.test.mjs","matchString":["AWSACCESSKEYID"]}` | 0 (rg, localSearch) | 2, both inside `[REDACTED-…]` (re-run by the coordinator) | native |
| R3-H1 | ghGetHistoryItem | major | `{"operation":"pullRequest","owner":"BurntSushi","repo":"ripgrep","number":1017,"sections":["patches"]}` | a read for every file whose patch GitHub withholds | one read for 12 such files, all marked unreachable (`terminalLimit`) | native |
| R3-C1 | clasify | major | `evaluate`, source localFetch `packages/octocode-config/contract/contract-fixtures.json` (170 KB), `maxRequests:1` | a price and `next.confirm` | 1 request spent, then a circular hint and no confirm | native |
| R3-GR1 | ghSearchCode | major | `{"owner":"pallets","repo":"flask","ref":"2.0.0","keywords":["before_first_request"]}` | the 3 files that match at 2.0.0, or a lead to them | 1 file; the warning implies the others would appear | core + native |

## Not checked

- Skills, the benchmark comparison, security as a full area, Windows and Linux.
- The MCP surface on this binary (checked on the round-2 build).
- clasify `next.nextCandidatePage` and `next.unfilteredUnknown` (they need more than 131,072 chars of hits, which means several billed requests).
