# octocode vs `gh`+`rtk` — full pass (all 17), 2026-08-04

**Bottom line:** correctness tied **10/10 on every question, both arms**. On characters pulled into context, **Octocode (B) was ~30% leaner overall** (168,773 vs 242,691) and leaner on **13/17** questions. `gh`+`rtk` (A) was leaner on the 4 pure locate / diff-dump questions (Q5, Q11, Q13, Q15). Octocode's edge comes entirely from **region-targeted reads** on large files/manifests that `gh` can only fetch whole (Q2, Q9, Q12, Q16). One structural finding: on the diff-review question **rtk silently truncated `gh pr diff`** and hid material changes (see Q5).

- **Arms:** A = read-only `gh` via `rtk gh …` (raw media type for file content, filtered `pr view`/`pr diff`). B = `npx octocode tools …` (published `octocode v17.0.1`).
- **Roles:** each question was answered by the two arms independently, then graded against ground truth I established myself from current primary evidence. All answers verified; no answer key used.
- **Measure:** characters of raw CLI output pulled into context.
- **Frozen refs (UTC 2026-08-04T19:27Z):** next.js@canary d87a203b · sindresorhus/is@main 7821031c · flask@main 6a2f545b · zustand@main beca84e6 · vuejs/core@main b67cfcfa · express@master a3714473 · vscode@main 43db17b4 · fastify@main f28528ff · axios@v1.x 39955a6e · esbuild@main 6ff1d8b0 · node@main 1fc74c8d · redis@unstable bf49481a · vitest@main 96e40fee · hono@main 192768fb · eslint@main 56110356.

## Per-question table

| Q | Correctness A | Correctness B | Depth A | Depth B | Workflow A | Workflow B | Chars A | Chars B | Leaner |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---|
| Q1 Route regex builder | 10 | 10 | 5 | 5 | 5 | 5 | 13,830 | 2,652 | **B** |
| Q2 `is` repo + bounded absence | 10 | 10 | 5 | 5 | 4 | 4 | 75,727 | 57,249 | **B** |
| Q3 Flask route history | 10 | 10 | 5 | 5 | 5 | 5 | 9,679 | 8,274 | **B** |
| Q4 Zustand fix PR state | 10 | 10 | 5 | 5 | 4 | 5 | 8,527 | 6,765 | **B** |
| Q5 Vue hydration diff review | 10 | 10 | 5 | 5 | 3 | 5 | 36,148 | 45,625 | **A** |
| Q6 Express router trace | 10 | 10 | 5 | 5 | 5 | 5 | 3,571 | 2,167 | **B** |
| Q7 Zustand Next.js contract | 10 | 10 | 5 | 5 | 4 | 5 | 7,466 | 2,808 | **B** |
| Q8 VS Code keybinding dispatch | 10 | 10 | 5 | 5 | 5 | 5 | 2,317 | 2,004 | **B** |
| Q9 Fastify lifecycle contract | 10 | 10 | 5 | 5 | 4 | 5 | 4,781 | 2,953 | **B** |
| Q10 Axios entry chain | 10 | 10 | 5 | 5 | 4 | 5 | 7,673 | 3,174 | **B** |
| Q11 Esbuild process boundary | 10 | 10 | 5 | 5 | 5 | 4 | 317 | 4,964 | **A** |
| Q12 Stream/EventEmitter wiring | 10 | 10 | 5 | 5 | 4 | 5 | 39,661 | 2,556 | **B** |
| Q13 Redis BITFIELD security | 10 | 10 | 5 | 5 | 4 | 4 | 7,536 | 10,998 | **A** |
| Q14 Vitest→Vite dependency | 10 | 10 | 5 | 5 | 5 | 4 | 5,576 | 3,390 | **B** |
| Q15 Hono JSX array PR | 10 | 10 | 5 | 5 | 4 | 5 | 3,658 | 5,064 | **A** |
| Q16 ESLint parser chain | 10 | 10 | 5 | 5 | 4 | 5 | 9,936 | 3,430 | **B** |
| Q17 Next.js fetch memoization | 10 | 10 | 5 | 5 | 5 | 5 | 6,288 | 4,700 | **B** |

## Summary of all

| Metric | A (`gh`+`rtk`) | B (Octocode) |
|---|---:|---:|
| Correctness (mean) | 10.0 | 10.0 |
| Research depth (mean) | 5.0 | 5.0 |
| Workflow (mean) | 4.35 | 4.76 |
| **Chars (total)** | **242,691** | **168,773** |
| Questions leaner | 4 | **13** |

Octocode ~30.5% fewer characters overall. Both arms reached identical, verifiable conclusions on all 17.

## Per question (detail)

Answers are the graded ground truth; both arms produced it. Fairness caveats noted inline.

**Q1 — Route regex builder.** `getRouteRegex` in `packages/next/src/shared/lib/router/utils/route-regex.ts`; the first helper it calls is `getParametrizedRoute(...)`, which builds the named `groups`. A: `rtk gh search code` → raw full-file pull (12.7 KB, gh has no region read). B: code search → one `matchString` region (1.7 KB). Both correct; **B leaner**.

**Q2 — `is` repo + bounded absence.** `sindresorhus/is`, primary language **TypeScript**, default branch **main**. `isQuantumSuperposition` is **NOT** defined/exported — bounded by reading the whole public export surface (`source/index.ts`, 2,038 lines) plus a scoped code search returning zero. A: repo view (89) + full `source/index.ts` raw (75.6 KB), grep quantum = 0. B: repo search (language) + `resolvedBranch:"main"` from structure + zero-hit code search (with an explicit "unproven absence" warning) + `minify:"symbols"` outline of the export surface (53.9 KB). Both establish the same NO on the same bounded evidence; **B leaner** (symbols outline < raw file). This is the single most expensive question for both arms.

**Q3 — Flask route history.** `route` decorator lives in `src/flask/sansio/scaffold.py`, owning base class **`Scaffold`**. Commit `705e5268` ("Add syntactic sugar for route registration") added the HTTP-method shortcut decorators `get/post/put/delete/patch` plus a `_method_route(method, rule, options)` helper that raises `TypeError` if `methods` is passed and otherwise delegates to `self.route(rule, methods=[method], **options)` — i.e. behavior read from the changed code, not the title. A: code search + full single-commit API (9.4 KB). B: code search + `ghHistoryResearch` commits-from-SHA with diff (6.2 KB) + two `matchString` reads. Both correct; **B leaner**.

**Q4 — Zustand fix PR state.** PR **#3531**, state **OPEN**, changes `src/middleware/devtools.ts`; edge case = a **source path containing spaces** (webpack namespace `webpack://my app/...` and Windows `C:\Program Files\...`) which the greedy V8 stack-frame regex `/.+ (.+) .+/` mis-captured. A: `pr view` + `pr diff` — the diff dumped the added `package-lock.json` (9,056 lines). B: one `ghHistoryResearch` PR call returned metadata + body + the 2-line `devtools.ts` patch and **skipped the lockfile patch**. Both correct; **B leaner and cleaner**.

**Q5 — Vue hydration diff review (structural finding).** PR **#15035** (merged), "preserve VNode anchors in dynamic component hydration." Two concrete scenarios fixed: (1) **async components during hydration** — an async wrapper can be moved/unmounted before its inner component resolves (`#3787`); `runtime-core/src/hydration.ts` now passes `nextNode` instead of `null` and reworks the async-wrapper placeholder/anchor handling; (2) **Vapor↔VDOM interop** — `runtime-vapor/src/vdomInterop.ts` stops delaying `selfAnchor` insertion via `queuePostFlushCb` and inserts it directly while preserving `vnode.el = selfAnchor`, and `runtime-vapor/src/component.ts` `unmountComponent` now handles an unresolved async block (`asyncDep && !asyncResolved`, `instance.block` null). Changes are required in **both** packages because the bug spans the VDOM↔Vapor boundary: `runtime-core` owns the shared hydration/anchor logic; `runtime-vapor` implements the Vapor side of the same interop.
**Fairness caveat:** A's first `rtk gh pr diff` returned a **compacted, truncated** diff (`… more changes truncated`) that showed only the two `runtime-core` files and **hid all `runtime-vapor` src changes** — a runner trusting it would have missed half the answer. A had to re-fetch the unfiltered diff (`--patch`), a redundant 17 KB pull. B got all 8 files' patches in **one** call (45.6 KB). Net chars favor **A** (36.1 KB vs 45.6 KB), but A's win depends on noticing rtk's truncation; workflow: A 3, B 5.

**Q6 — Express router trace.** The layer-matching loop is **not** in `expressjs/express`; `package.json` depends on `router` (`^2.2.0` → `pillarjs/router`). `next(err)` in `pillarjs/router` **`index.js`** advances layers; helper `matchLayer(layer, path)` (also `index.js`) tests one layer, calling `Layer.prototype.match` in **`lib/layer.js`**. A: full `package.json` raw + code searches. B: `matchString` dep read + three concise code searches. Both correct; **B leaner**.

**Q7 — Zustand Next.js contract.** `examples/with-zustand/src/lib/store.ts` is a **React Context-backed per-request store factory**, not a module singleton: `createContext`/`useContext` (react) + `createStore`/`useStore` (zustand); `initializeStore()` builds a fresh store per call, provided via `storeContext.Provider`. In zustand's root `package.json`, React is an **optional peer**: `peerDependencies.react: ">=18.0.0"` with `peerDependenciesMeta.react.optional: true` (not a regular dependency). A: full store + full 6 KB `package.json`. B: full store + `matchString` peer region (1.2 KB). Both correct; **B leaner**.

**Q8 — VS Code keybinding dispatch.** Concrete class **`WorkbenchKeybindingService`** in `src/vs/workbench/services/keybinding/browser/keybindingService.ts`, extending base **`AbstractKeybindingService`** in `src/vs/platform/keybinding/common/abstractKeybindingService.ts`; public method that receives a keypress for dispatch = **`dispatchEvent(e: IKeyboardEvent, target)`** (delegates to `_dispatch`). Both used code search + one confirming read; near-tie, **B marginally leaner**.

**Q9 — Fastify lifecycle contract.** Documented order: Incoming Request → Routing → Instance Logger → **onRequest** → **preParsing** → **Parsing** → **preValidation** → **Validation** → **preHandler** → User Handler. In `lib/route.js`, per-route context property **`context.onRequest`** is run by **`onRequestHookRunner`** (imported from `./hooks`). A: full `Lifecycle.md` raw (4.6 KB) + code search — the search snippet alone already named `context.onRequest` + `onRequestHookRunner`, so A's larger full-`route.js` pull was **avoidable** (lean path counted, 4,781; the redundant 23 KB file pull excluded, workflow 4). B: two `matchString` reads (doc section + the exact `route.js` hunk). Both correct; **B leaner**.

**Q10 — Axios entry chain.** `axios/axios`, dominant language **JavaScript** (974 KB vs TS 102 KB). `main: ./dist/node/axios.cjs`; `exports["."]` CJS `require` → `./dist/node/axios.cjs` (bundled); the non-bundled ESM default → `./index.js`, which `import axios from './lib/axios.js'` — the source entry under `lib/`. A: languages API + full 6.7 KB `package.json` + `index.js`. B: repo-search language + `matchString` `exports` region (1.5 KB) + `index.js` region. Both correct; **B leaner** (targeted manifest region).

**Q11 — Esbuild process boundary.** Source repo `evanw/esbuild`, dominant language **Go** (5.1 MB vs JS 1.1 MB). The normal Node API runs the core **in a separate process**: `lib/npm/node.ts` uses **`child_process.spawn`** to launch a long-lived esbuild service subprocess (`--service`), communicating over **stdin/stdout pipes** (`stdio: ['pipe','pipe','inherit']`) — not in-process. A: languages API (109) + two filename-scoped code searches returning one-line snippets (208) = **317 chars**, the leanest result of the whole run. B: repo search (976) + broader code-search snippets (2,773) + one region read (1,215) = 4,964. Both correct; **A much leaner** — a pure locate question where `gh`'s filename-scoped snippet search shines.

**Q12 — Stream/EventEmitter wiring.** Base `Stream` constructor in `lib/internal/streams/legacy.js`: `function Stream(opts){ EE.call(this, opts); }` with the prototype wiring `ObjectSetPrototypeOf(Stream.prototype, EE.prototype); ObjectSetPrototypeOf(Stream, EE);` (`EE = require('events')`). `EventEmitter.prototype.once(type, listener)` delegates by calling `this.on(type, _onceWrap(this, type, listener))`; the internal helper **`_onceWrap`** returns a `wrapper` guarded by a `fired` flag that calls `target.removeListener(type, wrapper)` on first fire and invokes the original via `ReflectApply` (`wrapper.listener = listener`). Delegated methods: `on`/`addListener` (registration via `_addListener`) and `removeListener` (one-shot cleanup). A: full `legacy.js` (3.3 KB) + full **`events.js` (36.3 KB)** — gh cannot region-read. B: two `matchString` reads (936 + 1,620). Both correct; **B dramatically leaner** (2.6 KB vs 39.7 KB) — the single biggest relative win for Octocode.

**Q13 — Redis BITFIELD security.** Issue **#15389** "[BUG] Redis BITFIELD `#offset` Signed Overflow DoS"; merged PR **#15433** "Fix signed overflow in BITFIELD #offset parsing." Vulnerable op = `BITFIELD`/`BITFIELD_RO` `#<offset>` syntax; function **`getBitOffsetFromArgument()`** in `src/bitops.c`; sink `loffset *= bits` (signed `long long` multiply before bounds check). First overflowing `i64` offset ≈ `floor(LLONG_MAX/64)+1` = **144115188075855872**. PR changed **`src/bitops.c`** and **`tests/unit/bitfield.tcl`**, **+18 / −3**. A: tiny concise issue/PR search rows + issue body (7 KB) + `pr view --json`; **avoidable slip** — first PR search used `--state merged` (invalid gh flag, ~3.3 KB help text) before the correct `--merged` (lean path 7,536 counted; workflow 4). B: `ghHistoryResearch` issue list + PR list + issue body (7 KB) + PR detail — its concise triage lists add overhead the tiny `gh` rows don't. Both correct; **A leaner** (7,536 vs 10,998) on triage economy, the issue body dominating both.

**Q14 — Vitest→Vite dependency.** In `packages/vitest/package.json`, `vite` is declared **only as a peer dependency** (`peerDependencies.vite: "^6.4.0 || ^7.0.0 || ^8.0.0"`), **not** as a regular dependency, and it is **not optional** — `peerDependenciesMeta.vite: { "optional": false }` (an explicit non-optional entry, alongside many `optional:true` peers). A: one full `package.json` raw (5.6 KB) — authoritative in a single read. B: three `matchString` region reads (1,473 + 1,095 + 822 = 3,390) to piece together deps-absence, the peer field, and the explicit `vite.optional:false` entry (its first window cut off before the last `vite` key). **B leaner in chars, A cleaner in calls** (1 vs 3).

**Q15 — Hono JSX array PR.** PR **#5179**, state **MERGED**, enables **a function/JSX component returning an array** (`() => [<a/>, <b/>]`, as `hono/jsx/dom` already allows) by handling arrays in `JSXFunctionNode.toStringToBuffer()` via `childrenToStringToBuffer()`. Source file: **`src/jsx/base.ts`**. References issue **#5177**; the alternative PR meant to be **closed rather than merged** is **#5178** ("The two are alternatives, not a pair. Only one should be merged, and I will close the other"). A: `pr view` (body, 3.1 KB) + a small `--json files` (572) to name `base.ts` = 3,658. B: one `ghHistoryResearch` call (body + full changed-file list, 5,064). Both correct; **A leaner in chars, B fewer calls**.

**Q16 — ESLint parser chain.** `eslint/eslint` → **`dependencies.espree`** `^11.2.0` → `espree` (`eslint/js`, `packages/espree/package.json`) → **`dependencies.acorn`** `^8.16.0` (the low-level JS parser; also `acorn-jsx`). Each hop cited via its `dependencies` field. A: full 7.8 KB root `package.json` + full espree `package.json` + code search. B: `matchString` dep region on eslint's manifest + **`npmSearch`** to resolve `espree`→`eslint/js` (`repositoryDirectory: packages/espree`) + `matchString` acorn region. Both correct; **B leaner** (3,430 vs 9,936) and the npm→repo hop was a clean resolution.

**Q17 — Next.js fetch memoization.** `patchFetch(options)` in `packages/next/src/server/lib/patch-fetch.ts` installs the wrapped fetch: `const original = createDedupeFetch(globalThis.fetch); globalThis.fetch = createPatchedFetcher(original, options)`. Two composed layers: **`createPatchedFetcher`** (caching, `patch-fetch.ts`) wrapping **`createDedupeFetch`** (request memoization, `server/lib/dedupe-fetch.ts`). Memoization is scoped to one render via **`React.cache`** (`getCacheEntries = React.cache(...)`). The dedupe cache key (`generateCacheKey`) is `JSON.stringify([request.method, filteredHeaders, request.mode, request.redirect, request.credentials, request.referrer, request.referrerPolicy, request.integrity])` (plain-string GET uses the `simpleCacheKey` fast path). A request **bypasses** dedup and hits the original fetch when `(request.method !== 'GET' && request.method !== 'HEAD') || request.keepalive` (side-effecting/keepalive requests) — `return originalFetch(resource, options)`. A: code searches to locate + full `dedupe-fetch.ts` (5 KB) yielding all three facts in one read. B: locate search + four `matchString` region reads (install / React.cache / key / bypass). Both correct; **B leaner** (4,700 vs 6,288).

## Fairness notes / caveats

- **rtk truncates diffs (Q5).** `rtk gh pr diff` compacted and dropped material file changes with only a `… truncated` marker; unfiltered re-fetch was required to answer correctly. Treat rtk-compacted diffs as potentially incomplete on multi-file PRs.
- **gh has no region read.** For large files/manifests (Q2, Q9, Q12, Q16, Q7, Q10, Q14) arm A must fetch the whole file (raw media type is its leanest legal path); this is where Octocode's `matchString`/range/symbols reads win. It is a genuine capability gap, not a handicap I imposed.
- **Octocode triage lists cost more than bare `gh` rows** (Q11, Q13, Q15): concise-but-structured JSON lists carry pagination/next-hint scaffolding that `gh`'s one-line rows omit — `gh` wins pure locate/enumerate tasks.
- **Avoidable slips counted leanly, flagged:** A's redundant full `route.js` (Q9) and its invalid `--state` gh flag (Q13) were excluded from A's char totals (lean path counted) but docked in workflow, so efficiency isn't inflated by my operator error.
- **Published version.** Arm B ran `npx octocode v17.0.1` (12-tool surface: `ghHistoryResearch` for PRs/commits/issues), per the benchmark rule (`npx octocode tools …` only, no monorepo entrypoint).
- One pass is a snapshot; repeat for a stable claim.
