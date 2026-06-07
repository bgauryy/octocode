# Run gh

| Agent | Questions | Calls | In Chars | Out Chars | Total Chars | Approx Tokens | Tool ms | Q wall ms | Reasoning ms |
|-------|----------:|------:|---------:|----------:|------------:|--------------:|--------:|----------:|-------------:|
| gh | 17 / 17 | 407 | 41,852 | 13,163,291 | 13,205,143 | 3,301,286 | 353,057 | 660,088 | 307,031 |

> **Total Chars** = per-question `in_chars + out_chars`. **Approx Tokens** = `ceil(Total Chars / 4)` and is a rough display-only token proxy; characters remain the canonical measurement. **Tool/Q/Reasoning ms** are context only for token-usage judging.

| Q | Calls | In Chars | Out Chars | Total Chars | Approx Tokens | Tool ms | Q wall ms | Reasoning ms | Answer (one line) |
|---|------:|---------:|----------:|------------:|--------------:|--------:|----------:|-------------:|-------------------|
| Q1 | 4 | 371 | 9,607 | 9,978 | 2,495 | 8,068 | 85,868 | 77,800 | - Unique files returned by `gh search code` containing exac… |
| Q2 | 9 | 793 | 193,100 | 193,893 | 48,474 | 6,549 | 41,678 | 35,129 | - `facebook/react`: `useState` is implemented in `packages/… |
| Q3 | 288 | 28,132 | 1,849,202 | 1,877,334 | 469,334 | 238,882 | 281,801 | 42,919 | - `src/compose.test.ts:45` — `const composed = compose(midd… |
| Q4 | 74 | 9,384 | 5,821,739 | 5,831,123 | 1,457,781 | 70,657 | 75,546 | 4,889 | - Files containing both `ppr` and `Postpone`: 41. |
| Q5 | 1 | 123 | 202,786 | 202,909 | 50,728 | 640 | 752 | 112 | - Top-level exported functions: `getWorkInProgressTransitio… |
| Q6 | 2 | 201 | 361,934 | 362,135 | 90,534 | 1,237 | 14,985 | 13,748 | - Latest release version listed in `packages/vite/CHANGELOG… |
| Q7 | 1 | 101 | 3,151,772 | 3,151,873 | 787,969 | 1,822 | 18,166 | 16,344 | - `createTypeChecker(host: TypeCheckerHost): TypeChecker` i… |
| Q8 | 1 | 52 | 25,550 | 25,602 | 6,401 | 525 | 651 | 126 | - Total direct files in `packages/react/src/`: 27. |
| Q9 | 1 | 92 | 32,640 | 32,732 | 8,183 | 539 | 657 | 118 | - `.ts` source files under `packages/reactivity/src/`: 13. |
| Q10 | 2 | 74 | 21,937 | 22,011 | 5,503 | 921 | 12,342 | 11,421 | - `honojs/hono/src/` has 10 top-level subdirectories: `adap… |
| Q11 | 3 | 253 | 60,928 | 61,181 | 15,296 | 2,521 | 3,504 | 983 | - The dev server is entered from the CLI entry file `packag… |
| Q12 | 10 | 1,338 | 1,980 | 3,318 | 830 | 7,274 | 78,506 | 71,232 | - Merged PRs in `vercel/next.js` with label `Pages Router` … |
| Q13 | 1 | 56 | 5,212 | 5,268 | 1,317 | 811 | 939 | 128 | - Inline review comments on PR #27733: 2. |
| Q14 | 4 | 261 | 117,076 | 117,337 | 29,335 | 2,848 | 15,920 | 13,072 | - PR: #1986 — feat(jsx/dom): provide jsx-runtime and jsx-de… |
| Q15 | 3 | 193 | 51,295 | 51,488 | 12,872 | 2,499 | 10,236 | 7,737 | - First merged PR introducing JSX/JSX renderer support: #30… |
| Q16 | 2 | 371 | 1,774 | 2,145 | 537 | 3,018 | 14,148 | 11,130 | - Matching TypeScript repositories with topic `mcp`, at lea… |
| Q17 | 1 | 57 | 1,254,759 | 1,254,816 | 313,704 | 4,246 | 4,389 | 143 | - Total public repositories in `vercel`: 233. |
| **Σ** | **407** | **41,852** | **13,163,291** | **13,205,143** | **3,301,286** | **353,057** | **660,088** | **307,031** | |
