# Octocode vs gh — Questions

## Q1

In `vuejs/core`, under `packages/reactivity/src`, find the call expressions of
`ref(<args>)`. (a) How many are there and in which files? (b) Give one non-call
occurrence of the token `ref` in those files.

## Q2

In `microsoft/TypeScript`, in `src/compiler/checker.ts`, read the function
`isTypeAssignableTo`. (a) What are its parameters and which function does it
delegate to? (b) How many characters did you transfer to obtain just that
function?

## Q3

Review the most recently merged `expressjs/express` PR that changes `lib/`. (a)
Files changed and net line delta. (b) Which are source (`lib/`) vs test/docs. (c)
The single source file with the largest combined diff. (d) What the PR changes,
in one sentence.

## Q4

In `vercel/next.js`, find the file that exports `getRouteRegex`. (a) Its full
repo-relative path. (b) One other exported function in the same file.

## Q5

In `pallets/flask`: (a) the file where the application class's `route` decorator
is defined; (b) a commit or PR that materially changed that decorator, with sha
or PR number and a one-line summary.

## Q6

Does `sindresorhus/is` define a function or export named
`isQuantumSuperposition`? Answer YES or NO, and how you determined it.

## Q7

For the npm package `axios`: (a) its GitHub source repo; (b) the dominant
implementation language by the repo's per-language byte breakdown; (c) the entry
file the Node build's `main`/`exports` field resolves to.

## Q8

In `lodash/lodash`, the internal function `baseGet`: (a) name at least three
public functions that ultimately call it; (b) why is a text search for `baseGet`
an unreliable way to enumerate its callers?

## Q9

In `redis/redis`, `src/networking.c`: (a) list at least 8 top-level function
names; (b) how much content did you transfer versus the full file size?

## Q10

In `nodejs/node`, `lib/child_process.js`: give the `file:function` location where
each of `spawn`, `execFileSync`, and `normalizeSpawnArguments` is defined, and
how many GitHub code-search calls your toolchain spent.
