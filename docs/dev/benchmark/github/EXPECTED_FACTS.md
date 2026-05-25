# Expected Facts

Answer key for [`QUESTIONS.md`](./QUESTIONS.md). Judge-only — agents must not see this file.

Auto-scored by `scripts/judge.mjs` (token-match heuristic) and verified for drift by `scripts/verify-facts.mjs`. Numeric claims (stars, issue counts, PR ranges) drift — re-verify quantitative facts at every refresh and update the `Verified` date at the bottom.

### Q1 — Where is `useState` implemented in React?

1. File: `packages/react/src/ReactHooks.js`
2. Function calls `resolveDispatcher()` to get the active dispatcher
3. Delegates to `dispatcher.useState(initialState)`
4. `resolveDispatcher()` returns `ReactSharedInternals.H` (the current fiber dispatcher)
5. Dispatcher is swapped between `HooksDispatcherOnMount` and `HooksDispatcherOnUpdate` — the implementation differs per render phase

### Q2 — How do Next.js, React Router, and TanStack Router each implement their primary navigation push?

1. **Next.js:** `push(href, options?)` method on `AppRouterInstance` defined in `packages/next/src/shared/lib/app-router-context.shared-runtime.ts`; implementation (in `app-router-instance.ts`) calls `startTransition(() => dispatchNavigateAction(href, 'push', ...))` — no `navigate()` method exists on the interface
2. **React Router:** `history.push(to, state?)` in `packages/react-router/lib/router/history.ts`; delegates to `router.navigate(to)` via `components.tsx`
3. **TanStack Router:** `RouterHistory.push(path, state?)` in `packages/history/src/index.ts`; type-safe navigation via `router.navigate()`
4. All three ultimately wrap the browser's `history.pushState` API
5. React Router's `navigate()` is async (returns `Promise<void>`); Next.js wraps `dispatchNavigateAction` in `startTransition`; TanStack uses synchronous push

### Q3 — How does React capture a thrown promise in Suspense?

1. Function: `throwException` in `packages/react-reconciler/src/ReactFiberThrow.js`
2. Checks `typeof value.then === 'function'` to identify the thrown value as a `Wakeable`
3. Marks the source fiber with `Incomplete` flag
4. Finds the nearest Suspense boundary via `getSuspenseHandler()`
5. Calls `attachPingListener(root, wakeable, rootRenderLanes)` — attaches a `.then()` so the promise resolving re-triggers a render
6. Sets `ShouldCapture` on the Suspense boundary fiber so it renders its fallback on the next pass

### Q4 — Does Next.js App Router use React's `startTransition`? Where?

1. Yes — `startTransition` is used in multiple client files
2. `packages/next/src/client/components/app-router.tsx` — wraps `dispatchAppRouterAction({ type: ACTION_RESTORE })` on history restore and `dispatchTraverseAction` on popstate; push/replace dispatch lives in `app-router-instance.ts` (imported by app-router.tsx) where `startTransition` wraps `dispatchNavigateAction`
3. `packages/next/src/client/app-call-server.ts` — wraps `dispatchAppRouterAction({ type: ACTION_SERVER_ACTION, actionId, actionArgs, ... })` for server actions
4. `packages/next/src/client/components/redirect-boundary.tsx` — wraps `router.push(redirect)` or `router.replace(redirect)` inside a `useEffect` on redirect
5. `startTransition` marks these as non-urgent updates, keeping the UI interactive while navigation state is being prepared

### Q5 — How do Next.js, Remix, and Nuxt each implement their SSR entry point?

1. **Next.js:** `packages/next/src/server/app-render/app-render.tsx` is the entry; calls React's `renderToReadableStream` (imported in `stream-ops.web.ts`); streaming (Fizz)
2. **Remix:** Server entry in `entry.server.tsx` default; calls `renderToReadableStream` (edge) or `renderToPipeableStream` (Node) with `<ServerRouter>` as root
3. **Nuxt:** `packages/nuxt/src/app/entry.ts` creates Vue app; renderer imports `renderToString as _renderToString` from `'vue/server-renderer'` (the unified Vue package re-export, not `@vue/server-renderer` directly); string-based (not streaming by default)
4. React-based frameworks (Next, Remix) use the Fizz streaming API; Nuxt uses Vue's synchronous `renderToString`
5. Next.js additionally calls `resume()` from `react-dom/server` for PPR dynamic shells

### Q6 — What parameters does `renderToReadableStream` accept?

1. File: `packages/react-dom/src/server/ReactDOMFizzServerBrowser.js`
2. Signature: `renderToReadableStream(children: ReactNodeList, options?: Options): Promise<ReactDOMServerReadableStream>`
3. Return: `ReadableStream & { allReady: Promise<void> }` — `allReady` resolves when all Suspense boundaries resolve
4. Key options: `bootstrapScripts`, `bootstrapModules`, `bootstrapScriptContent`, `signal` (AbortSignal), `onError`, `nonce`, `identifierPrefix`, `progressiveChunkSize`
5. Additional options: `importMap`, `formState` (ReactFormState), `onHeaders`, `maxHeadersLength`

### Q7 — What are the top-level imports in `app-render.tsx`?

1. Imports from `react` (ComponentType, ErrorInfo, JSX, ReactNode) and a namespace import `* as ReactClient`
2. Imports from `./types` (RenderOpts, PreloadCallbacks) and `../../shared/lib/app-router-types` (FlightRouterState, RSCPayload, FlightDataPath, etc.)
3. Imports from `node:stream` (Readable) and `http` (IncomingHttpHeaders)
4. Imports from `./work-async-storage.external` and `./work-unit-async-storage.external` (AsyncLocalStorage contexts)
5. Imports from `../render-result` (RenderResult) and `./stream-utils/` (chainStreams, continueFizzStream, streamToBuffer, etc.)

### Q8 — What does the `zustand` `createStore` type signature look like?

1. `CreateStore` is a type with two overloads (direct and curried)
2. Direct: `<T, Mos>(initializer: StateCreator<T, [], Mos>) => Mutate<StoreApi<T>, Mos>`
3. Curried: `<T>() => <Mos>(initializer: StateCreator<T, [], Mos>) => Mutate<StoreApi<T>, Mos>`
4. `Mos` extends `[StoreMutatorIdentifier, unknown][]` — the middleware mutators array that extends the store API (e.g., devtools, immer, persist)
5. `Mutate<S, Ms>` recursively applies mutators to the store type; `StoreApi<T>` exposes `getState`, `setState`, `getInitialState`, `subscribe`

### Q9 — How is `vercel/next.js` organized at the top level?

1. `packages/` — all publishable npm packages (next, create-next-app, eslint-plugin-next, etc.)
2. `turbopack/` — Turbopack Rust bundler source; `crates/` — SWC/native Rust crates
3. `test/` — integration and e2e test suites; `bench/` — benchmarking infrastructure
4. `evals/` — LLM evaluation harness for Next.js agent tasks (43+ eval scenarios)
5. `examples/` — framework usage examples; `skills/` — AI agent skills for maintainers

### Q10 — What subsystems exist inside `packages/next/src/server/`?

1. `app-render/` — App Router SSR core (app-render.tsx, stream-ops, dynamic-rendering.ts)
2. `route-modules/`, `route-definitions/`, `route-matchers/` — route handling and URL matching infrastructure
3. `dev/` — dev server (HMR, error overlay); `web/` — edge runtime adapter
4. `stream-utils/` — Node/Web stream helpers; `use-cache/` — `use cache` directive implementation; `response-cache/` — ISR cache
5. `mcp/` — Model Context Protocol integration; `og/` — Open Graph image generation; `after/` — `after()` post-response API

### Q11 — How do React, Next.js, and React Router organize their `packages/` directory?

1. **React** (~39 packages): `react-*` prefix throughout; layered architecture visible — `react` (API) → `react-reconciler` (logic) → `react-dom`/`react-native-renderer` (renderers); RSC packages follow `react-server-dom-<bundler>` pattern
2. **Next.js** (~19 packages): `next` is the monolithic core; satellites are tooling (`create-next-app`, `eslint-*`, `next-codemod`) — minimal public API surface
3. **React Router** (~11 packages): `react-router` (core) + `react-router-dom` (DOM bindings) + `react-router-<platform>` adapters (cloudflare, express, node) — explicit adapter-per-deployment-target pattern
4. React reveals a renderer-agnostic architecture; Next.js reveals a product architecture (everything in one package); React Router reveals a universal-router architecture
5. React has the most packages because it separates reconciler from renderer from devtools from utilities; React Router has the least complexity per package

### Q12 — Which of zustand, jotai, and redux-toolkit is most actively maintained?

1. zustand: ~58k stars, pushed within last week, <10 open issues
2. jotai: ~21k stars, pushed within last week, <20 open issues
3. redux-toolkit: ~11k stars, active pushes, 200+ open issues
4. All three are actively maintained (pushed within last 2 weeks)
5. Conclusion: zustand leads by stars and issue triage; RTK has more issues reflecting wider API surface and enterprise adoption

### Q13 — What are the top TypeScript testing repositories created after 2022, sorted by stars?

1. Strict "test framework" repos (test runner / assertion library, TypeScript-language, created >= 2022-01-01, stars > 1,000) are rare — fewer than 5 exist; broadening to "testing tools" (AI/LLM eval, browser automation) is required to surface a meaningful list
2. Confirmed repos (TypeScript, created >= 2022, stars > 1,000): `promptfoo/promptfoo` (~21k stars, LLM evaluation, 2022-07-01), `antiwork/shortest` (~5.6k stars, AI-powered browser testing, 2024-09-18), `wellwelwel/poku` (~1.2k stars, modern Node test runner, 2024-02-13)
3. Each entry should report: repo name, star count, creation date, and a description
4. Repos are sorted by stars descending; `promptfoo` dominates by star count
5. Agent notes any bias in results (topic search may return AI tools rather than pure testing frameworks; AI-eval tools dominate post-2022 TypeScript testing space)

### Q14 — What were the last 5 merged PRs in `facebook/react`?

1. Returns 5 PR numbers with titles and dates
2. All 5 most-recent merged PRs (as of 2026-05-25) are `[rust-compiler]` SWC e2e parity fixes merged in a stack on 2026-05-21: #36512 (bump round_trip test thread stack), #36511 (document parity TODOs), #36510 (inject CLI filename into PluginOptions), #36509 (apply compiler renames to SWC module), #36508 (treat redeclared functions as one binding)
3. PR numbers are in the 36,000–37,000 range based on current repo state (upper end of the 28k–37k window)
4. All 5 are Rust compiler SWC work — no Fizz/DevTools or JS fixes in the current top 5
5. Agent does NOT fabricate PR numbers — all entries are verifiable from the API response

### Q15 — Review PR #27733 in `facebook/react`

1. PR adds "reload and profile" support to `react-devtools-inline` for use in browser-extension or REPL contexts where `location.reload()` is unavailable
2. Files: `react-devtools-inline/src/backend.js` (exposes `attach`), `react-devtools-inline/src/frontend.js` (adds `reload?` param), `react-devtools-shared/src/devtools/index.js` (removed), `react-devtools-shell/src/app/devtools.js` (updated)
3. Core disagreement: reviewer (@hoxyq) wants the reload-profile flow to use `__REACT_DEVTOOLS_ATTACH__` global (self-contained); author needs an exported `attach` function for external orchestration in his web-extension use case
4. Not merged: PR is still **open and stale** (last updated March 2026, created November 2023); causes include unresolved design disagreement, reviewer bandwidth, potential `react-devtools-inline` deprecation, and stale-bot cycle

### Q16 — Have both React and Next.js had hydration-related PRs merged recently?

1. React: finds a recent merged PR with "hydration" in title (e.g., Strict Mode double-invoke during hydration, or hydration error improvements)
2. Next.js: finds a recent merged PR with "hydration" in title (e.g., dev-mode hydration failure fix, or hydration mismatch error improvements)
3. Both PRs include PR number, title, and merge date
4. React's hydration work is in `packages/react-reconciler/src/ReactFiberHydrationContext.js`
5. Agent honestly reports any tool limitation if text-search filtering is unreliable, and falls back to code search for context

### Q17 — When did `vercel/next.js` bump its React peer dependency to require React 18?

1. A specific PR number is found (not fabricated)
2. PR title references bumping React dev dep or peer dep to 18.x
3. Merge date is in 2022 (likely mid-2022 timeframe around Next.js 13)
4. Current `packages/next/package.json` peerDeps shows `"react": "^18.2.0 || 19.0.0-rc-de68d2f4-20241204 || ^19.0.0"` (includes a pinned RC version alongside the semver ranges)
5. Agent reports if PR search was unreliable and falls back to code evidence from `package.json`

### Q18 — How does Next.js call into React's streaming renderer?

1. Next.js file: `packages/next/src/server/app-render/stream-ops.web.ts` imports `renderToReadableStream` from `react-dom/server`
2. React file: `packages/react-dom/src/server/ReactDOMFizzServerBrowser.js` defines and exports `renderToReadableStream`
3. Full call chain: `app-render.tsx` → `stream-ops.web.ts` → `ReactDOMFizzServerBrowser.js` → `ReactFizzServer.js` (`createRequest` + `startWork` + `startFlowing`)
4. Next.js also uses `resume()` (from same React file) for PPR dynamic shell resumption
5. RSC flight stream uses a separate `renderToReadableStream` from the RSC renderer, not the Fizz HTML renderer

### Q19 — Do TanStack Query, zustand, and Recoil use `useSyncExternalStore`?

1. **TanStack Query:** ✅ found in `packages/react-query/src/useBaseQuery.ts` (and/or `useMutation.ts`); uses `React.useSyncExternalStore` to subscribe to query cache
2. **zustand:** ✅ found in `src/react.ts`; `React.useSyncExternalStore(api.subscribe, selector, selector)` — also in `src/traditional.ts` via `useSyncExternalStoreWithSelector`
3. **Recoil:** ❌ not found; Recoil predates `useSyncExternalStore` and uses its own internal atom subscription system
4. Agent provides exact file paths for confirmed usages
5. TanStack Query moved to `useSyncExternalStore` as a React 18 requirement in v5

### Q20 — How do Vue 3 and React each reconcile the virtual DOM?

1. **Vue:** `packages/runtime-core/src/renderer.ts` — `baseCreateRenderer()` returns `render` and `createApp`; core entry is the `patch(n1, n2, container)` function
2. **React:** `packages/react-reconciler/src/ReactFiberWorkLoop.js` — `workLoopConcurrent()` iterates over the fiber tree; `beginWork()` dispatches by fiber tag
3. Vue's `patch()` uses compiler-generated `patchFlag` and `dynamicChildren` to skip static nodes — O(dynamic nodes); React walks the entire fiber tree with bailouts via `memo` — O(all nodes in subtree)
4. Vue batches via microtask queue (`queueFlush`); React uses `Scheduler` with priority lanes and can yield mid-render (`shouldYield()`) for time-slicing
5. Fundamental difference: Vue combines reactivity (knows *what* changed) + VDOM (knows *how* it renders) → targeted updates; React has no reactivity → must re-render full subtree from dirty root

### Q21 — How do Vue 3's reactivity system and React's hook system each track state dependencies?

1. **Vue:** `packages/reactivity/src/dep.ts` — `track()` runs on Proxy `get`, links `activeSub` (current running effect) to the dependency; `trigger()` on Proxy `set` notifies all linked subscribers
2. **React:** `packages/react-reconciler/src/ReactFiberHooks.js` — `dispatchSetState` enqueues an `Update` with a `lane` (priority), calls `scheduleUpdateOnFiber`; no automatic dependency tracking
3. Vue: re-renders only components that accessed a changed reactive property; React: re-renders the entire component and its subtree (unless memoized)
4. Vue gotcha: destructuring a reactive object loses reactivity (must use `toRefs`); React gotcha: stale closures in `useEffect`, hook rules (no conditional calls)
5. Vue tracks at property granularity (Proxy per-key); React tracks at component granularity (fiber per component)

### Q22 — How does Svelte 5 implement the `$state` rune at the runtime level?

1. Core file: `packages/svelte/src/internal/client/reactivity/sources.js`
2. Key function: `source(initial_value)` creates a reactive signal; `mutable_source(initial_value)` is used for mutable state (e.g., objects/arrays)
3. `internal_set(source, value)` performs the mutation and notifies subscribers when the value changes (checked via `source.equals`)
4. Svelte compiles `$state` declarations into getter/setter accessors backed by these runtime signals — no virtual DOM involved
5. Effects and component renders that read a signal are tracked via a subscriber list on the signal node; mutations re-run only those dependent effects (fine-grained reactivity)

### Q23 — How does Vue 3's template compiler parse a `v-if` directive?

1. Transform plugin: `transformIf` in `packages/compiler-core/src/transforms/vIf.ts`
2. Parses `v-if`, `v-else-if`, `v-else` siblings and groups them into a single `IfNode` (`NodeTypes.IF`)
3. Each branch becomes an `IfBranchNode` with a `condition` expression and a `children` array
4. The codegen then emits a ternary expression chain: `condition ? consequent : alternate`
5. Unlike React's JSX `{condition && <Component />}`, Vue's compiler handles all branches together as a single structural node, allowing the runtime to optimize diffing between branches

### Q24 — What are the top-level configuration keys in Next.js's config schema?

1. File is `packages/next/src/server/config-schema.ts` (or `config-shared.ts` depending on version)
2. Top-level keys include: `env`, `webpack`, `redirects`, `rewrites`, `headers`, `pageExtensions`, `images`, `reactStrictMode`, `output`, `basePath`, `distDir`, `i18n`, `trailingSlash`, `compress`, `poweredByHeader`
3. `experimental` object contains sub-options including `ppr` (Partial Prerendering — `boolean | 'incremental'`), `serverActions`, `turbo`/`turbopack`, `reactCompiler`, `viewTransition`, `dynamicIO`, `useCache`, `after`
4. The schema uses Zod for runtime validation; keys are defined as `z.object(...)` shapes and composed into a top-level `configSchema`

### Q25 — How is `vitejs/vite` organized inside `packages/vite/src/`?

1. `node/` — dev server and build logic: subdirs `server/`, `optimizer/` (dep pre-bundling with esbuild), `plugins/` (built-in transform plugins), `ssr/`; plus top-level files `build.ts`, `config.ts`, `constants.ts`, `logger.ts`, etc.
2. `client/` — browser-side runtime: HMR client (`client.ts`), env constants (`env.ts`), overlay UI (`overlay.ts`)
3. `module-runner/` — ESM module runner for SSR and Environments API: `runner.ts`, `esmEvaluator.ts`, `hmrHandler.ts`, source maps; enables executing modules in non-browser environments without pre-bundling
4. `node/server/` — Vite dev server internals: `middlewares/`, `hmr.ts`, `ws.ts` (WebSocket for HMR), `moduleGraph.ts`
5. `shared/` — utilities shared between node and client (HMR protocol, constants, SSR transform); `types/` — TypeScript public API type definitions

### Q26 — Which JavaScript bundlers have the most GitHub stars? Compare top 5.

1. vite: ~80k+ stars (80,817 as of 2026-05-25), very actively maintained (pushed within days)
2. webpack: ~65k+ stars (65,770), actively maintained, highest open issue count
3. parcel: ~44k+ stars (44,029), actively maintained
4. esbuild: ~40k+ stars (39,913), maintained by solo maintainer (evanw)
5. rollup: ~26k+ stars (26,284), actively maintained (used by vite under the hood for production builds); star ranking: vite > webpack > parcel > esbuild > rollup

### Q27 — When was the React `use()` hook introduced?

1. `use()` hook is exported from `packages/react/src/ReactHooks.js` — agent finds this file as evidence
2. PR was merged in 2022–2023 timeframe (React 18.x / 19 canary era)
3. PR title likely references "use hook", "RFC: use()", or "Thenables in hooks"
4. Key change: added `use` function that accepts Context or a Promise/Thenable
5. Related to React's async hooks work and the Suspense data-fetching primitive

### Q28 — What were the last 5 merged PRs in `vitejs/vite`?

1. Returns 5 PR numbers with titles and dates — all verifiable from API response
2. Most recent PRs from 2026 (active repo): #22497 fix(glob) 2026-05-25, #22514 test(resolve) 2026-05-25, #22509 fix(resolve) viteResolvePlugin warning 2026-05-25, #22484 feat rolldown 1.0.2 bump 2026-05-21, #22483 chore ws security bump 2026-05-21
3. Mix of bug fixes, dependency/security bumps, and test additions
4. PRs reference glob module resolution (#22497), plugin container warnings (#22509), and rolldown integration (#22484)
5. Agent does NOT fabricate PR numbers

### Q29 — How does Next.js implement Partial Prerendering (PPR)?

1. Core file: `packages/next/src/server/app-render/dynamic-rendering.ts` — exports a `Postpone()` function that throws a "postpone" to signal a dynamic boundary
2. Uses React's experimental `unstable_postpone` (from `react`) to interrupt static rendering at a Suspense boundary
3. Static shell rendered at build time; `postponedState` blob stored for later dynamic resumption
4. At request time: `resume()` from `react-dom/server` receives the `postponedState` and streams the dynamic parts
5. `isRoutePPREnabled` flag in `packages/next/src/server/app-render/types.ts` gates the behavior; `app-render.tsx` checks it before choosing between static and PPR render paths

### Q30 — How does Solid.js implement `createSignal`? How does it differ from React's `useState`?

1. `createSignal` is in `packages/solid/src/reactive/signal.ts` (or `packages/solid/src/reactive/`)
2. Returns `[getter, setter]` where `getter()` is a function (unlike React's plain value) — reading it inside an effect auto-tracks the dependency
3. Solid uses an owner/observer graph: when `getter()` is called during an effect or memo, the signal registers the current observer as a subscriber
4. When `setter()` is called, only the subscribed observers (effects, memos, component render functions) re-run — NOT the entire component tree
5. Key difference from `useState`: React re-renders the whole component function; Solid re-runs only the specific reactive expression that read the changed signal (no virtual DOM diffing needed)

### Q31 — How do Svelte 5 and Vue 3 each approach compile-time vs runtime reactivity?

_No fact list — Q31 is ungraded until a 5-fact block is added. Treat it as open-ended for now and exclude from `/93` totals._

---

## Maintenance

- **Verified:** 2026-05-25 (full re-verification with live GitHub data)
- **Drift-sensitive facts** (re-check at every refresh): Q12 stars/issue counts, Q13 repo list and star counts, Q14 PR numbers and titles, Q17 peerDeps string and PR identity, Q26 star rankings, Q28 PR numbers and titles, Q9 top-level dir list for `vercel/next.js`.
- **Star counts as of 2026-05-25:** zustand 58,116 · jotai 21,173 · redux-toolkit 11,215 · vite 80,817 · webpack 65,770 · parcel 44,029 · esbuild 39,913 · rollup 26,284
- **Never** add Expected Facts to `QUESTIONS.md`. The split exists so the agents never see the answer key.
