# Run gh

| Agent | Questions | Calls | In Chars | Out Chars | Total Chars | Approx Tokens | Tool ms | Q wall ms | Reasoning ms |
|-------|----------:|------:|---------:|----------:|------------:|--------------:|--------:|----------:|-------------:|
| gh | 20 / 20 | 98 | 8,576 | 10,315,764 | 10,324,340 | 2,581,085 | 95,187 | 2,962,177 | 2,866,990 |

> **Total Chars** = per-question `in_chars + out_chars`. **Approx Tokens** = `ceil(Total Chars / 4)` and is a rough display-only token proxy; characters remain the canonical measurement. **Tool/Q/Reasoning ms** are context only for token-usage judging.

| Q | Calls | In Chars | Out Chars | Total Chars | Approx Tokens | Tool ms | Q wall ms | Reasoning ms | Answer (one line) |
|---|------:|---------:|----------:|------------:|--------------:|--------:|----------:|-------------:|-------------------|
| Q1 | 3 | 224 | 30,684 | 30,908 | 7,727 | 3,902 | 34,752 | 30,850 | - `gh search code` returns at most 100 results per call (Gi… |
| Q2 | 17 | 1,253 | 454,538 | 455,791 | 113,948 | 15,788 | 130,363 | 114,575 | **facebook/react — `useState`** |
| Q3 | 5 | 342 | 38,085 | 38,427 | 9,607 | 3,947 | 39,495 | 35,548 | Calls to `compose()` inside `src/` in `honojs/hono` (exclud… |
| Q4 | 2 | 123 | 13,274 | 13,397 | 3,350 | 3,789 | 26,716 | 22,927 | - `gh search code` cannot natively express file-level AND i… |
| Q5 | 15 | 1,119 | 3,171,084 | 3,172,203 | 793,051 | 13,249 | 154,192 | 140,943 | **Note:** In the current `main` branch of `facebook/react`,… |
| Q6 | 5 | 311 | 981,874 | 982,185 | 245,547 | 5,228 | 96,369 | 91,141 | **First `4.x` release: `4.0.0` (2022-12-09)** |
| Q7 | 4 | 338 | 4,344,124 | 4,344,462 | 1,086,116 | 4,960 | 58,225 | 53,265 | **File size:** 3,151,774 bytes (~3.1 MB), 54,435 lines. The… |
| Q8 | 1 | 52 | 25,550 | 25,602 | 6,401 | 745 | 13,844 | 13,099 | - Total files directly in `packages/react/src/` (not recurs… |
| Q9 | 2 | 100 | 206,986 | 207,086 | 51,772 | 1,802 | 20,087 | 18,285 | **13 `.ts` source files** under `packages/reactivity/src/` … |
| Q10 | 2 | 74 | 21,937 | 22,011 | 5,503 | 1,421 | 18,950 | 17,529 | **`honojs/hono` — `src/`** |
| Q11 | 4 | 230 | 103,213 | 103,443 | 25,861 | 3,195 | 37,409 | 34,214 | **Main entry point for the Vite dev server:** `packages/vit… |
| Q12 | 25 | 3,315 | 420,500 | 423,815 | 105,954 | 19,534 | 2,184,672 | 2,165,138 | - The `Pages Router` label exists in `vercel/next.js` (labe… |
| Q13 | 1 | 58 | 5,212 | 5,270 | 1,318 | 1,042 | 16,199 | 15,157 | - **Inline review comments (code-level thread comments):** … |
| Q14 | 3 | 251 | 54,591 | 54,842 | 13,711 | 2,226 | 30,716 | 28,490 | The merged PR introducing `hono/jsx` (JSX package/runtime s… |
| Q15 | 1 | 61 | 766 | 827 | 207 | 829 | 10,393 | 9,564 | **PR #306 — "feat: jsx middleware"** — merged 2022-06-10 |
| Q16 | 1 | 121 | 55,829 | 55,950 | 13,988 | 2,101 | 9,932 | 7,831 | - **Total TypeScript repositories matching (topic:mcp, ≥500… |
| Q17 | 2 | 151 | 283,204 | 283,355 | 70,839 | 4,998 | 21,364 | 16,366 | - **Total public repositories in `vercel` org:** 193 `[drif… |
| Q18 | 0 | 0 | 0 | 0 | 0 | 0 | 5,884 | 5,884 | UNKNOWN — out of scope for gh CLI. The `gh` CLI has no acce… |
| Q19 | 2 | 198 | 58,975 | 59,173 | 14,794 | 3,269 | 19,452 | 16,183 | - **Repositories in `vitejs` org with topic `vite-plugin` u… |
| Q20 | 3 | 255 | 45,338 | 45,593 | 11,399 | 3,162 | 33,163 | 30,001 | **PR #46827 — "feat(7481): Operator to ensure an expression… |
| **Σ** | **98** | **8,576** | **10,315,764** | **10,324,340** | **2,581,085** | **95,187** | **2,962,177** | **2,866,990** | |
