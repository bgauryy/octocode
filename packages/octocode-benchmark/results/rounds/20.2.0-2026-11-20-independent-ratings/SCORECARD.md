# Scorecard: Octocode `20.2.0`, `2026-11-20`

Eight independent rater agents scored all 17 tools on the
[12-dimension rubric](../../../RATINGS.md#agent-experience-rubric-12-dimensions), each checking every answer
against a source outside Octocode. Rater scores average **8.0/10** (previous round 7.8) and **9.4/10** for
correctness, but four high findings fail the correctness gate, so three tools are capped at 4 and core tools are
**Not ready**. Rating only; the fixes run in a separate round. All 58 findings are frozen in
[FINDINGS.md](FINDINGS.md).

| Round | Value |
|---|---|
| Version | `octocode 20.2.0+69024e9f` (contract fingerprint `69024e9f…`, generated from local octocode-core 21.2.0, which differs from the published 21.2.0) |
| Binary | `packages/octocode-native/npm/darwin-arm64/octocode`, SHA-256 `c6e1e3ab16908a54fd71dc0b13c31d5a3a2929d966d6fcdf90f197a8250910cd`, a dev build of 2026-11-20 02:49. The same binary for every lane. |
| Source | Dirty tree with uncommitted fix-round work; HEAD `67df1d964` when recorded |
| Previous round compared | The 2026-11-19 independent round (same lanes, Overall and Correct only, not stored as a round). A reference, never a carried score. |
| Surfaces | Raters: CLI (`octocode <tool> '{"queries":[…]}' --json`). MCP: the tool checks and a default-surface run over `octocode mcp` stdio, on the same binary (see [Release gates](#release-gates)). |
| Agents and models | 8 rater lanes, one fresh Claude Opus 5.5 agent each, briefed with [independent raters](../../../briefs/independent-raters.md); a Claude Opus 5.5 coordinator merged the lanes and spot-checked the high findings |
| Checked by / signed off by | Rater lanes / coordinator. Neither fixed anything during the round. |
| clasify requests billed | 33: the clasify lane's budget was 32 (20, extended by 12); one more request was refused by the provider, billing unknown. The end-to-end lane billed 0. |
| Conditions | 2026-11-20, 10:22–10:55 local; lanes took 13–31 minutes. Other sessions edited the repository and shared a warm rust-analyzer: timings are indicative. macOS arm64 only. |

## How it was measured

1. **Pinned build and guidance.** One binary for every lane (hash above). The `octocode mcp` initialize
   instructions (4,018 characters with the opt-in tools on) were saved to a file for the end-to-end lane.
2. **Eight lanes, one fresh agent each:** packages, clasify, local, code intelligence, GitHub read, GitHub history,
   graph, and end to end. Each lane rated its tools on fresh research questions of its own choosing, across several
   public repositories and ecosystems.
3. **Ground truth outside Octocode for every claim:** `rg`, `find`, `git`, `sed -n`, `cmp`, `gh api` (GET only),
   registry APIs and api.osv.dev, plus a fixture with known graph edges and 57 hand-labelled clasify judgments. A
   claim counted as correct only after the rater checked it.
4. **Required per tool:** a cold and a warm call, at least one stress test, a batch of up to 5 queries, and a walk
   of every page of at least one paged answer.
5. **Scores:** each of the 12 dimensions on 1–10, plus Overall (the rater's judgment) and Correct (verified claims
   right). Each score of 7 or below carries a reason and evidence.
6. **Regression check last:** after rating, each lane re-tested the previous round's issues for its tools.
7. **Coordinator merge:** one table, per-dimension averages, the correctness gate from [RATINGS.md](../../../RATINGS.md),
   spot checks of the high findings, and findings recorded with ids.
8. **MCP coverage on the same binary:** the tool checks (`tool-checks/run.mjs`, since removed from the package) over MCP stdio with the
   opt-in tools on, and a default-surface run with an empty `OCTOCODE_HOME`.

## Verdict

| Area | Verdict | Blocking items |
|---|---|---|
| Core tools | **Not ready** | K1, K2 (lspSearch), P1 (securityCheck), GR1 (ghStructure); C1 (clasify spends without consent) is major |
| Security | not checked as an area (partial evidence below) | L1 |
| Skills | not checked | — |
| Release gates | Ready on this binary | — |
| Benchmark | not run | — |
| **Overall** | **Not ready** | K1, K2, P1, GR1 |

## Core tools

Rater scores on the [12 dimensions](../../../RATINGS.md#agent-experience-rubric-12-dimensions). "Gated" applies the
correctness gate: an unresolved wrong answer, silent evidence loss or unreachable required page caps the tool at 4.

| Tool | Desc | Schema | Workflow | Clarity | Time | Tokens | Paging | Smart | Short | Long | Stress | Hints | Overall | Gated | Correct | Previous |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| localFetch | 9 | 9 | 9 | 9 | 9 | 8 | 10 | 8 | 9 | 9 | 9 | 9 | **9** | 9 | 10 | 9 |
| astSearch | 9 | 8 | 8 | 8 | 9 | 8 | 8 | 8 | 9 | 9 | 8 | 8 | **9** | 9 | 9 | 8 |
| ghSearchHistory | 8 | 8 | 9 | 8 | 8 | 9 | 9 | 8 | 8 | 9 | 9 | 8 | **9** | 9 | 10 | 8 |
| structureSearch | 9 | 8 | 8 | 9 | 9 | 7 | 10 | 7 | 9 | 9 | 8 | 8 | **8.5** | 8.5 | 10 | 9 |
| localSearch | 8 | 8 | 8 | 8 | 7 | 8 | 10 | 9 | 9 | 9 | 8 | 7 | **8.5** | 8.5 | 9 | 8 |
| ghSearchCode | 9 | 8 | 9 | 8 | 7 | 8 | 9 | 8 | 8 | 9 | 8 | 8 | **8** | 8 | 10 | 6 |
| ghSearchRepo | 9 | 8 | 9 | 8 | 8 | 9 | 9 | 8 | 9 | 9 | 8 | 8 | **8** | 8 | 10 | 8 |
| ghGetFileContent | 8 | 9 | 8 | 8 | 9 | 7 | 9 | 7 | 9 | 8 | 9 | 7 | **8** | 8 | 10 | 9 |
| ghCloneRepo | 7 | 8 | 7 | 8 | 9 | 10 | – | 8 | 9 | 9 | 8 | 8 | **8** | 8 | 10 | 7 |
| ghGetHistoryItem | 8 | 8 | 9 | 8 | 8 | 9 | 9 | 9 | 9 | 9 | 9 | 7 | **8** | 8 | 9 | 8 |
| artifactSearch | 8 | 9 | 8 | 8 | 9 | 9 | 9 | 7 | 9 | 9 | 8 | 7 | **8** | 8 | 9 | 8 |
| securityCheck | 8 | 9 | 9 | 7 | 7 | 6 | 8 | 9 | 9 | 8 | 7 | 7 | **8** | **4** (P1) | 9 | 8 |
| graphBuild | 8 | 8 | 8 | 8 | 6 | 9 | 8 | 8 | 9 | 6 | 8 | 8 | **8** | 8 | 9 | 7 |
| ghStructure | 9 | 8 | 8 | 8 | 6 | 9 | 7 | 8 | 9 | 8 | 5 | 7 | **7** | **4** (GR1) | 9 | 8 |
| lspSearch | 8 | 8 | 7 | 7 | 7 | 7 | 9 | 7 | 8 | 8 | 7 | 6 | **7** | **4** (K1, K2) | 8 | 7 |
| clasify | 7 | 7 | 9 | 8 | 9 | 9 | 8 | 5 | 6 | 9 | 6 | 8 | **7** | 7 | 9 | 7 |
| graphQuery | 7 | 9 | 7 | 6 | 8 | 4 | 8 | 8 | 9 | 6 | 7 | 7 | **7** | 7 | 9 | 7 |
| **Average** | 8.2 | 8.2 | 8.2 | 7.9 | 7.9 | 8.0 | 8.8 | 7.8 | 8.6 | 8.4 | 7.8 | 7.5 | **8.0** | 7.4 | 9.4 | 7.8 |

ghCloneRepo has no paging of its own (–). Pagination is the strongest dimension: every walked list rebuilt its
evidence exactly once (for example, 63 byte pages rebuilt a 1,040,682-byte one-line JSON identical under `cmp`).
Hints are the weakest. The lowest single cells are graphQuery tokens (4: `impact` returns 433 KB for 51 rows),
clasify smart (5: an uncapped call billed 19 requests) and ghStructure stress (5: a huge-repo walk retries
forever).

Of the previous round's issues the lanes re-tested, 12 are fixed, 7 partly fixed and 1 not fixed (relative
dates in structureSearch); graphQuery `impact` size got worse. Details per lane are in [FINDINGS.md](FINDINGS.md).

## Workflows

How the tools hand off to each other, as the raters observed:

| Flow | Chain |
|---|---|
| Local | structureSearch → localSearch → localFetch → lspSearch; astSearch → localFetch / lspSearch |
| Remote repository | ghSearchRepo → ghStructure → ghSearchCode → ghGetFileContent |
| History | ghGetFileContent → ghSearchHistory → ghGetHistoryItem → ghGetFileContent |
| Package | artifactSearch → securityCheck, or → ghStructure / ghGetFileContent at the release ref |
| Large content | any search or read tool → clasify → localFetch / ghGetFileContent at the cited lines |
| Graph | structureSearch / ghCloneRepo → graphBuild → graphQuery |

Missing handoffs: graphQuery never offers lspSearch although its description says to confirm there (G4);
ghCloneRepo offers no local-tool call after a clone (H1); ghSearchRepo `get` offers nothing (F7).

End-to-end questions, answered with only the served instructions and `--help`/`schema`:

| Workflow | Calls this round | Calls last round | Answer right? | Note |
|---|---|---|---|---|
| Local: how a page is cut and a cursor resumes | 10 (+30 paging-test pages), 68K chars, 0.6 s | new question | yes, every citation | 8/10: no leads emitted; navigated by `view:"symbols"` outlines |
| Local: what makes localSearch skip a file | 15, 84K chars, 5.0 s | 14, 69K chars | yes (one count unverified) | 8/10 (was 7): the misleading `walk_builder` lead is gone |
| Public: tokio `sleep` timer and the release that last changed the wheel | 14, 43K chars, 15.2 s | 8, 22K chars (different question) | yes (tokio 1.54.0) | 9/10 (was 8): the compare `status:"empty"` is undocumented |
| Guidance (instructions and `--help`) | – | – | routing right in all 3 | 8/10: `responseChars` help reads backwards (F1); `testCode` hard to find (F4) |

## Security

Not checked as an area. Partial evidence from the lanes:

| Check | Result | Evidence |
|---|---|---|
| Partial redaction | Fail (misleading) | L1: a match inside a masked span gives a false empty row; mask placeholders are matchable |
| Credential files withheld | Pass | `credentials/` folders no longer withheld (issue #477); the path policy withheld the 2 expected entries in the local flow |
| Provider egress (billing) | Fail | C1: an uncapped classified search billed 19 requests without a confirm step |
| Tokens and refusals | Pass | No key printed; `.env` refused by the path policy on GitHub reads |

## Skills

Not checked.

## Release gates

On the same binary, 2026-11-20:

| Gate | Command | Exit | Note |
|---|---|---|---|
| Full repo verify | `node skills-dev/octocode-dev/scripts/dev.mjs verify` | 0 | All 10 packages' `verify:own`, after fixing 6 lint violations committed on 2026-11-19; native 478/478 process tests |
| Tests without native | `… test:ci` | 0 | 97 s |
| Build outputs | `… check-outputs` | 0 | Native version and contract match |
| Publish check | `… prepublish --dry-run` | 0 | Would remove the 7 local resolutions, as expected |
| Tool checks over MCP, opt-ins on | `node packages/octocode-benchmark/tool-checks/run.mjs` | 0 | 17/17 tools, 81/81 steps, 0 undescribed schema fields |
| Default surface over MCP | empty `OCTOCODE_HOME`, every step of the 12 default tools' check files | – | 12 tools served; 57/57 applicable steps; the one securityCheck follow-up is correctly not offered when securityCheck is off; both launchers (`octocode-mcp`, `octocode`) start this build |

## Benchmark

Not run this round.

## Defects

All 58 findings, with exact inputs, evidence, severity and owner, are frozen in [FINDINGS.md](FINDINGS.md). The
high ones:

| ID | Area and tool | Severity | Repro call (exact, on the pinned binary) | Expected | Observed | Owner |
|---|---|---|---|---|---|---|
| K1 | lspSearch | blocker | `{"queries":[{"operation":"references","path":"test-utils/fixtures/demo-repo/advanced-local/rust/src/lib.rs","symbolName":"target","line":1}]}` | the 3 uses in `#[cfg(feature)]`-gated modules, or a partial reason naming them | 3 rows with `coverage.state:"complete"` | native |
| K2 | lspSearch | blocker | `{"queries":[{"operation":"references","path":"packages/octocode-agents-communication/src/runtime/validation.ts","symbolName":"asObject","line":30}]}` | 51 references | 45, with no partial reason | native |
| P1 | securityCheck | blocker | `{"queries":[{"operation":"package","ecosystem":"pypi","packageName":"flask","version":"0.12"}]}` during a GitHub read failure | "GitHub's advisory database was not read for it" | "matches none", beside `sources[github]` `partial / transport` (cause at `tool-packages/src/security_check/package.rs:280`) | native |
| GR1 | ghStructure | blocker | `{"queries":[{"operation":"tree","owner":"torvalds","repo":"linux","include":["**/sched/fair.c"]}]}` | a continuation that reads the 1,560 unread folders | `next.retry` with the same cursor and result, forever | native |
| C1 | clasify | major | `{"queries":[{"operation":"evaluate","source":{"tool":"localSearch","query":{"path":".","pattern":"retry"}},"questions":{"q":{"type":"noul","instructions":"The text mentions a restart."}}}]}` | a confirm step before a multi-request plan | 19 billed requests (ceiling 30) at once | native + core |
| G1 | graphQuery | major | `{"queries":[{"operation":"impact","since":"HEAD"}]}` at the workspace root | rows first, summary lists behind `limit`/`next.*` | 433,162 characters over 23 pages for 51 rows | native + core |

Severity here follows the [template](../../../briefs/SCORECARD.md#defects): FINDINGS.md's "high" is a blocker for
K1, K2, P1 and GR1 (wrong or silently incomplete answer, unreachable remainder) and major for C1 and G1.

## Not checked

- Skills, the benchmark comparison, and security as a full area.
- Windows and Linux builds: macOS arm64 only.
- Inline `classification` on search tools and clasify's `nextCandidatePage` / `unfilteredUnknown` / `unfiltered`:
  the billed run was blocked by the session's permissions.
- Cold rust-analyzer start on the whole native workspace: another session had already warmed it.
- Maven search ranking and Debian advisory severities (no independent source used).
