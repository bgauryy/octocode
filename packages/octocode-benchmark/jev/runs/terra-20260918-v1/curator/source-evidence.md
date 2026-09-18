# Curator evidence ledger — frozen before arm review

All research invocations are logged under the per-case curator directories. The references below are original repository/issue anchors, pinned to the resolved source ref where available.

| Case | Primary evidence |
| --- | --- |
| Q1 | `vercel/next.js` canary `3bf71ee3`: `packages/next/src/shared/lib/router/utils/route-regex.ts`, lines 149–166. |
| Q2 | `sindresorhus/is` main `7821031`: repository tree language/branch metadata, `package.json`, and `source/index.ts` exact zero-match for the alleged export. |
| Q3 | Current `pallets/flask` `d73fa1c`: `src/flask/sansio/scaffold.py`; historical `705e52684`: changed `src/flask/scaffold.py` and test patch. |
| Q4 | `axios/axios` `56a5f1a`: `package.json`, `lib/adapters/http.js`; `follow-redirects/follow-redirects` `0c23a22`: `index.js`. |
| Q5 | `vuejs/core` PR #15035 changed-file patches and source SHA `2c2b92d`. |
| Q6 | `expressjs/express` `9a34acf`: `package.json`, `lib/application.js`; `pillarjs/router` `bda4af3`: `index.js`, `lib/layer.js`. |
| Q7 | `vercel/next.js` `3bf71ee`: example store; `pmndrs/zustand` `b57db4f`: root package manifest. |
| Q8 | `microsoft/vscode` `265cdf2`: workbench class and abstract service dispatch method. |
| Q9 | `fastify/fastify` `630acd0`: lifecycle documentation and `lib/route.js`. |
| Q10 | `axios/axios` `56a5f1a`: manifest, `index.js`, `lib/axios.js`; language metadata was separately returned from a v1.x tree and is explicitly not used to pin the manifest. |
| BUG | Issue #37655 (allegations) and `react/react` `71f7255` `ReactFiberHooks.js` (actual mechanism evidence). |

The BUG case was not executed as a React runtime reproduction. The issue narrative supplies a conceptual trace; the frozen requirements intentionally distinguish that from source verification and from an executed regression test.
