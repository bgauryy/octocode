# Expected Facts

Answer key for [`QUESTIONS.md`](./QUESTIONS.md). Judge-only — agents must not see this file.

Ground truth for the judge agent. Each numbered bullet is one **fact** the judge checks semantically.

### Q1 — How do Next.js, React Router, and TanStack Router each implement their primary navigation push?

1. **Next.js:** `push(href, options?)` method on `AppRouterInstance` defined in `packages/next/src/shared/lib/app-router-context.shared-runtime.ts`; implementation (in `app-router-instance.ts`) calls `startTransition(() => dispatchNavigateAction(href, 'push', ...))` — no `navigate()` method exists on the interface
2. **React Router:** `history.push(to, state?)` in `packages/react-router/lib/router/history.ts`; calls `globalHistory.pushState(historyState, "", url)` directly — does NOT delegate through `router.navigate()`
3. **TanStack Router:** `RouterHistory.push(path, state?)` in `packages/history/src/index.ts`; calls `opts.pushState(path, state)` which queues a `win.history.pushState` call — does NOT delegate to `router.navigate()`
4. All three ultimately wrap the browser's `history.pushState` API
5. React Router's `navigate()` is async (returns `Promise<void>`); Next.js wraps `dispatchNavigateAction` in `startTransition`; TanStack push is microtask-scheduled (via `Promise.resolve().then(flush)`)

### Q2 — How does React capture a thrown promise in Suspense?

1. Function: `throwException` in `packages/react-reconciler/src/ReactFiberThrow.js`
2. Checks `typeof value.then === 'function'` to identify the thrown value as a `Wakeable`
3. Marks the source fiber with `Incomplete` flag
4. Finds the nearest Suspense boundary via `getSuspenseHandler()`
5. Calls `attachPingListener(root, wakeable, rootRenderLanes)` — attaches a `.then()` so the promise resolving re-triggers a render
6. Sets `ShouldCapture` on the Suspense boundary fiber so it renders its fallback on the next pass

### Q3 — Does Next.js App Router use React's `startTransition`? Where?

1. Yes — `startTransition` is used in multiple client files
2. `packages/next/src/client/components/app-router.tsx` — wraps `dispatchTraverseAction` in `startTransition` on popstate (ACTION_RESTORE is dispatched inside `dispatchTraverseAction`); push/replace dispatch lives in `app-router-instance.ts` where `startTransition` wraps `dispatchNavigateAction`
3. `packages/next/src/client/components/redirect-boundary.tsx` — wraps `router.push(redirect)` or `router.replace(redirect)` inside a `useEffect` on redirect
4. `startTransition` marks these as non-urgent updates, keeping the UI interactive while navigation state is being prepared

### Q4 — How do Next.js, Remix, and Nuxt each implement their SSR entry point?

1. **Next.js:** `packages/next/src/server/app-render/app-render.tsx` is the entry; calls React's `renderToReadableStream` (imported in `stream-ops.web.ts`); streaming (Fizz)
2. **Remix:** Server entry in `entry.server.tsx` default; calls `renderToReadableStream` (edge) or `renderToPipeableStream` (Node) with `<ServerRouter>` as root
3. **Nuxt:** `packages/nuxt/src/app/entry.ts` creates the Vue app; the Nitro server renderer (in `packages/nitro-server/src/runtime/utils/renderer/build-files.ts`) imports `renderToString as _renderToString` from `'vue/server-renderer'`; string-based (not streaming by default)
4. React-based frameworks (Next, Remix) use the Fizz streaming API; Nuxt uses Vue's synchronous `renderToString`
5. Next.js additionally calls `resume()` from `react-dom/server` for PPR dynamic shells

### Q5 — How do React, Next.js, and React Router organize their `packages/` directory?

1. **React** (~39 packages): `react-*` prefix throughout; layered architecture visible — `react` (API) → `react-reconciler` (logic) → `react-dom`/`react-native-renderer` (renderers); RSC packages follow `react-server-dom-<bundler>` pattern
2. **Next.js** (~19 packages): `next` is the monolithic core; satellites are tooling (`create-next-app`, `eslint-*`, `next-codemod`) — minimal public API surface
3. **React Router** (~11 packages): `react-router` (core) + `react-router-dom` (DOM bindings) + `react-router-<platform>` adapters (cloudflare, express, node) — explicit adapter-per-deployment-target pattern
4. React reveals a renderer-agnostic architecture; Next.js reveals a product architecture (everything in one package); React Router reveals a universal-router architecture
5. React has the most packages because it separates reconciler from renderer from devtools from utilities; React Router has the least complexity per package

### Q6 — Review PR #27733 in `facebook/react`

1. PR adds "reload and profile" support to `react-devtools-inline` for use in browser-extension or REPL contexts where `location.reload()` is unavailable
2. Files: `react-devtools-inline/src/backend.js` (sets `__REACT_DEVTOOLS_ATTACH__` getter on `contentWindow`), `react-devtools-inline/src/frontend.js` (adds `reload?` param to `initialize()` options), `react-devtools-shared/src/devtools/index.js` (removed), `react-devtools-shared/src/devtools/store.js` (modified), `react-devtools-shell/src/app/devtools.js` (updated)
3. The original disagreement (reviewer wanted `__REACT_DEVTOOLS_ATTACH__` global; author wanted exported `attach`) was resolved — the current PR diff adopts `__REACT_DEVTOOLS_ATTACH__`; remaining blockers are reviewer (@hoxyq) bandwidth and uncertainty about `react-devtools-inline` deprecation
4. Not merged: PR is still **open and stale** (last updated March 2026, created November 2023); stale-bot cycle and potential `react-devtools-inline` deprecation are the main blockers

### Q7 — How does Next.js call into React's streaming renderer?

1. Next.js file: `packages/next/src/server/app-render/stream-ops.web.ts` imports `renderToReadableStream` from `react-dom/server`
2. React file: `packages/react-dom/src/server/ReactDOMFizzServerBrowser.js` defines and exports `renderToReadableStream`
3. Full call chain: `app-render.tsx` → `stream-ops.web.ts` → `ReactDOMFizzServerBrowser.js` → `ReactFizzServer.js` (`createRequest` + `startWork` + `startFlowing`)
4. Next.js also uses `resume()` (from same React file) for PPR dynamic shell resumption
5. RSC flight stream uses a separate `renderToReadableStream` from the RSC renderer, not the Fizz HTML renderer

### Q8 — Do TanStack Query, zustand, and Recoil use `useSyncExternalStore`?

1. **TanStack Query:** ✅ found in `packages/react-query/src/useBaseQuery.ts` (and/or `useMutation.ts`); uses `React.useSyncExternalStore` to subscribe to query cache
2. **zustand:** ✅ found in `src/react.ts`; `React.useSyncExternalStore(api.subscribe, React.useCallback(() => selector(api.getState()), ...), React.useCallback(() => selector(api.getInitialState()), ...))` — also in `src/traditional.ts` via `useSyncExternalStoreWithSelector`
3. **Recoil:** ⚠️ optional — `packages/recoil/core/Recoil_ReactMode.js` contains `useSyncExternalStore` support gated behind a `recoil_sync_external_store` feature flag; the **default** (`LEGACY`) mode uses its own `subscribeToRecoilValue` atom subscription system, not `useSyncExternalStore`
4. Agent provides exact file paths for confirmed usages
5. TanStack Query moved to `useSyncExternalStore` as a React 18 requirement in v5

### Q9 — How do Vue 3 and React each reconcile the virtual DOM?

1. **Vue:** `packages/runtime-core/src/renderer.ts` — `baseCreateRenderer()` returns `render` and `createApp`; core entry is the `patch(n1, n2, container)` function
2. **React:** `packages/react-reconciler/src/ReactFiberWorkLoop.js` — `workLoopConcurrent()` iterates over the fiber tree; `beginWork()` (defined in `ReactFiberBeginWork.js`) dispatches by fiber tag
3. Vue's `patch()` uses compiler-generated `patchFlag` and `dynamicChildren` to skip static nodes — O(dynamic nodes); React walks the entire fiber tree with bailouts via `memo` — O(all nodes in subtree)
4. Vue batches via microtask queue (`queueFlush`); React uses `Scheduler` with priority lanes and can yield mid-render (`shouldYield()`) for time-slicing
5. Fundamental difference: Vue combines reactivity (knows *what* changed) + VDOM (knows *how* it renders) → targeted updates; React has no reactivity → must re-render full subtree from dirty root

### Q10 — How do Vue 3's reactivity system and React's hook system each track state dependencies?

1. **Vue:** `packages/reactivity/src/dep.ts` — `track()` runs on Proxy `get`, links `activeSub` (current running effect) to the dependency; `trigger()` on Proxy `set` notifies all linked subscribers
2. **React:** `packages/react-reconciler/src/ReactFiberHooks.js` — `dispatchSetState` enqueues an `Update` with a `lane` (priority), calls `scheduleUpdateOnFiber`; no automatic dependency tracking
3. Vue: re-renders only components that accessed a changed reactive property; React: re-renders the entire component and its subtree (unless memoized)
4. Vue gotcha: destructuring a reactive object loses reactivity (must use `toRefs`); React gotcha: stale closures in `useEffect`, hook rules (no conditional calls)
5. Vue tracks at property granularity (Proxy per-key); React tracks at component granularity (fiber per component)

### Q11 — How does Svelte 5 implement the `$state` rune at the runtime level?

1. Core file: `packages/svelte/src/internal/client/reactivity/sources.js`
2. Key function: `source(initial_value)` creates a reactive signal; `mutable_source(initial_value)` is used for legacy (Svelte 4 compat) mode — in runes mode `$.state()` calls `source()` directly
3. `internal_set(source, value)` performs the mutation and notifies subscribers when the value changes (checked via `source.equals`)
4. Svelte compiles `$state` declarations into `$.state()` runtime signal calls — template access is tracked via `$.template_effect`, not getter/setter property accessors; no virtual DOM involved
5. Effects and component renders that read a signal are tracked via a subscriber list on the signal node; mutations re-run only those dependent effects (fine-grained reactivity)

### Q12 — How does Vue 3's template compiler parse a `v-if` directive?

1. Transform plugin: `transformIf` in `packages/compiler-core/src/transforms/vIf.ts`
2. Parses `v-if`, `v-else-if`, `v-else` siblings and groups them into a single `IfNode` (`NodeTypes.IF`)
3. Each branch becomes an `IfBranchNode` with a `condition` expression and a `children` array
4. The codegen then emits a ternary expression chain: `condition ? consequent : alternate`
5. Unlike React's JSX `{condition && <Component />}`, Vue's compiler handles all branches together as a single structural node, allowing the runtime to optimize diffing between branches

### Q13 — How does Next.js implement Partial Prerendering (PPR)?

1. Core file: `packages/next/src/server/app-render/dynamic-rendering.ts` — exports a `Postpone()` function that throws a "postpone" to signal a dynamic boundary
2. Uses React's experimental `unstable_postpone` (from `react`) to interrupt static rendering at a Suspense boundary
3. Static shell rendered at build time; `postponedState` blob stored for later dynamic resumption
4. At request time: `resume()` from `react-dom/server` receives the `postponedState` and streams the dynamic parts
5. `isRoutePPREnabled` flag in `packages/next/src/server/app-render/types.ts` gates the behavior; `app-render.tsx` checks it before choosing between static and PPR render paths

### Q14 — How does Solid.js implement `createSignal`? How does it differ from React's `useState`?

1. `createSignal` is in `packages/solid/src/reactive/signal.ts` (or `packages/solid/src/reactive/`)
2. Returns `[getter, setter]` where `getter()` is a function (unlike React's plain value) — reading it inside an effect auto-tracks the dependency
3. Solid uses an owner/observer graph: when `getter()` is called during an effect or memo, the signal registers the current observer as a subscriber
4. When `setter()` is called, only the subscribed observers (effects, memos, component render functions) re-run — NOT the entire component tree
5. Key difference from `useState`: React re-renders the whole component function; Solid re-runs only the specific reactive expression that read the changed signal (no virtual DOM diffing needed)

### Q15 — How do Svelte 5 and Vue 3 each approach compile-time vs runtime reactivity?

1. **Svelte 5:** reactivity lives in both compiler and runtime — `$state` rune compiles to `source()` / `$.state()` call; the runtime (`packages/svelte/src/internal/client/reactivity/sources.js`) manages the signal graph
2. **Vue 3:** reactivity is primarily runtime — `ref()` and `reactive()` are plain function calls using `Proxy`; the compiler adds `patchFlags` and `dynamicChildren` for render optimization only
3. Svelte compiler output for `let count = $state(0)` → `let count = $.state(0)`; Vue's `const count = ref(0)` is a runtime `RefImpl` call — no compilation needed
4. Svelte knows which DOM node to update because the compiler generates a dedicated `effect` per reactive expression; Vue re-runs the whole component render function when any reactive dep changes, then diffs the `VNode` tree
5. Tradeoff: Svelte's compile-time approach produces smaller runtime bundles with fine-grained DOM updates; Vue's runtime approach is more dynamic and works without a build step (`RefImpl`, `Dep`, `ReactiveEffect` are pure JS)

### Q16 — Where is `parse()` implemented in Zod v4?

1. File: `packages/zod/src/v4/core/parse.ts`
2. `parse` is defined as `_parse(errors.$ZodRealError)` — a factory pattern that binds the error class at definition time
3. `_parse` calls `schema._zod.run({ value, issues: [] }, ctx)` — the schema's internal `run` method, not `safeParse`
4. If `result instanceof Promise`, immediately throws `$ZodAsyncError` — `parse()` is synchronous-only and rejects async schemas at runtime
5. On failure: maps `result.issues` through `util.finalizeIssue(iss, ctx, core.config())` and throws a `ZodError`; success returns `result.value as core.output<typeof schema>`

### Q17 — How does Hono chain middleware? (`compose()`)

1. File: `src/compose.ts`; `compose()` accepts `middleware`, optional `onError`, optional `onNotFound`; returns `(context: Context, next?: Next) => Promise<Context>`
2. Returns a dispatcher closure; calling it invokes `dispatch(0)` — a recursive async function indexed on middleware position
3. Each handler is called as `handler(context, () => dispatch(i + 1))` — the `next()` function advances the index to the next middleware
4. Guards against double-invocation with `if (i <= index) throw new Error('next() called multiple times')` — ensures each handler's `next()` can only advance once
5. After all handlers run: if the context is not finalized and `onNotFound` is provided, calls `onNotFound(context)` as a 404 fallback

### Q18 — How does Vitest execute a single test? (`runTest`)

1. File: `packages/runner/src/run.ts`; signature `runTest(test: Test, runner: VitestRunner): Promise<void>`
2. First calls `runner.onBeforeRunTask?.(test)` — optional runner lifecycle hook for plugins
3. Returns early if `test.mode !== 'run' && test.mode !== 'queued'` — handles `skip`, `todo`, `only`, `failing` modes without executing
4. Initializes `test.result = { state: 'run', startTime: unixNow(), retryCount: 0 }` and calls `setCurrentTest(test)` to make the test accessible globally during execution
5. Runs nested loops: outer `for (let repeatCount = 0; repeatCount <= repeats; repeatCount++)`, inner `for (let retryCount = 0; retryCount <= retry; retryCount++)` — `repeat` and `retry` are independent axes; a test with `repeats: 2, retry: 3` may execute up to 12 times

### Q19 — Astro vs Next.js: SSR rendering and component model

1. **Astro SSR entry:** `renderPage()` in `packages/astro/src/runtime/server/render/page.ts`; uses `renderToReadableStream` / `renderToAsyncIterable` from Astro's own renderer (`./astro/render.js`), **not** React's `react-dom/server`
2. **Next.js SSR entry:** `app-render.tsx` in `packages/next/src/server/app-render/`; calls React's `renderToReadableStream` from `react-dom/server` via `stream-ops.web.ts`
3. Astro is renderer-agnostic: React, Vue, Svelte components all render through framework adapters installed as integrations; Next.js is React-exclusive
4. **Islands model:** Astro ships zero JS by default; `client:*` directives (`client:load`, `client:idle`, `client:visible`) opt individual components into independent hydration via the `<astro-island>` web component; each island hydrates in isolation
5. **RSC model:** Next.js maintains a full React fiber tree per request; RSC sends a React flight payload; client components share one React tree and hydrate together — no zero-JS default

### Q20 — Review PR #7336 in `trpc/trpc`

1. PR solves: React 19 coerces proxy objects to primitives by calling `valueOf()`, `toString()`, or `toJSON()` on the proxy returned by `createInnerProxy()`; the existing `get` trap returned another proxy instead of a primitive, causing infinite recursion or broken coercion in React 19 rendering and string-interpolation paths
2. Files changed: `packages/server/src/unstable-core-do-not-import/createProxy.ts` (14 additions — the runtime fix) and `createProxy.test.ts` (33 additions — regression tests covering all three coercion methods)
3. Core fix: in the `apply` trap of `createInnerProxy`, detect when `valueOf`, `toString`, or `toJSON` is the last path segment and return a real primitive (debug string like `tRPC.proxy(<path>)`) rather than a new proxy — breaking the recursion
4. React 19 introduced new coercion call sites during rendering, string interpolation, and console logging that did not exist in React 18 — the bug was latent; downstream apps previously needed a `patch-package` workaround

---

## Maintenance

- **Verified:** 2026-05-25 (full re-verification with live GitHub data)
- **Never** add Expected Facts to `QUESTIONS.md`. The split exists so the agents never see the answer key.
