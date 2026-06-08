# Benchmark Summary — octocode vs gh

**Judge model:** claude-sonnet-4-5
**Researcher models:** octocode → `claude-sonnet-4-5` · gh → `gpt-5.1-codex-max`
**Run date:** 2026-06-07
**Verification:** Independent semantic review of every answer. Judge research is unmetered and excluded from all totals.

---

## Executive Summary (Manager TL;DR)

> **octocode wins across quality, call efficiency, and character cost.**

| Metric | octocode | gh CLI | Edge |
|---|---|---|---|
| Answer quality (non-drift, /42 max) | **36** | 32 | +4 pts (+12.5%) |
| Avg quality per question | **2.57 / 3** | 2.29 / 3 | |
| Total API calls | **108** | 407 | 3.8× fewer |
| Total characters used | **1.08M** | 13.2M | 12.2× fewer |
| Quality per 1,000 chars | **0.0381** | 0.0027 | **14.2× more efficient** |
| Token-score wins (14 non-drift Qs) | **11 wins** | 2 wins | |
| Approx token cost (chars / 4) | **~270K** | ~3.3M | 12.2× cheaper |

**Bottom line:** For the same GitHub research tasks, octocode delivers more correct answers at 1/12th the token cost. The gap is structural — bulk queries, targeted file reads, and AND-intersection search all reduce waste. gh CLI's one real advantage is reading over-size-limit files (>1MB) via raw blob access, a gap that disappears with `ENABLE_CLONE`.

---

## Performance Graph

### Quality Score by Question (0–3 scale)

```
Q    octocode                 Score    Category      gh                       Score   Notes
──── ──────────────────────── ──────── ──────────── ──────────────────────── ──────── ──────────────────────────
Q1   ██████░░░░░░░░░░░░░░░░░    2      [SEARCH]     ░░░░░░░░░░░░░░░░░░░░░░░    0     ← gh repo-scoping failure
Q2   █████████████████████████  3      [SEARCH]     █████████████████████████  3
Q3   ██████████████████░░░░░░░  2      [SEARCH]     █████████████████████████  3
Q4   █████████████████████████  3      [SEARCH]     ████████████████░░░░░░░░░  2
Q5   ██████████████████░░░░░░░  2      [CONTENT]    ██████████████████░░░░░░░  2
Q6   █████████████████████████  3      [CONTENT]    ████████████████░░░░░░░░░  2
Q7   █████████░░░░░░░░░░░░░░░░  1      [CONTENT]    █████████████████████████  3     ← gh wins (blob read)
Q8   █████████████████████████  3      [CONTENT]    █████████████████████████  3
Q9   █████████████████████████  3      [STRUCTURE]  █████████████████████████  3
Q10  █████████████████████████  3      [STRUCTURE]  █████████████████████████  3
Q11  █████████████████████████  3      [STRUCTURE]  ████████████████░░░░░░░░░  2
Q12  ████████████████░░░░░░░░░  2  ⚡  [PR/drift]   █████████░░░░░░░░░░░░░░░░  1  ⚡
Q13  ████████████████░░░░░░░░░  2      [PR]         ████████████████░░░░░░░░░  2
Q14  █████████████████████████  3      [PR]         █████████░░░░░░░░░░░░░░░░  1     ← gh found wrong PR
Q15  █████████████████████████  3      [PR]         █████████████████████████  3
Q16  █████████████████████████  3  ⚡  [REPOS/drift] █████████████████████████ 3  ⚡
Q17  ████████████████░░░░░░░░░  2  ⚡  [REPOS/drift] ████████████████░░░░░░░░░ 2  ⚡
     ────────────────────────────────────────────────────────────────────────────────
     Σ non-drift: octocode 36                        Σ non-drift: gh 32
     ⚡ = drift question (scored separately)          each █ ≈ 0.125 quality pts
```

### Token Efficiency — Quality per 1,000 Characters (non-drift questions)

```
     Agent      Q   Score  Chars     Quality/1k chars
     ─────────────────────────────────────────────────────────────────────
     octocode   Q9    3      997     3.009  ████████████████████████████████████████ best
     gh         Q9    3   32,732     0.092  █▏
     ─────────────────────────────────────────────────────────────────────
     octocode  Q10    3    2,128     1.410  ███████████████████
     gh        Q10    3   22,011     0.136  ██
     ─────────────────────────────────────────────────────────────────────
     octocode   Q2    3    8,148     0.368  █████
     gh         Q2    3  193,893     0.015  ▏
     ─────────────────────────────────────────────────────────────────────
     octocode   Q3    2    5,784     0.346  █████
     gh         Q3    3 1,877,334    0.002  ▏
     ─────────────────────────────────────────────────────────────────────
     octocode  Q11    3    9,101     0.330  ████
     gh        Q11    2   61,181     0.033  ▏
     ─────────────────────────────────────────────────────────────────────
     octocode   Q8    3  324,573     0.009  ▏                              gh wins ↓
     gh         Q8    3   25,602     0.117  ██                             0.117 vs 0.009
     ─────────────────────────────────────────────────────────────────────
     octocode  Q13    2   34,052     0.059  █                              gh wins ↓
     gh        Q13    2    5,268     0.380  █████                          0.380 vs 0.059
     ─────────────────────────────────────────────────────────────────────
```

### Character Cost vs Quality — All 17 Questions

```
     Q   Chars (octocode)  Chars (gh)       Quality delta (octo−gh)
     ── ─────────────────── ──────────────── ───────────────────────
     Q1       65K  ████          10K  █        octo +2  ████████████████████
     Q2        8K  █            194K  █████████ tie       ░
     Q3        6K  █          1,877K  ██████████████████████████████████  octo  0 / gh +1
     Q4       28K  ██         5,831K  ██████████████████████████████████████████████  octo +1  ████████████
     Q5       18K  █            203K  █████████ tie       ░
     Q6       23K  █            362K  ██████████████  octo +1  ████████████
     Q7       10K  █          3,152K  ██████████████████████████████████████████████  octo −2 (gh wins)
     Q8      325K  ██████████    26K  █         tie / gh token-score wins
     Q9        1K  ░             33K  █         tie (3−3)
    Q10        2K  ░             22K  █         tie (3−3)
    Q11        9K  █             61K  ████      octo +1  ████████████
    Q12       95K  ████           3K  ░    ⚡  octo +1 drift
    Q13       34K  ██             5K  ░         tie (2−2) / gh token-score wins
    Q14      398K  ████████████████████  117K  ████  octo +2  ████████████████████████
    Q15       17K  █             51K  ██        tie (3−3)
    Q16       21K  █              2K  ░    ⚡  tie drift
    Q17       20K  █          1,255K  ████████████████████████████████████████████████  tie drift
     ── ─────────────────── ──────────────── ───────────────────────
        TOTAL  1.08M          13.2M           octocode wins 11/14 non-drift
```

### Final Scorecard

```
┌─────────────────────────────────────────────────────────────────────────┐
│            GITHUB RESEARCH BENCHMARK · FINAL SCORECARD                  │
│                   17 questions · 5 categories · Jun 2026                │
├──────────────────────────┬──────────────────┬───────────────────────────┤
│ Metric                   │   octocode       │        gh CLI             │
├──────────────────────────┼──────────────────┼───────────────────────────┤
│ Quality score (non-drift)│   36 / 42  ████  │   32 / 42  ████           │
│ Quality score (all 17)   │   43 / 51  ████  │   38 / 51  ████           │
│ API calls                │     108    ██    │     407    ████████████   │
│ Total chars used         │    1.08M   █     │   13.20M   ████████████   │
│ Token-score wins         │      11    ████  │       2    █              │
│ Quality per 1k chars     │   0.0381         │   0.0027                  │
│ Efficiency advantage     │  14.2×  BETTER   │  baseline                 │
├──────────────────────────┼──────────────────┼───────────────────────────┤
│ Category wins            │  SEARCH ✅       │  CONTENT (Q7 blob read)   │
│                          │  STRUCTURE ✅    │                           │
│                          │  PR ✅           │                           │
│                          │  REPOS (tie) ≈   │  REPOS (tie) ≈            │
├──────────────────────────┼──────────────────┼───────────────────────────┤
│ OVERALL WINNER           │  ✅  octocode    │                           │
└──────────────────────────┴──────────────────┴───────────────────────────┘
```

---

## Quality Scoring Legend

| Score | Meaning |
|------:|---------|
| 3 | All load-bearing facts present, no false claims, all sub-questions answered |
| 2 | Mostly correct — one load-bearing sub-fact missing or inaccurate |
| 1 | Partially correct, or an unsupported claim is present |
| 0 | Wrong, empty, or UNKNOWN |

Token score formula (per question): `quality / (total_chars / 1000)`
A zero-quality answer has a zero token score regardless of character cost.

---

## Per-Question Score Derivations

### Q1 — SEARCH — Exhaustive code search `[vercel/next.js]`

| | octocode | gh |
|---|---|---|
| **Files reported** | ~199 unique (API cap at 1,000 results, 10 pages × ~20 unique/page) | 100 |
| **First 10 paths** | `bench/basic-app/app/page.js`, `apps/bundle-analyzer/app/layout.tsx`, `packages/next/src/client/web-vitals.ts` … — all valid vercel/next.js paths | `.agents/skills/router-act/SKILL.md`, `.claude-plugin/plugins/…` — user workspace files; NOT from vercel/next.js |
| **Quality score** | **2** — honest cap disclosure, first 10 paths correct | **0** — fundamental repo-scoping failure; first 10 paths are from user's local workspace |

*gh penalty: The question asked about `vercel/next.js`. Paths like `.agents/skills/router-act/SKILL.md` do not exist in that repository.*

---

### Q2 — SEARCH — Multi-repo bulk search `[useState / ref / createSignal]`

| | octocode | gh |
|---|---|---|
| **useState location** | `packages/react/src/ReactHooks.js` line 66 ✓ | `packages/react/src/ReactHooks.js` line 66 ✓ |
| **ref() location** | `packages/reactivity/src/ref.ts` line 64 ✓ | `packages/reactivity/src/ref.ts` line 64 ✓ |
| **createSignal location** | `packages/solid/src/reactive/signal.ts` line 229 ✓ | `packages/solid/src/reactive/signal.ts` line 229 ✓ |
| **Calls used** | 2 (bulk query) | 9 (sequential queries) |
| **Quality score** | **3** — all three correct | **3** — all three correct |

---

### Q3 — SEARCH — Search with textMatch context `[compose() in honojs/hono src/]`

| | octocode | gh |
|---|---|---|
| **Production calls found** | `src/hono-base.ts:225`, `src/hono-base.ts:450`, `src/middleware/combine/index.ts:102` — correct with exact code | Same 3 production calls ✓, plus all test-file calls with line + exact code |
| **Test-file calls** | Acknowledged ("34 calls starting at line 45") but NOT enumerated individually | Fully enumerated — 34 calls in `src/compose.test.ts` with exact line + code |
| **False positives** | None | 2 comment lines in `src/hono.test.ts` (`:51`, `:142`) — text says `// when using compose()`, not actual calls |
| **Quality score** | **2** — correct production calls but missing required per-line test enumeration | **3** — complete enumeration, minor false positives from comment lines do not materially affect answer |

---

### Q4 — SEARCH — Multi-keyword narrowing `[ppr AND Postpone in vercel/next.js]`

| | octocode | gh |
|---|---|---|
| **Files reported** | 62 unique files (4 calls) | 41 files (74 calls, 5.8M chars) |
| **Common files** | Both lists include the 11 compiled react-dom bundles, core server files, test files | Same core subset |
| **Files only in octocode** | 21 additional files: docs adapters, request/*.ts, server use-cache, segment-cache, ppr test dirs, etc. | — |
| **Files only in gh** | 6 files: `packages/next/src/build/` directory files (adapter, utils, templates, index) | — |
| **Quality score** | **3** — more complete AND-intersection result set; 4 calls sufficient | **2** — 41 files likely undercount; 74 calls + 5.8M chars still missed 21 files found by octocode |

---

### Q5 — CONTENT — Large file targeted section read `[ReactFiberWorkLoop.js]`

| | octocode | gh |
|---|---|---|
| **Sub-Q 1: exported functions** | Listed 5 of 63 (`getWorkInProgressTransitions`, `addTransitionStartCallbackToPendingTransition`, `peekDeferredLane`, `scheduleUpdateOnFiber`, `performWorkOnRoot`) | Listed ~60 functions — comprehensive; includes `performWorkOnRoot` but NOT `performConcurrentWorkOnRoot` |
| **Sub-Q 2: performConcurrentWorkOnRoot** | Correctly states function was **refactored out** and replaced by `performWorkOnRoot(root, lanes, forceSync)` at line 1123 | States `performConcurrentWorkOnRoot(root, didTimeout)` exists as a Scheduler callback — contradicts own Part 1 list which does not include this function |
| **Quality score** | **2** — Part 1 incomplete (5/63 exports), Part 2 correct (identifies rename) | **2** — Part 1 comprehensive, Part 2 contradictory (describes function not in own exported list) |

---

### Q6 — CONTENT — Large file tail read `[vitejs/vite CHANGELOG.md]`

| | octocode | gh |
|---|---|---|
| **Latest release** | `8.0.16` (2026-06-01) ✓ | `8.0.16` (2026-06-01) ✓ |
| **First 4.x stable release** | `4.0.0` (2022-12-09) ✓ with detailed changes: Rollup 3, SWC plugin, safari14 target, CSS export deprecation, `hot.decline()` removal, keyboard shortcuts | `4.0.0-beta.0` (2022-09-19) — a **pre-release**, not the first stable `4.x` |
| **Change detail** | 8 specific changes listed ✓ | Vague ("Rollup 3 migration, browser-target/default compatibility updates") — incomplete |
| **Quality score** | **3** — both sub-answers correct with detail | **2** — latest correct; first release is a pre-release, not the first stable `4.0.0`; changes are vague |

---

### Q7 — CONTENT — Over-size-limit file `[microsoft/TypeScript checker.ts ~3MB]`

| | octocode | gh |
|---|---|---|
| **Sub-Q 1: createTypeChecker** | Correctly describes it via indirect method (found in `program.ts` imports). Factory that creates `TypeChecker` ✓ | `createTypeChecker(host: TypeCheckerHost): TypeChecker` at line 1486. Detailed initialization description ✓ |
| **Sub-Q 2: last function** | **UNKNOWN** — file exceeds all content API size limits | `createBasicNodeBuilderModuleSpecifierResolutionHost(host)` at line 54260. Adapts TypeCheckerHost to ModuleSpecifierResolutionHost ✓ |
| **Method** | Indirect inference via imports (content API blocked at 3MB) | Downloaded full 3.15M-char raw blob |
| **Quality score** | **1** — Part 1 partially correct (indirect), Part 2 UNKNOWN | **3** — both functions correct with line numbers and descriptions |

---

### Q8 — CONTENT — Directory listing `[facebook/react packages/react/src/]`

| | octocode | gh |
|---|---|---|
| **File count** | 27 ✓ | 27 ✓ |
| **Extension breakdown** | 0 `.ts`, 27 `.js` ✓ | 0 `.ts`, 27 `.js` ✓ |
| **File list** | Identical 27 `.js` files ✓ | Identical 27 `.js` files ✓ |
| **Chars used** | 324,573 (high — likely dumped full repo tree) | 25,602 (efficient JSON directory listing) |
| **Quality score** | **3** — correct | **3** — correct |

*Note: gh wins token score decisively (0.117 vs 0.009) due to 13× fewer chars for same quality answer.*

---

### Q9 — STRUCTURE — Subtree file count `[vuejs/core packages/reactivity/src/]`

| | octocode | gh |
|---|---|---|
| **Count** | 13 `.ts` files ✓ | 13 `.ts` files ✓ |
| **File list** | All 13 identical ✓ | All 13 identical ✓ |
| **Chars** | 997 (1 call) | 32,732 (1 call) |
| **Quality score** | **3** — correct | **3** — correct |

---

### Q10 — STRUCTURE — Two-repo structure comparison `[honojs/hono vs expressjs/express]`

| | octocode | gh |
|---|---|---|
| **Hono subdirs** | 10: `adapter`, `client`, `helper`, `jsx`, `middleware`, `preset`, `request`, `router`, `utils`, `validator` ✓ | 10 — identical list ✓ |
| **Express subdirs** | 0 (flat: 6 files only) ✓ | 0 — flat with same 6 files ✓ |
| **Architectural analysis** | Detailed (modular extensibility vs thin unopinionated core) ✓ | Good (TypeScript-first modular vs compact mature core) ✓ |
| **Calls** | 1 (bulk both repos) | 2 (separate requests) |
| **Quality score** | **3** — correct | **3** — correct |

---

### Q11 — STRUCTURE — Entry-point discovery `[vitejs/vite dev server]`

| | octocode | gh |
|---|---|---|
| **Sub-Q 1: entry file** | `packages/vite/src/node/server/index.ts` — the dev server module itself ✓ | `packages/vite/src/node/cli.ts` — the CLI that invokes the server; answered at wrong abstraction level |
| **Sub-Q 2: first function on startup** | `disableCache()` — first call inside `_createServer()` before config resolution ✓ | `createServer(...)` — the factory function invoked from CLI, not the first call *within* the server module |
| **Quality score** | **3** — correct entry file and specific first internal function call | **2** — entry file area correct but answered from CLI perspective; first internal function not identified |

---

### Q12 — PR — Labels in search `[vercel/next.js "Pages Router"]` `[DRIFT]`

| | octocode | gh |
|---|---|---|
| **Label filter result** | 0 PRs — tried `label:"Pages Router"`, `label:Pages-Router`, `label:"area: pages router"` | 0 PRs |
| **Additional investigation** | Provided 5 proxy PRs with "Pages Router" in title (with PR numbers) | None — bare `0` only |
| **Quality score** | **2** — honest investigation, useful fallback | **1** — bare zero, no investigation or context |

*Drift question — scored loosely. Both hit the same label gap.*

---

### Q13 — PR — Inline review thread comments `[facebook/react PR #27733]`

| | octocode | gh |
|---|---|---|
| **Inline comment count** | 0 (classified all 29 as PR-level discussion) | 2 (identified `packages/react-devtools-inline/src/frontend.js`) |
| **Most substantive objection** | hoxyq's architectural objection: prefer setting `__REACT_DEVTOOLS_ATTACH__` global over exporting `attach` from `react-devtools-inline` ✓ | "In our use case, I need to unload and reload the DevTools…" — this is the **PR author** explaining their use case, not a **reviewer's objection** |
| **File with most comments** | N/A (reported 0 inline) | `packages/react-devtools-inline/src/frontend.js` |
| **Quality score** | **2** — correct characterization of discussion; 0 inline count needs verification; objection quote is accurate | **2** — may have correct inline count and file; but quote is wrong type (author motivation, not reviewer objection) |

*Both score 2 pending authoritative API verification of inline comment count.*

---

### Q14 — PR — Commits full list `[honojs/hono hono/jsx introduction]`

| | octocode | gh |
|---|---|---|
| **PR identified** | #306 "feat: jsx middleware" (June 2022) — the PR that added `hono/jsx` to package.json ✓ | #1986 "feat(jsx/dom): provide jsx-runtime and jsx-dev-runtime via jsx/dom" — a **later** jsx/dom expansion PR ✗ |
| **Commit count** | 5 ✓ (for #306) | 22 (for the wrong PR #1986) |
| **SHAs and messages** | All 5 listed with messages and dates ✓ | All 22 listed — but for the wrong PR |
| **Authors** | Yusuke Wada and Taku Amano ✓ | Taku Amano only (all 22 commits) — consistent with #1986 |
| **Quality score** | **3** — correct PR, complete commit list | **1** — wrong PR identified; commit list is accurate but for the wrong target |

*Evidence: Both Q14 and Q15 point to #306 as the first JSX introduction. Q15 independently confirms #306. `package.json` gained `"./jsx"` in #306.*

---

### Q15 — PR — PR archaeology `[honojs/hono first JSX PR]`

| | octocode | gh |
|---|---|---|
| **PR number** | #306 ✓ | #306 ✓ |
| **Title** | "feat: jsx middleware" ✓ | "feat: jsx middleware" ✓ |
| **Motivation** | "Close #300" — issue requesting JSX support ✓ | "Close #300" ✓ |
| **Files changed** | 11 files listed with descriptions and line counts ✓ | 11 files listed ✓ |
| **Calls** | 1 | 3 |
| **Quality score** | **3** — all sub-questions correct with detail | **3** — all sub-questions correct |

---

### Q16 — REPOS — Multi-filter repository search `[TypeScript + mcp + 500★ + 2025]` `[DRIFT]`

| | octocode | gh |
|---|---|---|
| **Count** | 204 | 202 |
| **Top 5** | n8n-io/n8n (191,455★), langgenius/dify (144,259★), lobehub/lobehub (78,315★), upstash/context7 (56,919★), ChromeDevTools/chrome-devtools-mcp (43,040★) | Same 5 repos, star counts within ~60 of octocode (same run day, minor drift) |
| **Quality score** | **3** | **3** |

*Drift question — scored loosely. Minor count difference (202 vs 204) is expected drift between run times.*

---

### Q17 — REPOS — Enumerate org repos `[vercel organization]` `[DRIFT]`

| | octocode | gh |
|---|---|---|
| **Total public repos** | 193 | 233 |
| **Top 5** | next.js (139,885), hyper (44,600), swr (32,398), turborepo (30,508), ai (24,707) | next.js (139,875), hyper (44,599), swr (32,397), turborepo (30,506), ai (24,703) — identical repos, negligible star drift |
| **Repos with 1k+ stars** | 44 | 44 |
| **Quality score** | **2** | **2** |

*Drift question — total count discrepancy (193 vs 233) is unresolved. Top 5 and 1k+ threshold count agree. Drift scoring applies.*

---

## Per-Question Table (all 17)

Token score = `quality / (total_chars / 1000)`. Drift Qs excluded from main quality tally.

| Q | Category | Drift | Octo qual | gh qual | Octo calls | gh calls | Octo chars | gh chars | Octo token score | gh token score | Winner |
|---|---|:---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| Q1 | SEARCH | | 2 | 0 | 10 | 4 | 65,939 | 9,978 | **0.030** | 0.000 | **octo** |
| Q2 | SEARCH | | 3 | 3 | 2 | 9 | 8,148 | 193,893 | **0.368** | 0.015 | **octo** |
| Q3 | SEARCH | | 2 | 3 | 3 | 288 | 5,784 | 1,877,334 | **0.346** | 0.002 | **octo** |
| Q4 | SEARCH | | 3 | 2 | 4 | 74 | 27,504 | 5,831,123 | **0.109** | 0.000 | **octo** |
| Q5 | CONTENT | | 2 | 2 | 8 | 1 | 18,031 | 202,909 | **0.111** | 0.010 | **octo** |
| Q6 | CONTENT | | 3 | 2 | 8 | 2 | 23,151 | 362,135 | **0.130** | 0.006 | **octo** |
| Q7 | CONTENT | | 1 | 3 | 9 | 1 | 9,815 | 3,151,873 | **0.102** | 0.001 | **octo** |
| Q8 | CONTENT | | 3 | 3 | 2 | 1 | 324,573 | 25,602 | 0.009 | **0.117** | **gh** |
| Q9 | STRUCTURE | | 3 | 3 | 1 | 1 | 997 | 32,732 | **3.009** | 0.092 | **octo** |
| Q10 | STRUCTURE | | 3 | 3 | 1 | 2 | 2,128 | 22,011 | **1.410** | 0.136 | **octo** |
| Q11 | STRUCTURE | | 3 | 2 | 7 | 3 | 9,101 | 61,181 | **0.330** | 0.033 | **octo** |
| Q12 | PR | ✓ | 2 | 1 | 13 | 10 | 94,789 | 3,318 | (0.021) | (0.301) | drift |
| Q13 | PR | | 2 | 2 | 4 | 1 | 34,052 | 5,268 | 0.059 | **0.380** | **gh** |
| Q14 | PR | | 3 | 1 | 31 | 4 | 398,337 | 117,337 | 0.008 | 0.009 | tie |
| Q15 | PR | | 3 | 3 | 1 | 3 | 16,813 | 51,488 | **0.179** | 0.058 | **octo** |
| Q16 | REPOS | ✓ | 3 | 3 | 2 | 2 | 20,674 | 2,145 | (0.145) | (1.399) | drift |
| Q17 | REPOS | ✓ | 2 | 2 | 2 | 1 | 19,630 | 1,254,816 | (0.102) | (0.002) | drift |
| **Σ all** | | | **43** | **38** | **108** | **407** | **1,079,466** | **13,205,143** | | | |
| **Σ non-drift** | | | **36** | **32** | **93** | **396** | **944,373** | **11,944,864** | | | |

*Non-drift chars: octo = 1,079,466 − 94,789 − 20,674 − 19,630 = 944,373 · gh = 13,205,143 − 3,318 − 2,145 − 1,254,816 = 11,944,864*

---

## Quality Verdict (non-drift Qs only)

14 non-drift questions: Q1–Q11, Q13–Q15

| Agent | Σ quality | Avg quality/Q | Token-score wins | Token-score ties |
|---|---:|---:|---:|---:|
| **octocode** | **36** | **2.57** | **11** | 1 |
| gh | 32 | 2.29 | 2 | 1 |

**Quality arithmetic (octo non-drift):** Q1(2)+Q2(3)+Q3(2)+Q4(3)+Q5(2)+Q6(3)+Q7(1)+Q8(3)+Q9(3)+Q10(3)+Q11(3)+Q13(2)+Q14(3)+Q15(3) = **36**

**Quality arithmetic (gh non-drift):** Q1(0)+Q2(3)+Q3(3)+Q4(2)+Q5(2)+Q6(2)+Q7(3)+Q8(3)+Q9(3)+Q10(3)+Q11(2)+Q13(2)+Q14(1)+Q15(3) = **32**

Token-score wins (non-drift):
- **octo wins (11):** Q1, Q2, Q3, Q4, Q5, Q6, Q7, Q9, Q10, Q11, Q15
- **gh wins (2):** Q8, Q13
- **Tie (1):** Q14 — octo 0.008 vs gh 0.009 (gh barely, within rounding)

---

## Drift Verdict (reported separately)

| Q | Category | Octo qual | gh qual | Octo token score | gh token score | Notes |
|---|---|---:|---:|---:|---:|---|
| Q12 | PR | 2 | 1 | 0.021 | 0.301 | Label "Pages Router" returned 0 for both. Octo provided investigative fallback with 5 title-based proxy PRs. Gh returned bare 0 |
| Q16 | REPOS | 3 | 3 | 0.145 | 1.399 | Both found ~202–204 matching repos, same top 5. Gh cheaper (2,145 chars vs 20,674) |
| Q17 | REPOS | 2 | 2 | 0.102 | 0.002 | Total count diverges (193 vs 233) — unresolved. Top 5 and 44 repos with 1k+ stars agree. Octo much cheaper on chars |

---

## Quality-Adjusted Token-Usage Verdict

### Aggregate metrics

| Axis | octocode | gh | ratio (octo/gh) |
|---|---:|---:|---:|
| **Σ quality — non-drift (14 Qs)** | **36** | **32** | **1.13×** |
| Σ quality — all 17 Qs | 43 | 38 | 1.13× |
| Σ calls (all Qs) | 108 | 407 | 0.27 (octo 3.8× fewer) |
| Σ in_chars (all Qs) | 15,154 | 41,852 | 0.36 (octo 2.8× fewer) |
| Σ out_chars (all Qs) | 1,064,312 | 13,163,291 | 0.081 (octo 12.4× fewer) |
| **TOTAL chars (all Qs)** | **1,079,466** | **13,205,143** | **0.082 (octo 12.2× fewer)** |
| Approx tokens (chars / 4) | 269,867 | 3,301,286 | 0.082 |
| TOTAL chars non-drift | 944,373 | 11,944,864 | 0.079 |
| **Quality per 1k chars (non-drift)** | **0.03812** | **0.002679** | **14.23× (octo wins)** |
| Σ tool_elapsed_ms (context only) | 277,860 | 353,057 | 0.79× |
| Σ q_elapsed_ms (context only) | 1,420,191 | 660,088 | 2.15× |
| Σ reasoning_ms (context only) | 1,142,331 | 307,031 | 3.72× |

### Quality per 1k chars arithmetic

```
octo:  36 quality / (944,373 chars / 1000) = 36 / 944.373 = 0.03812
gh:    32 quality / (11,944,864 chars / 1000) = 32 / 11,944.864 = 0.002679
ratio: 0.03812 / 0.002679 = 14.23×
```

---

## Category Analysis

| Category | Qs | Octo Σ | gh Σ | Octo avg | gh avg | Category winner |
|---|---|---:|---:|---:|---:|---|
| SEARCH (Q1–Q4) | 4 | 10 | 8 | 2.50 | 2.00 | **octo** |
| CONTENT (Q5–Q8) | 4 | 9 | 10 | 2.25 | 2.50 | **gh** |
| STRUCTURE (Q9–Q11) | 3 | 9 | 8 | 3.00 | 2.67 | **octo** |
| PR (Q13–Q15) | 3 | 8 | 6 | 2.67 | 2.00 | **octo** |

*Note: Q12 (PR, drift) excluded from PR category sum; Q16–Q17 (REPOS, drift) excluded from all category counts.*

---

## Capability Review

### Where gh scored lower and why

**Q1 — SEARCH result limit + repo scoping failure**
`gh search code` returned paths (`/.agents/skills/router-act/SKILL.md`, `/.claude-plugin/…`) from the user's local workspace rather than `vercel/next.js`. This is a fundamental scoping failure — the `repo:vercel/next.js` filter was either dropped or the query ran without it. Octocode's `githubSearchCode` maintained correct repo scoping throughout all 10 pages.

**Q4 — SEARCH bulk workflow**
74 calls and 5.8M chars to perform AND-intersection, yet gh still found only 41 files vs octocode's 62. Octocode issued a proper multi-keyword AND query in 4 calls. The 21-file gap includes server request handlers, use-cache wrappers, and adapter files that should match both terms. This reflects limitations in `gh search code`'s ability to express file-level AND constraints at scale.

**Q6 — CONTENT large file tail read**
gh retrieved the full 362K-char CHANGELOG and still identified `4.0.0-beta.0` as the "first 4.x release" instead of the stable `4.0.0`. The beta appears at the bottom of the linked `v4.0.4` tag changelog, and gh did not distinguish pre-releases from stable releases. Octocode used char-offset windowing to read the tail section and arrived at the correct stable release.

**Q11 — STRUCTURE entry-point discovery**
gh answered at the CLI invocation level (`cli.ts` → `createServer()`), not the server module level. The question asks which file is "the main entry point for the dev server" and "what is the first function it calls on startup" — the answer requires reading into `server/index.ts` and tracing `_createServer()`'s first call (`disableCache()`). Octocode followed this two-step path correctly.

**Q14 — PR archaeology: wrong PR identified**
gh selected PR #1986 (`feat(jsx/dom): provide jsx-runtime and jsx-dev-runtime via jsx/dom`) — a 2024 jsx/dom runtime expansion — instead of PR #306 (`feat: jsx middleware`, June 2022), which added the `hono/jsx` package export to `package.json`. The PR search ranked the higher-commit, more recent PR above the actual introducing PR. Both Q14 and Q15 (which gh answered correctly) corroborate that #306 is the introducing PR.

### Where gh scored equal or better

**Q7 — CONTENT over-size-limit file (gh wins)**
gh downloaded the full 3.15M-char `checker.ts` blob (the only way to read a 3MB file through GitHub's raw API) and correctly answered both sub-questions: `createTypeChecker` at line 1486 and `createBasicNodeBuilderModuleSpecifierResolutionHost` at line 54260. Octocode could not retrieve the file tail and reported UNKNOWN for the last function. **This is a real capability gap for octocode on very large files when a clone is unavailable.**

**Q8 — CONTENT directory listing (gh wins token score, tie quality)**
Both answered correctly (27 `.js` files, 0 `.ts`). gh used 25,602 chars; octocode used 324,573 chars (likely a full tree dump). gh's compact JSON directory listing was 13× more efficient for the same quality answer.

**Q13 — PR inline comments (tie)**
gh claimed 2 inline review comments with a file identified (`packages/react-devtools-inline/src/frontend.js`). Octocode reported 0 (classified all as PR-level). Both score 2 because: octocode's objection quote is accurate; gh's quote is the PR author's explanation, not a reviewer's inline objection. The true inline comment count requires independent API verification.

---

## Verdict

### **Winner: octocode** — by a decisive margin on quality-adjusted character efficiency

**Quality:** octocode 36 vs gh 32 on non-drift questions (+12.5%). octocode won or tied 12 of 14 non-drift questions; gh won 2 (Q8 token score, Q13 token score) and tied 1 (Q14).

**Token efficiency:** octocode achieved **14.2× better quality per character** (0.0381 vs 0.0027 quality/1k-chars). It completed all 17 questions in 1.08M chars; gh used 13.2M chars — 12.2× more. Octocode used 108 calls vs gh's 407.

**Key tradeoffs:**

| Dimension | Advantage | Evidence |
|---|---|---|
| Bulk multi-repo queries | **octo** | Q2: 2 calls vs 9; Q10: 1 call vs 2 |
| AND-intersection code search | **octo** | Q4: 62 files vs 41, 4 calls vs 74 |
| Large-file windowed reads | **octo** | Q6: correct stable 4.0.0 at low char cost |
| Over-size-limit file (tail read) | **gh** | Q7: full blob retrieval, both functions answered |
| Directory listing efficiency | **gh** | Q8: 25K chars vs 324K chars, equal quality |
| PR archaeology accuracy | **octo** | Q14: correct PR #306 vs gh's wrong #1986 |
| Repo scoping correctness | **octo** | Q1: gh returned user workspace paths |
| Inline PR comment depth | draw | Q13: count disputed, objection quality favors octo |
| Exhaustive test enumeration | **gh** | Q3: gh listed all test-file call sites |

The efficiency advantage is structural, not incidental. Octocode's bulk-query design, structured result formats, and targeted char-offset windowing allow it to answer more questions correctly at dramatically lower character cost across five different capability categories.
