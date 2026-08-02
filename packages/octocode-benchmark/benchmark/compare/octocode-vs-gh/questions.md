# Octocode vs gh — Questions

## Q1

In `vuejs/core`, under `packages/reactivity/src`, find the call expressions of
`ref(<args>)`. (a) How many are there and in which files? (b) Give one non-call
occurrence of the token `ref` in those files. (c) State how your method
distinguished real call expressions from JSDoc/type/declaration mentions, and
what a naive text search for `ref(` would have reported instead (give its raw
hit count).

## Q2

In `microsoft/TypeScript`, in `src/compiler/checker.ts`, read ONLY the region of
the function `isTypeAssignableTo` — the file is multi-MB and transferring it
whole scores poorly. (a) What are its parameters and which function does it
delegate to? (b) How many characters did you transfer to obtain just that
function, versus the full file size?

## Q3

Review the most recently merged `expressjs/express` PR that changes `lib/`. (a)
Files changed and net line delta. (b) Which are source (`lib/`) vs test/docs. (c)
The single source file with the largest combined diff. (d) What behavior or bug
motivated the change — ground it in the diff hunks and/or the linked
issue/discussion (cite it), not the PR title alone.

## Q4

In `expressjs/express` (current default branch), trace how a request is routed:
(a) Does the layer-matching loop that drives `next()` live inside the express
repo itself? If not, name the npm package and GitHub repo that actually contain
it, and cite the `package.json` dependency that proves the link. (b) In that
code, give the function that advances to the next matching layer (name, file,
approx line) and the helper that tests whether a single layer matches the
current path (name, file). (c) Why would a text search for `matchLayer` inside
`expressjs/express` mislead you?

## Q5

In `pallets/flask`: (a) the file where the application class's `route`
decorator is defined on the CURRENT default branch — trace the class hierarchy
if it is not on the `Flask` class itself, and note if the file has moved over
the project's history; (b) a commit or PR that materially changed that
decorator or its delegate `add_url_rule`, with sha or PR number and a one-line
summary you verified against the actual diff (not the commit title alone);
(c) go deeper in history: the commit (sha + date) that MOVED the decorator to
its current file — recent history won't contain it; expect to page further
back than a first results page.

## Q6

(a) Without relying on memory, discover via GitHub repository search the
most-starred repository owned by `sindresorhus` whose package is the
type-checking utility named `is` — cite the star count you observed. (b) Does
that repo define a function or export named `isQuantumSuperposition`? Answer
YES or NO, and how you determined it — an empty search alone is not proof of
absence.

## Q7

For the npm package `axios`: (a) its GitHub source repo; (b) the dominant
implementation language by the repo's per-language byte breakdown; (c) trace the
Node entry resolution: what `main` points to, what the `exports` map resolves
`require` from Node to, and which real source file under `lib/` the resolved
entry ultimately wraps or re-exports.

## Q8

In `lodash/lodash`, the internal function `baseGet`: (a) name at least three
public functions that ultimately call it; (b) why is a text search for `baseGet`
an unreliable way to enumerate its callers?

## Q9

In `redis/redis`, `src/networking.c`: (a) list at least 8 top-level function
names; (b) which two functions have the largest line spans (name + approx
span); (c) how much content did you transfer versus the full file size to
answer (a) and (b)?

## Q10

In `nodejs/node`, `lib/child_process.js`: give the `file:function` location where
each of `spawn`, `execFileSync`, and `normalizeSpawnArguments` is defined, and
how many GitHub code-search calls your toolchain spent.
