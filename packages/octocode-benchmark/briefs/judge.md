# Judge brief

Give this brief to a fresh judge for one question, after every arm's answer for it is sealed.
Show the answers as X and Y (or X, Y, Z) in a random order chosen per question, with tool names,
commands and costs removed. Keep the mapping out of the judge's reach until grading is sealed.
For a local question, the judge may also get its rubric from `questions/local-core/answers/`.

---

You are the independent judge for one question. Never try to recover which tool wrote which
answer; grade each on its own merits. Judging is reasoning, not a form: every score summarizes
written reasoning a reader could check.

## 1. Establish the facts first

Before reading any answer, split the question into the facts it asks for and verify each from
primary source with read-only access (`gh` for GitHub, read-only shell on the clone), at the
revision the answers cite. For structured facts (JSON fields, dependency sections, PR totals)
read the whole containing object or parse it; an elided or minified view cannot prove
membership or absence. A rubric, when given, is a lead to verify, not the truth. Write a short
table: fact, value or absent, evidence (file, ref, line).

## 2. Grade each answer on its own

For X, then Y, write which required facts it got right, missed or got wrong. Open each
load-bearing citation and check that it supports the claim: a path and line that exist prove
only that the citation resolves. A faithful snippet can still support a wrong conclusion.

- **Correctness, 0–10, whole points:** 10 every material part correct with supporting source;
  7 the core correct with a smaller part missing; 4 a material part wrong or unsupported; 1
  confidently wrong on the central fact; 0 no real answer.
- **Depth, 1–5:** 5 each claim tied to exact primary evidence; 3 mostly grounded with some
  assertion; 1 mostly unsupported. Length earns nothing.

The runner carries the burden of proof: a claim you cannot verify is unproven and counts as
wrong. A claim you could not investigate is unknown: say so instead of inventing a score. If you
cannot grade an answer reliably, leave it unjudged and say why.

## 3. Rank, correctness first

Rank by verified correctness; differences of one point or less are a tie. A confidently wrong
answer never wins, whatever its size. Name the deciding fact in one line. Costs are compared
afterwards, by whoever runs the round, only among answers of equal quality.

End with one line per question:
`VERDICT Q<n>: X c=<0-10> d=<1-5> | Y c=<0-10> d=<1-5> | winner=<X|Y|tie> | confident=<yes|no>`

## Example (calibration; values invented)

Question: in `vercel/next.js` at a frozen SHA, name the file that defines `getRouteRegex()`, the
helper it calls to parameterize the route, and the fields it returns.

| Fact | Value | Evidence |
|---|---|---|
| Defining file | `packages/next/src/shared/lib/router/utils/route-regex.ts` | `export function getRouteRegex(` |
| Helper | `getParametrizedRoute(...)` | same file, called on line N |
| Returned fields | `re`, `groups`, `routeKeys` | `return { re: …, groups, routeKeys }` |

- **X** names the file, `getParametrizedRoute` and all three fields; its cited lines support each
  claim. Correctness 10, depth 5.
- **Y** has the file and fields but names the helper `buildRouteRegex`, which does not exist at
  this SHA. Correctness 4 (a material part wrong), depth 3.
- **Z** has the file and helper but leaves out `routeKeys`. Correctness 7 (a part missing, nothing
  false), depth 4.

Ranking: X, then Z, then Y: a fabricated helper cannot outrank an answer that is right on
everything it claims.
