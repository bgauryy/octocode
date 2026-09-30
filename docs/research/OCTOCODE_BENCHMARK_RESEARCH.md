# Octocode research: does it beat expert gh and rg?

Status: round 3 measured (2026-09-30). Round 4 defect fixes are in progress; this doc will be updated after the next full run.
Every number comes from a real run. The runnable suite is [`octocode-local-testing/bench/`](../../octocode-local-testing/bench/BENCHMARK.md), with the result tables in `bench/results/` and the full validation reports in `bench/validate/*/REPORT.md` (`github`, `local`, `features`, `orangu`, `release`). The concept these results test is in [OCTOCODE_PROTOCOL.md](../OCTOCODE_PROTOCOL.md). The test repos are not committed; [repos/README.md](../../octocode-local-testing/repos/README.md) lists each one at its pinned commit.

## Questions (hypotheses) and verdicts
| # | Hypothesis | Verdict | Evidence |
|---|---|---|---|
| H1 | On large PRs, Octocode needs less context than an expert `gh` user | **Supported** | 9.6k vs 45k chars over 4 PRs, all correct |
| H2 | The naive step-by-step PR flow is competitive | **Rejected** | 88k vs 45k. The fix is steering agents to the filtered query (done in round 3) |
| H3 | On local repos, Octocode matches rg/sed accuracy with less context | **Partly** | Same accuracy (14/15). 113k vs 105k chars; the gap is the citable per-line gutter |
| H4 | clasify improves accuracy on behavioral ("how/where") questions | **Supported** | Whole-file locate: 10/10 vs rg 9/10, with 40% fewer files opened (18 vs 30) |
| H5 | clasify should be used everywhere | **Rejected** | PR scouting costs 159k and 79 s vs 9.6k and 8.3 s. Rule: use it only for a behavioral target across ≥2 files |
| H6 | Octocode degrades cleanly without a clasify key | **Supported** | 12 tools, 0 clasify mentions, no dangling next steps |
| H7 | clasify's provider payload is complete, lean and safe; its cache is correct | **Supported** | 14 captured calls: no leaks. The cache keys on the full state and never caches errors |
| H8 | Octocode is safer than raw tools on hostile inputs | **Supported** | Leaked 0 of 5 fake secrets vs 3 of 3 for rg. Blocks escapes and stale rewrites. A 3 MB file costs 18k chars vs 3.1M |
| H9 | GitHub tools give signals `gh` doesn't | **Supported** | Rename recovery, the compare cap, patch flags on 656-file PRs, SHA pinning, a rate-limit breaker |
| H10 | Documented features behave as documented | **Mostly** | 167 of 217 claims verified, 35 partial, 14 fail. 45 doc drifts were fixed |

## What we did (chronological)
1. **Minimal responses by default** (`debug:true` adds metadata). Responses are 16% smaller with no data lost; a contract guard restores required fields.
2. **Native flow refactor:**
   - `followUp` continuations: the brief is inherited, and 0 of 24 continuations repeat it.
   - A shared `defaultExcludes`.
   - Per-function import liveness.
   - A shared walk-thread budget: peak threads went from 72 to 22.
3. **clasify:**
   - scout on every list tool, plus a `sufficient` preset;
   - the wire payload was audited through a proxy;
   - in-flight dedupe;
   - reads anchor on the densest line;
   - window coverage across all hit clusters;
   - windows are merged, taking provider pages from 162 to 134.
4. **Large PRs:**
   - compact file rows;
   - `fileFilter` and `matchContext`;
   - `matchString` returns matching hunks only;
   - verbatim patch rendering;
   - the 3000-file cap is now honest;
   - rename origin is kept;
   - patchless files are flagged;
   - batches load in parallel;
   - large PRs get the filtered-query next step.
5. **Local:**
   - hit ranking inside a file (declaration > deciding statement > code > comment);
   - identifier searches rank the declaring file first;
   - lean `279:` gutter;
   - compact rows;
   - no single-row wrapper.
6. **Instructions:** rewritten to ≤ 2,000 chars (the host was truncating 48%); clasify use/skip rule; PR steering.
7. **Validation:**
   - 5 audits: GitHub vs gh, local vs rg/ast-grep, the feature claims, orangu over 91 real sessions, and release readiness;
   - the regression harness was modernized;
   - docs were fixed.

## Method
- **Suite:** `bench.mjs --suite pr|local|all --label <name>`, documented in BENCHMARK.md.
- **Fairness:** every arm answers the same question and is graded against the same ground truth. `gh` and the shell get expert flags; octocode gets the equivalent expert query.
- **Measures:** chars are what the agent reads (≈ tokens = chars/4), counting every call and continuation. Timing is wall ms per task.
- **Ground truth** was verified independently: `git grep`, ast-grep, reading code for every reference hit, `gh api`, and a blobless clone for compare counts.

## Results: benchmark suite (bench/results/bench-round3.md)
**PRs.** Four PRs: TypeScript#51387 (656 files), rust#157558 (429 files), TypeScript#61986 (22k-line patch), tokio#8156 (37 files).

| Arm | Correct | Baseline chars | Round 3 chars | Time |
|---|--:|--:|--:|--:|
| gh typical | 4/4 | 15.6M | 15.6M | 16 s |
| gh expert | 4/4 | 45k | 45k | 11 s |
| octocode step-by-step | 4/4 | 247k | 88k | 22 s |
| **octocode filtered query** | 4/4 | — | **9.6k** | 8.3 s |
| octocode + clasify scout | 4/4 | 305k | 159k | 79 s |

Per PR, gh expert vs octocode filtered:
- TS#51387: 3.8k vs **0.98k**
- TS#61986: 40k vs **6.0k**
- rust#157558: **1.24k** vs 1.55k
- tokio#8156: **0.73k** vs 1.05k

The two small losses are evidence octocode keeps on purpose: `@@` line numbers, the head SHA, and patch size.

**Local.** 5 repos (TS, Rust, Go, Python, Java) × 3 tasks.

| Arm | Correct | Baseline chars | Round 3 chars | Time |
|---|--:|--:|--:|--:|
| rg / sed | 14/15 | 105k | 105k | 0.9 s |
| octocode | 13/15 → **14/15** | 139k | **113k** | 2.3 s |
| octocode + clasify (chunks) | 3/15 → **12/15** | 108k | 109k | 11 s |
| octocode + clasify (whole-file locate) | **10/10** (how/unknown tasks) | 112k | **97k** | 9.3 s |

## Results: deep validation vs best practice
### GitHub tools vs expert gh (validate/github/REPORT.md, 31 tasks)
| | Chars | Calls | Time | Fully correct |
|---|--:|--:|--:|--:|
| gh expert | 106k | 34 | 34 s | 28/31 |
| octocode | 270k | 52 | 71 s | **30/31** |

**Ratings:**

| Tool | Rating |
|---|--:|
| ghSearchCode | 8 |
| ghGetFileContent | 8 |
| ghSearchHistory | 8 |
| artifactSearch | 8 |
| ghSearchRepo | 7 |
| ghStructure | 6 |
| ghGetHistoryItem | 6 |

**Octocode wins:**
- recovering renamed repos (gh returns 0 silently);
- the compare cap (gh reported 300 as complete; the truth is 339);
- 656-file PRs (`gh pr view` caps at 100 silently, and `gh pr diff` fails with 406);
- SHA-pinned reads;
- the rate-limit breaker;
- binary detection;
- typed exit codes.

**Octocode loses:**
- 2.6× the chars overall;
- a full compare takes 8 calls vs 1, and a full tree 10 vs 1 (both being fixed in round 4);
- latency: 1.06 s vs 0.65 s median.

### Local tools vs expert rg / sed / ast-grep / ctags (validate/local/REPORT.md, 8 repos)
| Tool | Result | Rating |
|---|---|--:|
| lspSearch | Exact references; rg precision is only 0.26–0.62 | 8 (with a server) |
| localSearch | 17/17 exact; 1.28× rg's chars | 7 |
| localFetch | 22/22 byte-exact; 1.2× the chars | 7 |
| astRewrite | 5/5 identical to ast-grep, where sed was wrong on 4/5; has a stale-edit guard | 7 |
| astSearch | Symbols 6/6 exact (regex/ctags 0.81–0.98); match strictness bug | 6 |
| structureSearch | Exact; 1.9× the chars; page cap of 100 | 6 |
| astTopology | Correct at the repo root; false-complete from a subdirectory (being fixed) | 5 |

**Hostile inputs:**
- Secrets: 0 of 5 leaked vs 3 of 3 for rg.
- A 3 MB file costs 18k chars vs 3.1M.
- A 2 MB minified line costs 710 chars vs 2.07M for naive rg.
- `.env`, symlink escapes and `..` escapes are refused.
- Stale rewrites are blocked.

### Feature claims (validate/features/REPORT.md)
- 217 claims: 167 verified, 35 partial, 14 fail, 1 untestable.
- **Every "better than raw tools" claim holds:**
  - pinned SHAs;
  - honest pagination;
  - minimal output 25–60% smaller;
  - redaction;
  - sandbox;
  - continuations;
  - batching;
  - config precedence;
  - CLI and MCP byte parity.

### Real sessions (validate/orangu/REPORT.md, 91 sessions, 1,692 octocode calls)
- **MCP instructions** were truncated by the host at about 2 KB; fixed with a ≤ 2,000 char cap and a test.
- **Recurring input errors:**
  - `structureSearch` without `operation`: fixed with a default of `tree`.
  - `regex: true`: the description now says it takes a mode, not a boolean.
- **Harness items for the owner:** the permission allowlist, duplicated skills, and the size of always-loaded context.

## Conclusions
1. **Octocode's winning pattern is "ask precisely, get exactly that, with evidence".**
   - The filtered PR query beats expert `gh` by 4.7× in total.
   - It stays correct where the obvious `gh` commands fail silently.
   - Losses appear when agents page through everything; steering fixes that.
2. **Locally, accuracy equals rg while safety and semantics are better,** with about 8% more chars. That overhead is the line-number gutter, kept on purpose because agents cite lines.
3. **clasify is a precision tool, not a default step.**
   - It wins on behavioral questions across candidate files (10/10).
   - It loses on identifiers, literals and PR filtering.
   - The instructions now encode this rule.
4. **The biggest remaining costs:**
   - multi-page GitHub reads (compare, trees, issue threads);
   - rewrite preview size;
   - CLI latency (a process per call). MCP is warm and much faster.

## Open items (round 4, in progress)
Silent-failure defects first:
- topology false-complete;
- AST match strictness;
- nested TS symbols;
- `WORKSPACE_ROOT` relative paths;
- `invertMatch`;
- `followUp` forgery;
- PR `matchString` without patches, and unsearched patchless files;
- ghStructure missing-branch fallback;
- compare pagination;
- `auth status` accepting invalid tokens.

Release blockers left for the owner: see [validate/release/REPORT.md](../../octocode-local-testing/bench/validate/release/REPORT.md) and the [release checklist](../../skills-dev/octocode-dev/docs/RELEASE.md).
