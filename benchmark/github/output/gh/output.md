# Run gh

| Agent | Questions | Calls | In Chars | Out Chars | Total Chars | Approx Tokens | Tool ms | Q wall ms | Reasoning ms |
|-------|----------:|------:|---------:|----------:|------------:|--------------:|--------:|----------:|-------------:|
| gh | 20 / 20 | 131 | 15,516 | 14,591,996 | 14,607,512 | 3,651,878 | 1,738,270 | 1,372,808 | 1,003,818 |

> **Total Chars** = per-question `in_chars + out_chars`. **Approx Tokens** = `ceil(Total Chars / 4)` and is a rough display-only token proxy; characters remain the canonical measurement. **Tool/Q/Reasoning ms** are context only for token-usage judging.

| Q | Calls | In Chars | Out Chars | Total Chars | Approx Tokens | Tool ms | Q wall ms | Reasoning ms | Answer (one line) |
|---|------:|---------:|----------:|------------:|--------------:|--------:|----------:|-------------:|-------------------|
| Q1 | 2 | 151 | 29,282 | 29,433 | 7,359 | 2,352 | 28,235 | 25,883 | - `gh search code` is hard-capped at 100 results per query;… |
| Q2 | 20 | 1,311 | 302,972 | 304,283 | 76,071 | 11,390 | 345,647 | 334,257 | - **`facebook/react` — `useState`** |
| Q3 | 6 | 387 | 39,532 | 39,919 | 9,980 | 3,953 | 76,624 | 72,671 | - `gh search code` returned 13 files containing "compose" i… |
| Q4 | 3 | 196 | 657,105 | 657,301 | 164,326 | 2,122 | 52,025 | 49,903 | - `gh search code "ppr Postpone" --repo vercel/next.js` ret… |
| Q5 | 7 | 781 | 1,294,369 | 1,295,150 | 323,788 | 7,563 | 63,107 | 55,544 | - File: `packages/react-reconciler/src/ReactFiberWorkLoop.j… |
| Q6 | 14 | 1,477 | 2,382,690 | 2,384,167 | 596,042 | 16,137 | 108,138 | 92,001 | - The current `packages/vite/CHANGELOG.md` only contains en… |
| Q7 | 4 | 454 | 9,455,365 | 9,455,819 | 2,363,955 | 4,776 | 70,821 | 66,045 | - File: `src/compiler/checker.ts` in `microsoft/TypeScript`… |
| Q8 | 1 | 52 | 24,947 | 24,999 | 6,250 | 1,106 | 11,076 | 9,970 | - **Total files directly in `packages/react/src/`**: 27 fil… |
| Q9 | 1 | 53 | 10,983 | 11,036 | 2,759 | 550 | 7,856 | 7,306 | - **13 `.ts` source files** exist directly in `packages/rea… |
| Q10 | 2 | 74 | 21,937 | 22,011 | 5,503 | 1,167 | 19,402 | 18,235 | **honojs/hono `src/` — 10 subdirectories:** |
| Q11 | 6 | 368 | 224,919 | 225,287 | 56,322 | 4,130 | 50,145 | 46,015 | - **Main dev server entry point**: `packages/vite/src/node/… |
| Q12 | 42 | 7,326 | 120,653 | 127,979 | 31,995 | 1,662,728 | 293,448 | 0 | - **Label existence**: The `Pages Router` label exists in `… |
| Q13 | 2 | 321 | 959 | 1,280 | 320 | 2,268 | 24,919 | 22,651 | - **Inline review comment count**: **2** (via `repos/facebo… |
| Q14 | 6 | 691 | 6,035 | 6,726 | 1,682 | 3,989 | 49,685 | 45,696 | - **PR**: #306 — `feat: jsx middleware` in `honojs/hono` |
| Q15 | 2 | 119 | 1,805 | 1,924 | 481 | 1,016 | 23,487 | 22,471 | - **PR number**: #306 |
| Q16 | 1 | 251 | 2,062 | 2,313 | 579 | 1,902 | 20,311 | 18,409 | - **Total matching repos** (TypeScript, topic `mcp`, ≥500 s… |
| Q17 | 3 | 320 | 230 | 550 | 138 | 3,025 | 30,881 | 27,856 | - **Total public repos in `vercel` org**: **234** (from `or… |
| Q18 | 0 | 0 | 0 | 0 | 0 | 0 | 12,106 | 12,106 | UNKNOWN — out of scope for gh CLI. The `gh` CLI has no npm/… |
| Q19 | 3 | 543 | 2,501 | 3,044 | 761 | 3,496 | 37,581 | 34,085 | - **Total repos in `vitejs` org with topic `vite-plugin` up… |
| Q20 | 6 | 641 | 13,650 | 14,291 | 3,573 | 4,600 | 47,314 | 42,714 | - **PR number**: #46827 |
| **Σ** | **131** | **15,516** | **14,591,996** | **14,607,512** | **3,651,878** | **1,738,270** | **1,372,808** | **1,003,818** | |
