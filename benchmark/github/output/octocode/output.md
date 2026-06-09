# Run octocode

| Agent | Questions | Calls | In Chars | Out Chars | Total Chars | Approx Tokens | Tool ms | Q wall ms | Reasoning ms |
|-------|----------:|------:|---------:|----------:|------------:|--------------:|--------:|----------:|-------------:|
| octocode | 20 / 20 | 68 | 39,986 | 695,941 | 735,927 | 183,982 | 235,146 | 1,795,590 | 1,560,444 |

> **Total Chars** = per-question `in_chars + out_chars`. **Approx Tokens** = `ceil(Total Chars / 4)` and is a rough display-only token proxy; characters remain the canonical measurement. **Tool/Q/Reasoning ms** are context only for token-usage judging.

| Q | Calls | In Chars | Out Chars | Total Chars | Approx Tokens | Tool ms | Q wall ms | Reasoning ms | Answer (one line) |
|---|------:|---------:|----------:|------------:|--------------:|--------:|----------:|-------------:|-------------------|
| Q1 | 2 | 1,276 | 21,268 | 22,544 | 5,636 | 5,809 | 43,764 | 37,955 | - The GitHub Search API caps at 1,000 total results; the qu… |
| Q2 | 2 | 2,655 | 6,938 | 9,593 | 2,399 | 14,897 | 46,425 | 31,528 | - **facebook/react — `useState`**: |
| Q3 | 4 | 2,782 | 9,587 | 12,369 | 3,093 | 7,720 | 43,530 | 35,810 | - `src/hono-base.ts`, line 225: `(await compose([], app.err… |
| Q4 | 2 | 1,763 | 26,312 | 28,075 | 7,019 | 7,115 | 94,219 | 87,104 | - The query for both `ppr` AND `Postpone` in `vercel/next.j… |
| Q5 | 7 | 4,502 | 60,620 | 65,122 | 16,281 | 16,502 | 127,965 | 111,463 | - **Note:** `performConcurrentWorkOnRoot` does **not exist*… |
| Q6 | 4 | 2,052 | 24,191 | 26,243 | 6,561 | 9,842 | 58,454 | 48,612 | - **Latest release version**: `8.0.16` (2026-06-01) — fixed… |
| Q7 | 6 | 3,399 | 3,387 | 6,786 | 1,697 | 13,549 | 117,609 | 104,060 | - **File size**: 54,434 lines; 3,078 KB — exceeds the GitHu… |
| Q8 | 1 | 433 | 1,579 | 2,012 | 503 | 1,909 | 12,283 | 10,374 | - **Total files directly in `packages/react/src/`**: 27 fil… |
| Q9 | 1 | 423 | 915 | 1,338 | 335 | 1,979 | 7,903 | 5,924 | - **Total `.ts` source files** in `packages/reactivity/src/… |
| Q10 | 1 | 800 | 1,848 | 2,648 | 662 | 1,669 | 14,313 | 12,644 | - **honojs/hono `src/`**: 10 subdirectories at the top leve… |
| Q11 | 8 | 3,811 | 20,438 | 24,249 | 6,063 | 18,168 | 79,310 | 61,142 | - **Main entry point for the dev server**: `packages/vite/s… |
| Q12 | 5 | 2,647 | 52,942 | 55,589 | 13,898 | 29,289 | 78,659 | 49,370 | - The label `"Pages Router"` does not return results when u… |
| Q13 | 3 | 1,394 | 15,705 | 17,099 | 4,275 | 8,169 | 57,964 | 49,795 | - **PR #27733** — "Add reload and profile to react-devtools… |
| Q14 | 5 | 2,520 | 94,685 | 97,205 | 24,302 | 21,657 | 90,290 | 68,633 | - **PR that introduced `hono/jsx`**: PR #306 — "feat: jsx m… |
| Q15 | 1 | 437 | 1,052 | 1,489 | 373 | 1,933 | 16,369 | 14,436 | - **PR #306** — "feat: jsx middleware" is the first merged … |
| Q16 | 2 | 986 | 21,650 | 22,636 | 5,659 | 8,351 | 42,878 | 34,527 | - **Total matches** (topic `mcp`, ≥500 stars, updated since… |
| Q17 | 2 | 1,104 | 29,133 | 30,237 | 7,560 | 7,843 | 48,798 | 40,955 | - **Total public repos in `vercel` GitHub org**: **193** `[… |
| Q18 | 3 | 2,553 | 9,273 | 11,826 | 2,957 | 31,502 | 672,407 | 640,905 | - **`hono`**: |
| Q19 | 3 | 1,366 | 11,366 | 12,732 | 3,183 | 6,933 | 47,905 | 40,972 | - **Number of repos in `vitejs` org with topic `vite-plugin… |
| Q20 | 6 | 3,083 | 283,052 | 286,135 | 71,534 | 20,310 | 94,545 | 74,235 | - **PR number**: #46827 |
| **Σ** | **68** | **39,986** | **695,941** | **735,927** | **183,982** | **235,146** | **1,795,590** | **1,560,444** | |
