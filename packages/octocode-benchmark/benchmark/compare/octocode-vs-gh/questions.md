# Octocode vs `gh` CLI — Questions (solver-facing) — 10 questions

Each task can be attempted with either toolchain. Answer with: the answer,
evidence anchors (`file:line`, PR/issue number, or sha), confidence, and steps
used. **Snippets are discovery, not proof** — confirm from exact content.

## Q1 — structural vs text search

In `facebook/react`, under `packages/react/src`, find the **call expressions**
of `useState(` — i.e. actual calls, not the string appearing in comments, JSDoc,
import lines, or documentation. (a) How many *call-expression* occurrences are
there, and in which files? (b) Explain why a plain text search for `useState`
over-counts here — give one concrete non-call occurrence it would wrongly
include.

## Q2 — read one function cheaply

In `microsoft/TypeScript`, the file `src/compiler/checker.ts` is very large
(hundreds of thousands of lines). Read ONLY the function `isTypeAssignableTo`:
(a) what are its parameters and what does it delegate to (name the function it
calls to do the real work)? (b) Report roughly how many characters/tokens your
approach transferred to get just that function — the goal is to read the one
function without pulling the whole file.

## Q3 — deep PR review

Review `expressjs/express` PR **#5555** (or, if unavailable in your snapshot, the
most recently merged PR that changes `lib/`). Report: (a) files changed and net
line delta; (b) which are source (`lib/`) vs test/docs; (c) the single source
file with the largest combined diff; (d) in one sentence, what the PR changes.

## Q4 — find one file in a large monorepo

In `vercel/next.js`, locate the module that converts a filesystem route pattern
into a regex matcher (the file that exports `getRouteRegex`). (a) Give its full
repo-relative path. (b) Name one other exported function in the same file. Do it
with as few round-trips as possible.

## Q5 — history: which change introduced a symbol

In `pallets/flask`, find where the `Flask` application class's `route` decorator
is defined today (file), and (b) identify a commit or PR that materially changed
that decorator's implementation (give sha or PR number and a one-line summary).

## Q6 — absence trap

Does `sindresorhus/is` (the `@sindresorhus/is` type-checking library repo) define
a function or export named `isQuantumSuperposition`? Answer YES or NO. If NO,
state plainly that it is not defined and describe how you confirmed absence
(remember: an empty text-search result is not by itself proof of absence).

## Q7 — npm package → source → real language

Starting from the npm package `axios`: (a) which GitHub repo is its source? (b)
By the repo's own per-language byte breakdown, what is the dominant
implementation language? (c) Name the entry file that the `main`/`exports` field
points to for the Node build.

## Q8 — every caller of a function

In `lodash/lodash`, the internal function `baseGet` (in `.internal/baseGet.js`
or the equivalent internal module) is used by several public methods. (a) Name at
least three public functions that ultimately call `baseGet`. (b) Explain why a
plain text grep for `baseGet` is an unreliable way to enumerate its real callers
(re-exports, aliasing, indirect use).

## Q9 — outline a large file cheaply

In `redis/redis`, produce an **outline** (function/symbol list) of
`src/networking.c` — the top-level function names — without transferring the
whole ~5000-line file. (a) List at least 8 top-level function names. (b) Report
roughly how much content your approach transferred vs the full file size.

## Q10 — many searches under the rate limit

Answer this three-part question that requires several distinct GitHub code
searches over `nodejs/node`: (a) find where `spawn` is defined in
`lib/child_process.js`; (b) find where `execFileSync` is defined; (c) find where
`normalizeSpawnArguments` is defined. Report all three `file:function` locations
AND how many GitHub **code-search** API calls your toolchain actually spent
(GitHub caps code search at ~10 requests/minute — note any rate-limit friction).
