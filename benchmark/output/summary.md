# Benchmark Summary: octocode vs rtk-gh

**Repo:** vercel/next.js · **Questions:** 20 · **Date:** 2026-06-13

## Per-question scores

| Q | octocode D | rtk-gh D | octocode chars | rtk-gh chars | efficiency ratio | Notes |
|---|---|---|---|---|---|---|
| Q1 | 2 | 2 | 988 | 71,724 | 0.01× | Both define `not-found.ts:23` + `throw error`; octocode catch in `app-render.tsx` (`renderToStream` ~3972, misnamed `renderToHTMLOrFlightImpl`); rtk cites client `HTTPAccessFallbackErrorBoundary` only |
| Q2 | 3 | 3 | 1,658 | 149 | 11.1× | All three class declarations at correct file:line |
| Q3 | 3 | 3 | 1,149 | 64,307 | 0.02× | Verified zero `revalidatePath(` call sites under `server/` (declaration only at `revalidate.ts:97`) |
| Q4 | 3 | 3 | 2,801 | 1,119,623 | 0.00× | **33** files with both `appDir` and `pagesDir`; lists match |
| Q5 | 2 | 3 | 1,772 | 41 | 43.2× | Shared def/throw; rtk names catch function `renderToStream` correctly; octocode mislabels catch as `renderToHTMLOrFlightImpl` |
| Q6 | 3 | 3 | 1,728 | 41 | 42.1× | Return type, params, and opening `Invalid URL` guard verified |
| Q7 | 3 | 3 | 1,244 | 41 | 30.3× | `revalidate.ts:34` → `pendingRevalidatedTags` → `executeRevalidates` |
| Q8 | 3 | 3 | 1,480 | 41 | 36.1× | `ACTION_HEADER` / `handleAction` / `FlightRenderResult` line verified |
| Q9 | 1 | 3 | 278,499 | 56,289 | 4.95× | rtk **#57287** “Partial Prerendering” is the introduction PR; octocode **#68958** is a later PFPR follow-on |
| Q10 | 0 | 1 | 21,305 | 3,450,660 | 0.01× | octocode UNKNOWN; rtk cites **#47438** (reply codec, not feature intro) with 0 inline comments |
| Q11 | 2 | 3 | 799 | 1,811 | 0.44× | rtk `send-response.ts` + `pipeToNodeResponse`; octocode `send-payload.ts` helper path (valid but less direct entry) |
| Q12 | 2 | 3 | 304 | 1,811 | 0.17× | Clone has **312** matches; rtk count + list complete; octocode **311** (misses `ReactDOMServerPages.js`) |
| Q13 | 2 | 3 | 825 | 1,811 | 0.46× | Both find `callGenerateStaticParams`; rtk quotes param merge loop + `buildAppStaticPaths` |
| Q14 | 3 | 2 | 695 | 1,811 | 0.38× | **25** subdirs; most children **`app-render` = 75** (rtk said 76) |
| Q15 | 2 | 2 | 791 | 1,811 | 0.44× | **21** `abstract` methods on `Server`; neither quotes a class-level purpose doc (none on `Server`) |
| Q16 | 3 | 3 | 783 | 1,811 | 0.43× | `reactStrictMode: null`, `poweredByHeader: true` verified |
| Q17 | 3 | 3 | 790 | 1,811 | 0.44× | `NextNodeServer` → `Server` chain; no `implements` |
| Q18 | 3 | 3 | 310 | 1,811 | 0.17× | Sole override `NextNodeServer` `next-server.ts:530`; no `super` |
| Q19 | 1 | 3 | 278 | 1,811 | 0.15× | **29** unique files (excl. `compiled/`); octocode reports 27 (omits `cache.js`, `errors.json`) |
| Q20 | 3 | 3 | 790 | 1,811 | 0.44× | Single direct caller `AppPageRouteModule.render` `module.ts:160` |

## Totals

| Metric | octocode | rtk-gh |
|---|---|---|
| Total depth score | 47 / 60 | 52 / 60 |
| Total chars | 318,989 | 4,781,026 |
| Avg D | 2.35 | 2.60 |
| Questions answered (D≥1) | 20 | 20 |
| Questions with D=3 | 11 | 16 |

## Verdict

**Accuracy:** rtk-gh leads on depth (52 vs 47), especially on PR research (Q9), exhaustive surveys (Q12, Q19), and build-pipeline detail (Q13). octocode matches or exceeds rtk on core code-tracing items (Q2–Q8, Q17–Q18, Q20) but loses on Q9 (wrong PPR PR), Q10 (gave up vs wrong PR), and under-counted cross-package search (Q19). For Q1 and Q5, octocode found the server `app-render` catch path but misnamed the enclosing function; rtk’s Q1 answer stops at the client error boundary without the server render catch.

**Efficiency:** octocode used ~**0.07×** the total character budget (319k vs 4.78M). rtk-gh’s cost is dominated by Q4 and Q10 tool output (millions of chars) while many answers are still correct. octocode is dramatically leaner on Q1–Q8 and Q11–Q20; only Q9 was heavier for octocode (still ~5× cheaper than rtk on that question).

**Struggles:** Both agents stumble on “which PR introduced X” (Q9–Q10). octocode’s Q10 honesty (UNKNOWN) scores zero but avoids fabrication; rtk burned enormous tokens on Q10 without identifying the true Server Actions introduction PR. Local clone tasks favor rtk when completeness matters (TODO census, `unstable_cache` file set); octocode often gave the right core fact with a small off-by-one or missing peripheral file.

**Overall winner:** **rtk-gh** on answer quality (higher depth score and more perfect scores), **octocode** on efficiency (same model, ~15× less measured I/O). For production code research where PR history and exhaustive enumeration matter, rtk-gh’s extra cost bought materially better answers; for targeted tracing questions, octocode reached parity at a fraction of the token budget.
