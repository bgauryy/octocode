# Questions

31 GitHub research questions. Answer each one, in order, using only the tool you were assigned.

Answer key (Expected Facts) is in [`EXPECTED_FACTS.md`](./EXPECTED_FACTS.md) — judge-only.

---

### Q1 — Where is `useState` implemented in React?

Find where `useState` is defined in `facebook/react`. Which dispatcher method does it call?

### Q2 — Primary navigation push across three routers

Find the core navigation primitive in `vercel/next.js`, `remix-run/react-router`, and `TanStack/router`.

### Q3 — How does React capture a thrown promise in Suspense?

In `facebook/react`, find what happens when a component throws a promise (wakeable). What function handles it and what does it do?

### Q4 — Does Next.js App Router use React's `startTransition`?

Search `vercel/next.js` for `startTransition` usage in client code. What does it wrap?

### Q5 — SSR entry points: Next.js, Remix, Nuxt

Identify the server render function and the streaming/string API it calls in `vercel/next.js`, `remix-run/react-router`, and `nuxt/nuxt`.

### Q6 — Signature of `renderToReadableStream`

Read the React server renderer file that exports `renderToReadableStream` in `facebook/react`. What is the function signature and what options does it accept?

### Q7 — Top-level imports in `app-render.tsx`

Read the first 50 lines of `packages/next/src/server/app-render/app-render.tsx` in `vercel/next.js`. What modules does it import from?

### Q8 — `zustand`'s `createStore` type signature

Read `src/vanilla.ts` in `pmndrs/zustand`. What is the TypeScript type of `createStore`, including its generics?

### Q9 — Top-level layout of `vercel/next.js`

List the top-level directories and describe the purpose of each.

### Q10 — Subsystems inside `packages/next/src/server/`

List subdirectories and notable files. What subsystems can you identify?

### Q11 — `packages/` organization across three repos

List the package names under `packages/` for `facebook/react`, `vercel/next.js`, `remix-run/react-router`. What does the naming convention reveal about each project's architecture?

### Q12 — Most actively maintained: zustand vs jotai vs redux-toolkit

Compare `pmndrs/zustand`, `pmndrs/jotai`, `reduxjs/redux-toolkit` on stars, last push date, and open issue count.

### Q13 — Top TypeScript testing repos created since 2022

Find TypeScript testing repos with more than 1,000 stars created since 2022.

### Q14 — Last 5 merged PRs in `facebook/react`

List by PR number, title, and merge date.

### Q15 — Review PR #27733 in `facebook/react`

PR: https://github.com/facebook/react/pull/27733

Answer all four:
1. What problem does this PR solve?
2. Which files does it change and what does each change do?
3. What is the core technical disagreement in the review thread?
4. Why has it not been merged?

### Q16 — Recent hydration PRs in React and Next.js

Search both repos for recently merged PRs mentioning "hydration". Report the most recent from each.

### Q17 — When did Next.js bump React peer dep to 18?

Find the PR. Report number, title, and merge date.

### Q18 — Next.js → React's streaming renderer

Trace the path: which Next.js file calls `renderToReadableStream`, and which React file defines it?

### Q19 — `useSyncExternalStore` adoption

For `TanStack/query`, `pmndrs/zustand`, `facebookexperimental/Recoil`: report found / not found and file path if found.

### Q20 — Vue 3 vs React reconciliation

Using `vuejs/core` and `facebook/react`:
1. Find the core reconciler/renderer entry point in each repo.
2. How does each decide what changed? (Vue's `patch()` vs React's fiber work loop)
3. How does each schedule and commit updates?
4. What is the fundamental architectural difference in their reconciliation strategies?

### Q21 — Vue 3 reactivity vs React hooks: dependency tracking

Using `vuejs/core` (`packages/reactivity/src/`) and `facebook/react` (`packages/react-reconciler/src/`):
1. Find the state/reactivity primitive in each.
2. How does Vue track which components depend on which reactive values?
3. How does React know which components to re-render?
4. What are the practical consequences of each approach for the developer?

### Q22 — Svelte 5 `$state` runtime

In `sveltejs/svelte`, find where `$state` values are represented as reactive signals at runtime. What is the core primitive and how is state mutation tracked?

### Q23 — Vue 3 template compiler: `v-if`

In `vuejs/core`, find the function that processes `v-if` / `v-else-if` / `v-else` during template compilation. What AST node does it produce?

### Q24 — Next.js config schema

Read `packages/next/src/server/config-schema.ts` in `vercel/next.js`. What are the top-level config keys and what does the `experimental` object contain?

### Q25 — `vitejs/vite` inside `packages/vite/src/`

Browse `packages/vite/src/`. What are the main subsystems and what does each handle?

### Q26 — Top JavaScript bundlers by stars

Compare `vitejs/vite`, `evanw/esbuild`, `webpack/webpack`, `rollup/rollup`, `parcel-bundler/parcel` on stars, last push date, and open issue count.

### Q27 — When was React's `use()` hook introduced?

Find the PR that first shipped the `use()` hook API in `facebook/react`. Report PR number, title, merge date, and key files added or modified.

### Q28 — Last 5 merged PRs in `vitejs/vite`

List by PR number, title, and merge date.

### Q29 — Next.js Partial Prerendering (PPR)

Trace the mechanism: which Next.js file contains the `Postpone` component, how does it interact with React, and how is the static shell assembled?

### Q30 — Solid.js `createSignal` vs React `useState`

Find `createSignal` in `solidjs/solid`. What mechanism makes updates fine-grained rather than component-level?

### Q31 — Svelte 5 vs Vue 3: compile-time vs runtime reactivity

Using `sveltejs/svelte` and `vuejs/core`:
1. Where does reactivity live: compiler, runtime, or both?
2. Find the Svelte compiler's output for a `$state` variable vs Vue's `ref()` runtime call.
3. How does each framework know which part of the DOM to update when state changes?
4. What is the fundamental tradeoff between compile-time and runtime reactivity?
