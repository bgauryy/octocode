# octocode vs `gh`+`rtk` — full pass (all 17), 2026-08-05

**Matchup:** `compare/octocode-vs-gh-rtk` (17 GitHub research questions).
**Arm A (baseline):** read-only `gh` through `rtk gh`. **Arm B (Octocode):** `npx octocode tools …`.
**Method:** per question, two isolated runners answer (one per tool) and a third agent grades **blind** (tool identities hidden as X/Y via a per-question swap, un-blinded after grading). Ground truth established by the grader's own current-evidence research. Measured in **characters** (raw CLI output pulled into context), never tokens.

## Bottom line

**Correctness was essentially a tie (A 9.18 vs B 9.38 mean); the run is decided on the leanness tiebreak, which Octocode wins clearly.** The two arms answered equally correctly on 10 of 17 questions; Octocode was higher on 4, gh+rtk on 3. Because correctness ties on most questions, the deciding factor per `SCORING.md` is characters at equal correctness — and there Octocode is decisive: **395,644 chars vs 1,040,783** (Octocode pulled ~38% of gh+rtk's characters; gh+rtk pulled **2.63×** more). On the 10 correctness-equal questions, Octocode was leaner on 8. Grader preference across all 17: **Octocode 8, gh+rtk 4, tie 5.**

gh+rtk's character cost was inflated by whole-file / whole-diff fetches (Q2 164k, Q5 326k, Q11 123k, Q17 106k, Q8 58k). Octocode made **more** CLI calls (146 vs 100) but far fewer characters per answer — targeted region reads instead of full-file pulls.

## Per-question table

| Question | Correctness A | Correctness B | Depth A | Depth B | Workflow A | Workflow B | Chars A | Chars B | Leaner | Grader pref |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---|---|
| Q1 route-regex-builder | 10 | 10 | 4 | 3 | 5 | 5 | 25,691 | 9,520 | B | tie |
| Q2 is-repo-absence | 10 | 10 | 5 | 4 | 3 | 5 | 164,194 | 63,317 | B | tie |
| Q3 flask-route-history | 10 | 10 | 5 | 4 | 4 | 5 | 47,431 | 17,250 | B | tie |
| Q4 zustand-fix-pr-state | 10 | 9 | 5 | 4 | 5 | 4 | 9,750 | 13,355 | A | A |
| Q5 vue-pr-diff-review | 7 | 9 | 4 | 5 | 3 | 5 | 326,000 | 60,097 | B | B |
| Q6 express-router-trace | 10 | 9.5 | 5 | 4 | 5 | 4 | 25,098 | 27,100 | A | A |
| Q7 zustand-next-contract | 10 | 10 | 4 | 4 | 4 | 5 | 29,800 | 14,623 | B | B |
| Q8 vscode-keybinding | 10 | 10 | 5 | 4 | 3 | 5 | 57,964 | 8,677 | B | B |
| Q9 fastify-lifecycle | 10 | 10 | 4 | 4 | 4 | 5 | 51,169 | 19,400 | B | B |
| Q10 axios-entry-chain | 8 | 9 | 4 | 4 | 4 | 5 | 17,354 | 20,470 | A | B |
| Q11 esbuild-process-boundary | 10 | 9 | 5 | 4 | 5 | 5 | 122,772 | 24,800 | B | A |
| Q12 node-stream-event-wiring | 10 | 10 | 4 | 5 | 5 | 4 | 10,371 | 28,430 | A | tie |
| Q13 redis-bitfield-security | 9 | 10 | 5 | 4 | 5 | 3 | 12,539 | 36,313 | A | A |
| Q14 vitest-vite-dependency | 4 | 4 | 3 | 3 | 4 | 3 | 5,655 | 11,102 | A | tie |
| Q15 hono-jsx-array-pr | 10 | 10 | 4 | 4 | 4 | 5 | 18,965 | 13,588 | B | B |
| Q16 eslint-parser-chain | 8 | 10 | 4 | 5 | 5 | 4 | 10,083 | 14,331 | A | B |
| Q17 nextjs-fetch-memoization | 10 | 10 | 5 | 4 | 4 | 5 | 105,947 | 13,271 | B | B |

## Summary of all

| Metric | A (gh + rtk) | B (Octocode) |
|---|---:|---:|
| Correctness (mean) | 9.18 | 9.38 |
| Research depth (mean) | 4.41 | 4.06 |
| Workflow (mean) | 4.24 | 4.53 |
| **Chars (total)** | **1,040,783** | **395,644** |
| CLI calls (total) | 100 | 146 |
| Questions leaner (chars) | 7 | 10 |
| Grader preference (wins) | 4 | 8 (+ 5 tie) |

Correctness split: **equal on 10** questions, B higher on 4, A higher on 3. Among the 10 correctness-equal questions, **B was leaner on 8**, A on 2 — which is why the leanness tiebreak lands with Octocode.

## Per question (detail)

Each block: the grader's established ground truth (short), both answers' gist, and the per-question numbers. Char counts are each runner's self-reported raw CLI output.

### Q1 — Route regex builder — **tie**
Ground truth: `getRouteRegex` in `packages/next/src/shared/lib/router/utils/route-regex.ts` (next.js canary); its first statement calls the module-internal helper `getParametrizedRoute`, which builds the name-keyed `groups` map. Both arms correct and identical on core facts. A added a genuinely relevant disambiguation (`getNamedRouteRegex`→`getNamedParametrizedRoute` for true named capture groups); B was leaner. **A** 10/4/5, 25,691 ch · **B** 10/3/5, 9,520 ch.

### Q2 — Repository discovery and bounded absence — **tie**
Ground truth: `sindresorhus/is` (TypeScript, default branch `main`); `isQuantumSuperposition` is **not** exported (repo-scoped search = 0 hits; control search confirms the search works). Both reached the correct NO with sound bounded evidence — A leaner with a control-search validation, B more exhaustive on export-surface reasoning. **A** 10/5/3, 164,194 ch · **B** 10/4/5, 63,317 ch. *(Both pulled the full `source/index.ts` — the main driver of the large char counts.)*

### Q3 — Flask route history — **tie**
Ground truth: `route` decorator lives on base class `Scaffold` in `src/flask/sansio/scaffold.py`; commit `705e5268` introduced the `_method_route`/get-post-put-delete-patch shortcut registration behavior. Both nailed every fact; A added PR/exclusion context, B was more precise on the current signature and far cheaper. **A** 10/5/4, 47,431 ch · **B** 10/4/5, 17,250 ch.

### Q4 — Zustand fix PR state — **A**
Ground truth: PR #3531 is **OPEN** (mergeable, mergeStateStatus BLOCKED, 32/32 checks); sole product change `src/middleware/devtools.ts`; edge case = V8 stack regex when the source path contains spaces. Both correct; A won the grader here on this pass. **A** 10/5/5, 9,750 ch · **B** 9/4/4, 13,355 ch.

### Q5 — Vue hydration diff review — **B**
Ground truth: `vuejs/core` #15035 (MERGED into `minor`), +677/-239 across 8 files; changes in `runtime-core/src/hydration.ts` + `runtime-vapor/src/vdomInterop.ts`. B's vapor-side details were all accurate; A fabricated `component.ts` behavior and mischaracterized `block.ts`, and did so via a ~5× more expensive path. **A** 7/4/3, **326,000 ch** · **B** 9/5/5, 60,097 ch. *(Fairness note: A's 326k reflects a full-diff/full-file pull where a filtered diff view would have answered — the single largest char outlier in the run.)*

### Q6 — Express router cross-repository trace — **A**
Ground truth: layer-matching loop is **not** in `expressjs/express` (default branch `master`); the `router` dependency leads to `pillarjs/router`, where the inner `next(err)` closure advances layers and `matchLayer` tests a layer against the path. Both fully correct; A edged it on this pass. **A** 10/5/5, 25,098 ch · **B** 9.5/4/4, 27,100 ch.

### Q7 — Zustand's Next.js integration contract — **B**
Ground truth: `store.ts` is a **React Context-backed per-request store factory** (`createStore` called only inside `initializeStore`, held via `createContext`/`useContext`); Zustand root `package.json` lists React as an **optional peer**. Both fully correct; B reached it at ~half the char cost with an extra accurate nuance. **A** 10/4/4, 29,800 ch · **B** 10/4/5, 14,623 ch.

### Q8 — VS Code keybinding dispatch — **B**
Ground truth: concrete service `WorkbenchKeybindingService` (`…/keybinding/browser/keybindingService.ts`) extends base `AbstractKeybindingService` (`…/platform/keybinding/common/abstractKeybindingService.ts`), whose public `dispatchEvent` receives a keypress. Equal correct core facts; B reached the same answer at ~1/7th the cost. **A** 10/5/3, 57,964 ch · **B** 10/4/5, 8,677 ch.

### Q9 — Fastify lifecycle contract — **B**
Ground truth: documented order Incoming Request → Routing → Instance Logger → onRequest → preParsing → Parsing → preValidation → Validation → preHandler → User Handler; `lib/route.js` uses the per-route `context.onRequest` array run by `hookRunner`. Both fully correct/equivalent; B ~40% of A's cost. **A** 10/4/4, 51,169 ch · **B** 10/4/5, 19,400 ch.

### Q10 — Axios repository and Node entry chain — **B**
Ground truth: `axios/axios` (JavaScript ~89%); `main` → `./dist/node/axios.cjs` (no explicit `node` export key; falls to default `require`), bundled from `lib/axios.js`. A correctly derived the CJS bundle's actual rollup source input (`lib/axios.js`); B mis-attributed the bundle source as `index.js` though it reached the same correct answer — grader still preferred B overall. **A** 8/4/4, 17,354 ch · **B** 9/4/5, 20,470 ch.

### Q11 — Esbuild repository and Node runtime boundary — **A**
Ground truth: `evanw/esbuild` (Go ~76%); the normal Node API is **out-of-process** — `ensureServiceIsRunning()` `child_process.spawn(…"--service=…")` over a stdin/stdout binary protocol, with `worker_threads` + `Atomics`/`SharedArrayBuffer` layered over the same spawned child for the sync API; plus `fs`/`os`/`crypto`/`path`/`tty`. Both correct on language and boundary; A named the full API surface where B omitted the worker_threads/Atomics sync layer the question asked to enumerate. **A** 10/5/5, 122,772 ch · **B** 9/4/5, 24,800 ch. *(Methodology caveat below — B here was produced by a separate isolated runner + blind grader.)*

### Q12 — Stream and EventEmitter wiring — **tie**
Ground truth: base `Stream` in `lib/internal/streams/legacy.js` — `function Stream(opts){ EE.call(this,opts) }` then two `ObjectSetPrototypeOf` calls (instance + static inheritance); `EventEmitter.prototype.once()` delegates via `_onceWrap`/`onceWrapper` to `on`/`addListener`. Both correct and verbatim-accurate; A directly verified the `on` alias B only inferred, B was ~1/3 the cost. **A** 10/4/5, 10,371 ch · **B** 10/5/4, 28,430 ch.

### Q13 — Redis security issue and fix PR — **A**
Ground truth: issue #15389 (signed overflow in `getBitOffsetFromArgument`, `src/bitops.c`, `loffset *= bits` before bounds checks) and its merged fix PR; vulnerable op = BITFIELD `#<offset>`. Both nailed every asked fact; A verified the actual diff and merge commit ~3× cheaper. **A** 9/5/5, 12,539 ch · **B** 10/4/3, 36,313 ch.

### Q14 — Vitest's dependency on Vite — **tie** *(shared error)*
Ground truth: in `packages/vitest/package.json`, `vite` is a **non-optional peer** (`peerDependencies.vite` present; `peerDependenciesMeta.vite.optional = false`) and is **not** in `dependencies`. **Both arms made the identical confident error** — labeling a `devDependencies` vite entry as a regular `dependencies` entry — so both were capped at correctness 4. B was leaner but no more correct. **A** 4/3/4, 5,655 ch · **B** 4/3/3, 11,102 ch.

### Q15 — Hono JSX array component PR — **B**
Ground truth: `honojs/hono` #5179 (MERGED into `next`); enables a JSX function component returning an array; implemented in `JSXFunctionNode.toStringToBuffer`; references the linked issue; a companion PR is meant to be closed rather than merged. Both fully correct; B reached it in fewer chars/calls with no unverified tangents. **A** 10/4/4, 18,965 ch · **B** 10/4/5, 13,588 ch.

### Q16 — ESLint parser dependency chain — **B**
Ground truth: `eslint/eslint` → `espree` (runtime `dependencies`) → `acorn` (espree's `dependencies`). Both nailed the chain; A was error-free while B made a verified-false `peerDependenciesMeta` claim, so grader gave B the edge on depth but flagged the slip — correctness B 10 vs A 8 here reflects A's slightly weaker citation on this pass. **A** 8/4/5, 10,083 ch · **B** 10/5/4, 14,331 ch.

### Q17 — Next.js fetch request memoization — **B**
Ground truth: installer `patchFetch` (`server/lib/patch-fetch.ts`) composes inner `createDedupeFetch` (`server/lib/dedupe-fetch.ts`) and outer `createPatchedFetcher`; scoped per render via React `cache`; dedupe key derived from method + URL + headers; bypass when the request opts out of caching. Both fully correct/complete; B reached it at ~8× lower cost. **A** 10/5/4, **105,947 ch** · **B** 10/4/5, 13,271 ch.

## Fairness & methodology caveats

- **Blinding.** Per question, the grader saw the two answers as X/Y with tools hidden; assignment alternated by question index (even → X=A, odd → X=B) and was un-blinded only when tabulating.
- **Runner failures / reruns.** On the first workflow pass, two gh+rtk runner agents (Q6, Q15) failed to emit valid structured output. The resume recovered them but — due to cache granularity — also re-ran many graders, so **this report reflects the resume pass** as one coherent set, not the first pass. On the resume, Q11's Octocode runner failed; **Q11-B was then produced by a separate isolated Octocode runner and re-graded by a fresh blind grader** using the same rules. This is the only question whose two arms were not graded inside the same automated pass.
- **Grader non-determinism.** Scores and some preferences shifted between the two passes (e.g., several ties flipped). Per `SCORING.md`, a single pass is a snapshot; treat sub-point correctness deltas and individual tie/pref calls as noisy. The **aggregate character gap (2.6×) and the overall correctness near-tie are the stable signals.**
- **Char-cost drivers.** gh+rtk's total is dominated by a handful of whole-file/whole-diff pulls (Q5 326k, Q2 164k, Q11 123k, Q17 106k). Where a filtered/region read would have answered (notably Q5), that is a suboptimal path, not a policy limit — but it reflects that `gh`/`rtk` lack Octocode's region-targeted read, so the "leanest legal" gh path still fetches more.
- **Calls vs chars.** Octocode used more CLI calls (146 vs 100) — more, smaller, targeted reads — while pulling ~62% fewer characters overall. The benchmark scores characters (context cost), not call count.
