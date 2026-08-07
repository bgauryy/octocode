# Example verdict (calibration)

An illustrative `judge/Q<n>.md` for one question, showing the bar and the shape. Numbers and
answers are fabricated for calibration — copy the **structure and rigor**, not the values.
Reasoning comes first; each score is a one-sentence summary of it.

---

**Question (paraphrased):** In `vercel/next.js` on `canary`, name the file that defines the
exported `getRouteRegex()`, the internal helper it calls to parameterize the route, and the
top-level fields it returns.

**Judge model:** `<neutral-model-id>` · **Label mapping (out of band):** X=?, Y=?, Z=?
(randomized) · **Refs frozen:** `canary` @ `<SHA>`, UTC `<ts>`.

## Ground truth (built before reading any answer)

| Fact asked | Value | Evidence |
|---|---|---|
| Defining file | `packages/next/src/shared/lib/router/utils/route-regex.ts` | exact read, `export function getRouteRegex(` |
| Internal helper | `getParametrizedRoute(...)` | same file, called on line N |
| Returned top-level fields | `{ re, groups, routeKeys }` | `return { re: ..., groups, routeKeys }` |

## Per answer

**X** — Names the correct file and `getParametrizedRoute` as the helper, and lists `re`,
`groups`, `routeKeys`. Opened its cited line for `routeKeys` — it supports the claim.
Path was two targeted reads.
- Correctness **10** — every material part correct, each citation verified.
- Depth **5** — every claim tied to an exact line I re-opened.
- Workflow **5** — two targeted region reads, no waste.
- Chars **4,210** (5 calls).

**Y** — Correct file and fields, but names the helper as `buildRouteRegex`, which does not
exist at this SHA; the real helper is `getParametrizedRoute`. Its other citations check out.
- Correctness **4** — a material part (the helper) is wrong/unsupported.
- Depth **3** — grounded except the fabricated helper name.
- Workflow **4** — mostly lean; one redundant tree view.
- Chars **6,880** (7 calls).

**Z** — Correct file and helper, but omits `routeKeys` from the returned fields and says
"returns `re` and `groups`." Missing a material part, not wrong on it.
- Correctness **7** — core correct, one required field missing.
- Depth **4** — solid on what it covered.
- Workflow **5** — lean; single symbol read.
- Chars **3,050** (3 calls).

## Ranking (correctness first)

1. **X** — only answer correct and fully verified on all three parts.
2. **Z** — correct as far as it goes; missed `routeKeys` (a required field), and leaner than Y.
3. **Y** — a fabricated helper name is a confidently-wrong material part; cannot outrank a
   complete answer regardless of footprint.

No decisive-win confirmation needed here (X is unambiguous). Had X vs Z been close on
correctness, I would swap order and re-judge before crediting the char tiebreak.
