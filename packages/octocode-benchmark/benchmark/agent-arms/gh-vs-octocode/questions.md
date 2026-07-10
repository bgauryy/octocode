# GitHub Research Questions (solver-facing)

Pure remote research — every answer lives on GitHub or the npm registry.
Answer each with: the answer, evidence anchors (path:line / sha / PR# / URL),
confidence (high/medium/low), and number of research steps used.

## Q1 — code search
In `vercel/next.js` (canary/default branch): which file DEFINES the function
`getSortedRoutes` (not re-exports or callers)? Full repo path.

## Q2 — exact read
In `sindresorhus/p-limit` (default branch): which file implements the limiter,
and which queue package does it import?

## Q3 — repo discovery
Find the most-starred GitHub repository with topic `state-management` whose
primary language is TypeScript. Report owner/repo and approximate star count.

## Q4 — structure browse
In `tokio-rs/tokio` (default branch): name the workspace crates that live as
top-level directories of the repo (the `tokio*` ones). How many are there?

## Q5 — PR archaeology
In `facebook/react`: which pull request added the file
`packages/use-sync-external-store/src/useSyncExternalStoreShimClient.js`?
Give the PR number and its author.

## Q6 — commit history
In `nodejs/node`: find the commit that first added `lib/test.js` (the `node:test`
module entry). Give sha (short ok), author, and date.

## Q7 — cross-repo comparison
Both `pmndrs/zustand` and `pmndrs/jotai` ship a primary React hook. Give the
file path in EACH repo that defines it (`useStore` in zustand, `useAtom` in
jotai).

## Q8 — defaults hunting
In `vitest-dev/vitest` (main): what is the default value of `teardownTimeout`,
and which file defines that default? Path + line if possible.

## Q9 — releases
What is the latest release tag of `microsoft/TypeScript`, and on what date was
it published?

## Q10 — remote root-cause read
In `axios/axios` (default branch): which file defines the DEFAULT
`transformRequest` (the one that JSON-stringifies plain objects), and at what
approximate line does the function start?
