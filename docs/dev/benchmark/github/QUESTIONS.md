# Questions

20 GitHub research questions. Answer each one, in order, using only the tool you were assigned.

Answer key (Expected Facts) is in [`EXPECTED_FACTS.md`](./EXPECTED_FACTS.md) — judge-only.

---

### Q1 — Primary navigation push across three routers

Find the core navigation primitive in `vercel/next.js`, `remix-run/react-router`, and `TanStack/router`.

### Q2 — How does React capture a thrown promise in Suspense?

In `facebook/react`, find what happens when a component throws a promise (wakeable). What function handles it and what does it do?

### Q3 — Does Next.js App Router use React's `startTransition`?

Search `vercel/next.js` for `startTransition` usage in client code. What does it wrap?

### Q4 — SSR entry points: Next.js, Remix, Nuxt

Identify the server render function and the streaming/string API it calls in `vercel/next.js`, `remix-run/react-router`, and `nuxt/nuxt`.

### Q5 — `packages/` organization across three repos

List the package names under `packages/` for `facebook/react`, `vercel/next.js`, `remix-run/react-router`. What does the naming convention reveal about each project's architecture?

### Q6 — Review PR #27733 in `facebook/react`

PR: https://github.com/facebook/react/pull/27733

Answer all four:
1. What problem does this PR solve?
2. Which files does it change and what does each change do?
3. What is the core technical disagreement in the review thread?
4. Why has it not been merged?

### Q7 — Next.js → React's streaming renderer

Trace the path: which Next.js file calls `renderToReadableStream`, and which React file defines it?

### Q8 — `useSyncExternalStore` adoption

For `TanStack/query`, `pmndrs/zustand`, `facebookexperimental/Recoil`: report found / not found and file path if found.

### Q9 — Vue 3 vs React reconciliation

Using `vuejs/core` and `facebook/react`:
1. Find the core reconciler/renderer entry point in each repo.
2. How does each decide what changed? (Vue's `patch()` vs React's fiber work loop)
3. How does each schedule and commit updates?
4. What is the fundamental architectural difference in their reconciliation strategies?

### Q10 — Vue 3 reactivity vs React hooks: dependency tracking

Using `vuejs/core` (`packages/reactivity/src/`) and `facebook/react` (`packages/react-reconciler/src/`):
1. Find the state/reactivity primitive in each.
2. How does Vue track which components depend on which reactive values?
3. How does React know which components to re-render?
4. What are the practical consequences of each approach for the developer?

### Q11 — Svelte 5 `$state` runtime

In `sveltejs/svelte`, find where `$state` values are represented as reactive signals at runtime. What is the core primitive and how is state mutation tracked?

### Q12 — Vue 3 template compiler: `v-if`

In `vuejs/core`, find the function that processes `v-if` / `v-else-if` / `v-else` during template compilation. What AST node does it produce?

### Q13 — Next.js Partial Prerendering (PPR)

Trace the mechanism: which Next.js file contains the `Postpone` component, how does it interact with React, and how is the static shell assembled?

### Q14 — Solid.js `createSignal` vs React `useState`

Find `createSignal` in `solidjs/solid`. What mechanism makes updates fine-grained rather than component-level?

### Q15 — Svelte 5 vs Vue 3: compile-time vs runtime reactivity

Using `sveltejs/svelte` and `vuejs/core`:
1. Where does reactivity live: compiler, runtime, or both?
2. Find the Svelte compiler's output for a `$state` variable vs Vue's `ref()` runtime call.
3. How does each framework know which part of the DOM to update when state changes?
4. What is the fundamental tradeoff between compile-time and runtime reactivity?

### Q16 — Where is `parse()` implemented in Zod v4?

Find where `parse` is defined in `colinhacks/zod` (v4 core). What internal method does it call to validate, and what happens when the validation result is a Promise?

### Q17 — How does Hono chain middleware? (`compose()`)

In `honojs/hono`, find `src/compose.ts`. How does `compose()` build and execute the middleware chain? What prevents `next()` from being called twice?

### Q18 — How does Vitest execute a single test? (`runTest`)

In `vitest-dev/vitest`, read `packages/runner/src/run.ts`. What are the key steps in `runTest(test, runner)`, and how does Vitest handle `retry` and `repeats`?

### Q19 — Astro vs Next.js: SSR rendering and component model

Using `withastro/astro` and `vercel/next.js`:
1. Find the SSR render entry function in each repo.
2. How does each stream HTML to the client?
3. How does Astro's islands model differ architecturally from Next.js's React Server Components?

### Q20 — Review PR #7336 in `trpc/trpc`

PR: https://github.com/trpc/trpc/pull/7336

Answer all four:
1. What problem does this PR solve?
2. Which files does it change and what does each change do?
3. What is the core technical fix?
4. Why does this issue specifically appear with React 19?
