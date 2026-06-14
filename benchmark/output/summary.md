# Benchmark Summary — octocode vs rtk-gh

Two agents — **octocode** (MCP-based remote search, claude-sonnet-4-5) and **rtk-gh** (local clone + `gh` CLI, claude-sonnet-4-6) — each answered 10 questions about `vercel/next.js`, covering multi-hop code tracing, symbol lookup, cache-invalidation architecture, Server Action routing, repo/workflow discovery, and one drift search question. **octocode leads on total research score** (73 vs 63 out of 81 possible) and wins the **tradeoff score by ~35×** (0.47 vs 0.013 research-points per 1 k-chars). rtk-gh won more per-question tradeoffs on the code-tracing group (Q2, Q5–Q8) thanks to targeted local searches, but was zeroed out on Q9 and Q10 because it ran against a different questions file and answered different questions entirely; Q4 accumulated 1.1 M chars of leaked research, further collapsing its overall efficiency.

---

## Per-Question Table

> Q3 is marked **[drift]** — excluded from quality/efficiency tallies. Scores prefixed `d:`.
> rtk-gh Q9 and Q10 answered **different questions** (PPR intro PR and Server Actions inline-review PR from an older questions file); scored Q=0/D=0 against the current benchmark questions.

| Q | Category | Drift | octocode Q | octocode D | octocode T | octocode chars | octocode tradeoff | rtk-gh Q | rtk-gh D | rtk-gh T | rtk-gh chars | rtk-gh tradeoff | Winner | Notes |
|---|----------|-------|-----------|-----------|-----------|---------------|------------------|---------|---------|---------|-------------|----------------|--------|-------|
| Q1 | Code tracing | | 3 | 3 | 4 | 17,363 | 0.52 | 3 | 3 | 5 | 71,724 | 0.13 | **octocode** | Both correct: `not-found.ts:23`, `throw error` with `HTTPAccessFallbackError` digest, `HTTPAccessFallbackErrorBoundary.getDerivedStateFromError`; octocode 4× cheaper |
| Q2 | Symbol lookup | | 3 | 3 | 2 | 11,567 | 0.78 | 3 | 3 | 3 | 842 | 10.69 | **rtk-gh** | Both correct: `request.ts:14`, `response.ts:36`, `image-response.ts:19`; rtk-gh 14× cheaper with targeted `rg` |
| Q3 | Search | ✓ | d:3 | d:3 | 7 | 52,463 | — | d:3 | d:3 | 11 | 64,307 | — | — | Zero `revalidatePath(` call sites under `server/`; declaration only at `revalidate.ts`; both verified |
| Q4 | Search (file set) | | 3 | 3 | 2 | 14,541 | 0.62 | 3 | 3 | 34 | 1,119,623 | 0.008 | **octocode** | Both correct: 33 files with `appDir` + `pagesDir`; rtk-gh log inflated (Q5–Q8 research leaked under Q4's question number) |
| Q5 | Code tracing | | 2 | 2 | 4 | 16,410 | 0.24 | 3 | 3 | 5 | 4,559 | 1.97 | **rtk-gh** | Def `redirect.ts:38` ✓ both; throw `getRedirectError(...)` ✓ both; octocode names catch as "`renderToHTMLOrFlight`" (wrong — actual enclosing function is `renderToStream` at line 3147) |
| Q6 | Targeted read | | 3 | 3 | 4 | 13,593 | 0.66 | 3 | 3 | 4 | 1,294 | 6.96 | **rtk-gh** | Return type `Promise<RenderResult<AppPageRenderResultMetadata>>`, all 8 params, `if (!req.url) throw new Error('Invalid URL')` verified; rtk-gh 11× cheaper |
| Q7 | Architecture tracing | | 3 | 2 | 4 | 15,665 | 0.38 | 3 | 3 | 4 | 3,523 | 2.55 | **rtk-gh** | octocode line 36 (off: verified 34); octocode consumer `incremental-cache/index.ts get()` valid but less direct than rtk-gh's `executeRevalidates` in `revalidation-utils.ts`; rtk-gh deeper and 4× cheaper |
| Q8 | Cross-layer tracing | | 3 | 3 | 7 | 28,153 | 0.32 | 3 | 3 | 7 | 3,339 | 2.70 | **rtk-gh** | `ACTION_HEADER='next-action'` at `app-router-headers.ts:2`, `handleAction` at `action-handler.ts:538`, `return new FlightRenderResult(response.body!)` at line 281; same quality, rtk-gh 8× cheaper |
| Q9 | Repo discovery | | 3 | 3 | 5 | 12,869 | 0.70 | 0 | 0 | 22 | 56,289 | 0 | **octocode** | octocode: `vercel/next.js/evals/`, README quoted, PROMPT.md/EVAL.ts/app/ structure, eval IDs by category; rtk-gh: answered PPR intro PR (#57287) — wrong question (different questions file) |
| Q10 | Workflow discovery | | 3 | 3 | 11 | 23,668 | 0.38 | 0 | 0 | 129 | 3,450,660 | 0 | **octocode** | octocode: `.github/workflows/turbopack-benchmark.yml`, trigger YAML quoted, 3 jobs with Cargo targets; rtk-gh: answered Server Actions inline-review PR — wrong question; 3.4 M chars wasted |

---

## Research Quality Verdict (non-drift Qs only)

| Agent | Σ Q | Σ D | Σ research_score | Avg Q/Q | Avg D/Q | Tradeoff wins | Tradeoff ties |
|-------|-----|-----|-----------------|---------|---------|--------------|--------------|
| octocode | 26 | 25 | 73 | 2.89 | 2.78 | 4 | 0 |
| rtk-gh | 21 | 21 | 63 | 2.33 | 2.33 | 5 | 0 |

octocode leads on total research score (73 vs 63) — its wins on Q9 and Q10, where it answered the correct questions completely, contribute 18 research points that rtk-gh surrendered entirely. On the seven code-tracing and lookup questions (Q1–Q8), rtk-gh matches or exceeds octocode on Q2 and Q5–Q8 by using tight local `rg`/`read` calls that produce dense, correct answers at far lower char cost; octocode lost Q5 depth by misidentifying the catch function and Q7 depth by a small line-number offset. Neither agent made fabrications — octocode's Q5 error was a mislabeled function name, not invented content.

---

## Character Efficiency Verdict (non-drift Qs only)

| Axis | octocode | rtk-gh | ratio (octo / rtk) |
|------|----------|--------|-------------------|
| Σ research_score (non-drift) | 73 | 63 | octocode +16% |
| Σ calls / turns | 41 | 183 | 0.22× |
| Σ in_chars (per-Q) | 22,242 | 15,691 | 1.42× |
| Σ out_chars (per-Q) | 131,587 | 4,696,162 | 0.028× |
| Init / tool-context chars | 0 | 0 | — |
| **TOTAL effective_chars** | **153,829** | **4,711,853** | **0.033×** |
| Approx tokens (chars / 4) | 38,457 | 1,177,963 | 0.033× |
| Actual LM tokens | N/A | N/A | — |
| **tradeoff_score** | **0.474** | **0.013** | **35.5×** |
| Avg turns_per_point | 1.67 | 35.9† | — |
| Σ tool_elapsed_ms (context) | 63,672 | 96,483 | — |
| Σ q_elapsed_ms (context) | 515,016 | 152,191 | — |
| Σ reasoning_ms (context) | 451,344 | 155,708 | — |

> † rtk-gh avg turns_per_point inflated by Q9 (22 calls / Q=0) and Q10 (129 calls / Q=0).

octocode delivers 35× better research value per measured character. The efficiency gap is driven by three rtk-gh events: Q4 (1.1 M chars of leaked cross-question research), Q10 (3.4 M chars on the wrong question), and Q9 (56 k chars on the wrong question). Excluding those three anomalies, rtk-gh's per-question cost on Q1–Q3 and Q5–Q8 is actually lower than octocode's (e.g. Q6: 1,294 vs 13,593 chars for identical quality), showing that focused local-clone searches outperform remote MCP calls when the file path is known. octocode's `turns_per_point` of 1.67 reflects a methodical multi-hop strategy; rtk-gh's comparable calls on Q1–Q8 averaged ~4 calls/Q with equally tight targeting.

---

## Drift Verdict

| Q | Category | octocode Q | rtk-gh Q | Notes |
|---|----------|-----------|---------|-------|
| Q3 | revalidatePath search | d:3 | d:3 | Verified: zero `revalidatePath(` call sites inside `packages/next/src/server/`; only the declaration at `revalidate.ts` (not a call site). Both agents reported this correctly. |

---

## Depth Analysis

**Q5 — octocode D=2 (rtk-gh D=3, gap: 1)**
octocode correctly found the definition (`redirect.ts:38`), the throw (`throw getRedirectError(url, type, RedirectStatusCode.TemporaryRedirect)`), and the catch file (`app-render.tsx`), but labelled the enclosing function as "`renderToHTMLOrFlight`" when the actual catch block lives inside `async function renderToStream` (line 3147). The catch line is line 3976 (`} else if (isRedirectError(err)) {`). rtk-gh explicitly named `renderToStream` and line 3147. The shallow depth correlated directly with the wrong function name, reducing Q from 3 to 2.

**Q7 — octocode D=2 (rtk-gh D=3, gap: 1)**
octocode placed `revalidateTag` at line 36; independent verification via `rg` found it at line 34 (2-line discrepancy, likely a minor drift in the working-copy line count). More substantively, octocode cited `incremental-cache/index.ts get()` (lines 451 and 514) as the consumer, which is a valid read site — it checks `pendingRevalidatedTags` on every cache lookup. rtk-gh cited `executeRevalidates` in `revalidation-utils.ts` (line 186), which is the function that calls `revalidateTags()` to execute the actual invalidation work — the more architecturally central consumer. The depth gap did not cause a quality error (the cited consumer IS correct), but rtk-gh's answer is more informative about the cache-invalidation execution path.

**Q4 — rtk-gh D=3 but T=34 (anomaly)**
rtk-gh's 34 calls on Q4 are an artifact of Q5–Q8 research (redirect, renderToHTMLOrFlight, revalidateTag, action-handler) being executed while the question sentinel was still set to Q4. The Q4 answer (33 files with both `appDir` and `pagesDir`) is correct with verified file lists from both agents. The inflated call count does not reflect actual Q4 research depth.

---

## Capability Review

- **rtk-gh — wrong questions file on Q9/Q10**: The rtk-gh run was executed against an older 20-question file where Q9 = "Partial Prerendering introduction" and Q10 = "Inline review thread: Server Actions introduction." The current canonical `nextjs.md` has Q9 = "Official Next.js agent eval benchmark" and Q10 = "Official Turbopack benchmark workflow." rtk-gh's answers are entirely correct for the old questions but score Q=0/D=0 against the current benchmark, costing it 18 research points.

- **rtk-gh — Q4 call-count inflation**: 34 calls and 1,119,623 chars are logged under Q4 because the question sentinel was not advanced before researching Q5–Q8. The actual Q4 answer required only the two `rg -l` commands (3 calls in the original run). This makes rtk-gh's Q4 tradeoff score (0.008) artificially terrible.

- **rtk-gh — Q10 cost pathology**: 129 calls / 3.4 M chars on Server Actions PR discovery, including dozens of `gh api search/issues` and `gh api search/commits` sweeps plus per-PR body reads. This is the dominant cost in the entire run and produced a wrong answer for the current Q10.

- **octocode — Q5 function-name error**: octocode stated the catch is "within the main `renderToHTMLOrFlight` render function." Verbatim quote from answer: *"This is inside the error-recovery classification block (comment `// MARK: errorRecovery classification`) within the main `renderToHTMLOrFlight` render function in `app-render.tsx`."* The actual enclosing function is `renderToStream` (line 3147). `renderToHTMLOrFlight` is a thin wrapper that calls `renderToStream`; the catch block is one level deeper.

- **octocode — Q7 minor line drift**: octocode reported `revalidateTag` at line 36; verified at line 34. The 2-line discrepancy may reflect a shallow clone vs a slightly different file version, or a counting error in the pagination of a large file read.

---

## Verdict

**octocode wins the tradeoff score by ~35×** (0.474 vs 0.013 research-points per k-char), primarily because rtk-gh surrendered all points on Q9 and Q10 (wrong questions) and burned 4.5 M chars on those two questions alone. **rtk-gh leads on raw research score for the seven overlapping code-tracing and lookup questions** (Q1–Q8): on Q2 and Q5–Q8 it achieved D=3 with targeted `rg`/`read` calls costing 1/8 to 1/14 of octocode's character budget, demonstrating that a focused local-clone search strategy outperforms remote MCP calls for precise file-line lookups. octocode's efficiency advantage is structural: its bulk-query MCP tool amortizes per-call overhead and returns dense results, but the total per-question output is larger; rtk-gh's `read` of a full 312 k-char file hurt Q4–Q6 costs while `rg` line-targeted searches on Q5–Q8 were extremely lean. **turns_per_point** reveals a strategic difference: octocode averaged 1.67 turns per research point (systematic multi-hop), while rtk-gh used ~1.4 turns/point on valid questions and burned unbounded turns on invalid ones (Q9: 44, Q10: 258 turns/point against current questions).
