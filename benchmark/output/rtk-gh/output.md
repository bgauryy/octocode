# Run rtk-gh

| Agent | Questions | Calls | In Chars | Out Chars | Total Chars | Approx Tokens | Tool ms | Q wall ms | Reasoning ms |
|-------|----------:|------:|---------:|----------:|------------:|--------------:|--------:|----------:|-------------:|
| rtk-gh | 20 / 20 | 217 | 16,344 | 4,764,682 | 4,781,026 | 1,195,257 | 100,531 | 7,490 | 1,150 |

> **Total Chars** = per-question `in_chars + out_chars`. **Approx Tokens** = `ceil(Total Chars / 4)` and is a rough display-only token proxy; characters remain the canonical measurement. **Tool/Q/Reasoning ms** are context only for token-usage judging.

| Q | Calls | In Chars | Out Chars | Total Chars | Approx Tokens | Tool ms | Q wall ms | Reasoning ms | Answer (one line) |
|---|------:|---------:|----------:|------------:|--------------:|--------:|----------:|-------------:|-------------------|
| Q1 | 5 | 302 | 71,422 | 71,724 | 17,931 | 2,041 | 582 | 0 | - `notFound()` is defined in `packages/next/src/client/comp… |
| Q2 | 2 | 130 | 19 | 149 | 38 | 867 | 607 | 0 | - `NextRequest` is defined at `packages/next/src/server/web… |
| Q3 | 11 | 908 | 63,399 | 64,307 | 16,077 | 2,398 | 586 | 0 | - `packages/next/src/server/web/spec-extension/revalidate.t… |
| Q4 | 34 | 3,330 | 1,116,293 | 1,119,623 | 279,906 | 4,006 | 637 | 0 | - **33** files under `packages/next/src/` contain both `app… |
| Q5 | 1 | 33 | 8 | 41 | 11 | 562 | 654 | 92 | - `redirect()` is defined in `packages/next/src/client/comp… |
| Q6 | 1 | 33 | 8 | 41 | 11 | 523 | 611 | 88 | - `renderToHTMLOrFlight` return type: `Promise<RenderResult… |
| Q7 | 1 | 33 | 8 | 41 | 11 | 504 | 592 | 88 | - Server-side `revalidateTag` is defined in `packages/next/… |
| Q8 | 1 | 33 | 8 | 41 | 11 | 500 | 587 | 87 | - Header: `ACTION_HEADER = 'next-action'` in `packages/next… |
| Q9 | 22 | 1,663 | 54,626 | 56,289 | 14,073 | 13,387 | 604 | 0 | - PR **#57287** — title: **Partial Prerendering** |
| Q10 | 129 | 9,389 | 3,441,271 | 3,450,660 | 862,665 | 75,109 | 601 | 0 | - PR **#47438** — title: **Finalize HOC support with server… |
| Q11 | 1 | 49 | 1,762 | 1,811 | 453 | 66 | 136 | 70 | - `packages/next/src/server/send-response.ts` — `export asy… |
| Q12 | 1 | 49 | 1,762 | 1,811 | 453 | 64 | 148 | 84 | - Total count: **312** |
| Q13 | 1 | 49 | 1,762 | 1,811 | 453 | 65 | 146 | 81 | - Build invokes `generateStaticParams` via `callGenerateSta… |
| Q14 | 1 | 49 | 1,762 | 1,811 | 453 | 69 | 147 | 78 | - **25** direct subdirectories under `packages/next/src/ser… |
| Q15 | 1 | 49 | 1,762 | 1,811 | 453 | 74 | 160 | 86 | - The default export abstract class in `base-server.ts` is … |
| Q16 | 1 | 49 | 1,762 | 1,811 | 453 | 72 | 148 | 76 | - `reactStrictMode` default: `null` (`packages/next/src/ser… |
| Q17 | 1 | 49 | 1,762 | 1,811 | 453 | 57 | 121 | 64 | - Inheritance chain (leaf → root) for `NextNodeServer`: |
| Q18 | 1 | 49 | 1,762 | 1,811 | 453 | 66 | 140 | 74 | - Concrete class override of `protected abstract sendRender… |
| Q19 | 1 | 49 | 1,762 | 1,811 | 453 | 43 | 155 | 112 | - **29** unique files reference `unstable_cache` (excluding… |
| Q20 | 1 | 49 | 1,762 | 1,811 | 453 | 58 | 128 | 70 | - Direct caller: `render` method of the App Page route modu… |
| **Σ** | **217** | **16,344** | **4,764,682** | **4,781,026** | **1,195,257** | **100,531** | **7,490** | **1,150** | |
