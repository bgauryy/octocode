# Run rtk-gh

| Agent | Questions | Calls | In Chars | Out Chars | Total Chars | Approx Tokens | Tool ms | Q wall ms | Reasoning ms |
|-------|----------:|------:|---------:|----------:|------------:|--------------:|--------:|----------:|-------------:|
| rtk-gh | 10 / 10 | 224 | 18,053 | 4,758,107 | 4,776,160 | 1,194,040 | 98,881 | 223,205 | 218,255 |

> **Total Chars** = per-question `in_chars + out_chars`. **Approx Tokens** = `ceil(Total Chars / 4)` and is a rough display-only token proxy; characters remain the canonical measurement. **Tool/Q/Reasoning ms** are context only and never decide the winner.

| Q | Calls | In Chars | Out Chars | Total Chars | Approx Tokens | Tool ms | Q wall ms | Reasoning ms | Answer (one line) |
|---|------:|---------:|----------:|------------:|--------------:|--------:|----------:|-------------:|-------------------|
| Q1 | 5 | 302 | 71,422 | 71,724 | 17,931 | 2,041 | 582 | 0 | - `notFound()` is defined in `packages/next/src/client/comp… |
| Q2 | 3 | 216 | 626 | 842 | 211 | 337 | 25,036 | 24,699 | - `NextRequest` is defined at `packages/next/src/server/web… |
| Q3 | 11 | 908 | 63,399 | 64,307 | 16,077 | 2,398 | 586 | 0 | - `packages/next/src/server/web/spec-extension/revalidate.t… |
| Q4 | 34 | 3,330 | 1,116,293 | 1,119,623 | 279,906 | 4,006 | 637 | 0 | - **33** files under `packages/next/src/` contain both `app… |
| Q5 | 5 | 552 | 4,007 | 4,559 | 1,140 | 367 | 49,431 | 49,064 | - `redirect()` is defined in `packages/next/src/client/comp… |
| Q6 | 4 | 502 | 792 | 1,294 | 324 | 309 | 44,009 | 43,700 | - `renderToHTMLOrFlight` return type: `Promise<RenderResult… |
| Q7 | 4 | 421 | 3,102 | 3,523 | 881 | 326 | 45,784 | 45,458 | - Server-side `revalidateTag` is defined in `packages/next/… |
| Q8 | 7 | 770 | 2,569 | 3,339 | 835 | 601 | 55,935 | 55,334 | - Header: `ACTION_HEADER = 'next-action' as const` at `pack… |
| Q9 | 22 | 1,663 | 54,626 | 56,289 | 14,073 | 13,387 | 604 | 0 | - PR **#57287** — title: **Partial Prerendering** |
| Q10 | 129 | 9,389 | 3,441,271 | 3,450,660 | 862,665 | 75,109 | 601 | 0 | - PR **#47438** — title: **Finalize HOC support with server… |
| **Σ** | **224** | **18,053** | **4,758,107** | **4,776,160** | **1,194,040** | **98,881** | **223,205** | **218,255** | |
