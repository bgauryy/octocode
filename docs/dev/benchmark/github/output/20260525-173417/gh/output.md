# Run gh

| Agent | Questions | Calls | In Chars | Out Chars | Tool ms | Q wall ms | Reasoning ms |
|-------|----------:|------:|---------:|----------:|--------:|----------:|-------------:|
| gh | 46 / 46 | 202 | 23,325 | 9,165,081 | 244,314 | 1,718,746 | 1,474,432 |

> **Tool ms** = Σ wall time on tool calls. **Q wall ms** = Σ wall time per question from `set-q.sh` to `record.sh`. **Reasoning ms** = Q wall − Tool (approx time the LLM spent thinking between calls).


| Q | Calls | In Chars | Out Chars | Tool ms | Q wall ms | Reasoning ms | Answer (one line) |
|---|------:|---------:|----------:|--------:|----------:|-------------:|-------------------|
| Q1 | 2 | 160 | 18,612 | 2,577 | 18,184 | 15,607 | - `useState` is defined in `packages/react/src/ReactHooks.j… |
| Q2 | 13 | 1,201 | 712,395 | 16,488 | 86,770 | 70,282 | - **vercel/next.js**: core navigation primitive is `router.… |
| Q3 | 4 | 407 | 112,542 | 4,761 | 34,899 | 30,138 | - When a component throws a promise (wakeable), the functio… |
| Q4 | 4 | 432 | 95,227 | 4,645 | 31,983 | 27,338 | - Yes, Next.js App Router uses React's `startTransition` in… |
| Q5 | 13 | 1,359 | 100,099 | 16,645 | 96,212 | 79,567 | - **vercel/next.js**: Server render function is in `package… |
| Q6 | 6 | 718 | 20,569 | 6,235 | 45,446 | 39,211 | - The file is `packages/react-dom/src/server/ReactDOMFizzSe… |
| Q7 | 2 | 196 | 818,038 | 7,476 | 45,649 | 38,173 | - `packages/next/src/server/app-render/app-render.tsx` impo… |
| Q8 | 2 | 124 | 9,056 | 2,421 | 19,707 | 17,286 | - `createStore` is typed as `CreateStore` in `src/vanilla.t… |
| Q9 | 1 | 72 | 2,694 | 924 | 16,011 | 15,087 | - `packages/` — the main Next.js npm packages (next, create… |
| Q10 | 1 | 96 | 3,371 | 1,277 | 18,745 | 17,468 | - **app-render/**: App Router rendering pipeline (RSC, stre… |
| Q11 | 3 | 191 | 1,392 | 2,867 | 29,018 | 26,151 | - **facebook/react** packages: `react`, `react-dom`, `react… |
| Q12 | 3 | 323 | 194 | 3,994 | 23,238 | 19,244 | - **pmndrs/zustand**: 58,115 stars, last push 2026-05-22, 5… |
| Q13 | 1 | 207 | 759 | 3,142 | 22,029 | 18,887 | - `promptfoo/promptfoo` — 21,582 stars, created 2023-04-28 … |
| Q14 | 3 | 539 | 1,542 | 3,935 | 28,426 | 24,491 | - PR #36512 — "[rust-compiler] Bump round_trip test thread … |
| Q15 | 3 | 346 | 16,139 | 3,962 | 33,225 | 29,263 | - **Problem solved**: Exposes the `attach` function from `r… |
| Q16 | 2 | 372 | 699 | 2,667 | 15,865 | 13,198 | - **facebook/react** most recent hydration-related merged P… |
| Q17 | 22 | 3,916 | 1,930,100 | 40,852 | 230,908 | 190,056 | - PR #41629 — "BREAKING CHANGE: Remove React 17" — merged 2… |
| Q18 | 4 | 371 | 26,916 | 3,857 | 35,369 | 31,512 | - **Next.js file that calls `renderToReadableStream`**: `pa… |
| Q19 | 3 | 313 | 443 | 3,503 | 21,052 | 17,549 | - **TanStack/query**: FOUND — `packages/react-query/src/use… |
| Q20 | 14 | 1,344 | 1,695,408 | 18,826 | 115,428 | 96,602 | - **Vue 3 reconciler entry point**: `packages/runtime-core/… |
| Q21 | 5 | 437 | 728,868 | 6,980 | 49,980 | 43,000 | - **Vue 3 state primitive**: `ref()` / `reactive()` in `pac… |
| Q22 | 5 | 526 | 40,679 | 5,764 | 44,062 | 38,298 | - In `sveltejs/svelte`, `$state` values are represented as … |
| Q23 | 4 | 378 | 28,330 | 4,795 | 36,502 | 31,707 | - The function is `transformIf` in `packages/compiler-core/… |
| Q24 | 7 | 623 | 263,298 | 8,073 | 65,603 | 57,530 | - Top-level config keys in `packages/next/src/server/config… |
| Q25 | 4 | 336 | 2,013 | 5,104 | 47,203 | 42,099 | - `packages/vite/src/node/` is the main server subsystem wi… |
| Q26 | 5 | 533 | 330 | 4,623 | 20,231 | 15,608 | - **vitejs/vite**: 80,819 stars, last push 2026-05-25, 729 … |
| Q27 | 10 | 1,828 | 8,330 | 12,352 | 82,174 | 69,822 | - PR #25084 — "experimental_use(promise)" — merged 2022-08-… |
| Q28 | 1 | 173 | 607 | 1,030 | 13,161 | 12,131 | - PR #22514 — "test(resolve): add test for sass @use with n… |
| Q29 | 6 | 651 | 160,120 | 5,819 | 51,827 | 46,008 | - The `Postpone` component is defined in `packages/next/src… |
| Q30 | 9 | 760 | 401,577 | 9,124 | 73,242 | 64,118 | - `createSignal` is defined in `packages/solid/src/reactive… |
| Q31 | 9 | 1,033 | 78,654 | 7,399 | 79,500 | 72,101 | - **Where reactivity lives**: Svelte 5 — primarily **compil… |
| Q32 | 1 | 81 | 10,453 | 565 | 11,314 | 10,749 | - `parse` for Zod v4 core is implemented in `colinhacks/zod… |
| Q33 | 1 | 59 | 2,990 | 611 | 7,585 | 6,974 | - `compose()` is implemented in `honojs/hono` at `src/compo… |
| Q34 | 1 | 102 | 8,720 | 590 | 7,804 | 7,214 | - `initTRPC` is `export const initTRPC = new TRPCBuilder()`… |
| Q35 | 2 | 154 | 87,624 | 1,515 | 17,129 | 15,614 | - `runTest(test, runner)` is exported from `vitest-dev/vite… |
| Q36 | 1 | 95 | 7,045 | 611 | 8,627 | 8,016 | - Astro extracts island hydration directives in `withastro/… |
| Q37 | 1 | 97 | 7,541 | 610 | 9,029 | 8,419 | - `renderPage()` is defined in `withastro/astro` at `packag… |
| Q38 | 2 | 165 | 474 | 1,136 | 12,427 | 11,291 | - `trpc/trpc` currently has these directories under `packag… |
| Q39 | 2 | 118 | 2,993 | 1,375 | 14,786 | 13,411 | - Top-level `biomejs/biome` directories include `.cargo`, `… |
| Q40 | 3 | 541 | 285 | 2,142 | 9,528 | 7,386 | - `prisma/prisma`: `46,008` stars, last pushed `2026-05-19T… |
| Q41 | 1 | 121 | 530 | 809 | 5,726 | 4,917 | - `#10446` — `perf: improve performance in hot paths` — mer… |
| Q42 | 2 | 118 | 60,116 | 1,395 | 13,846 | 12,451 | - `Context` is defined in `honojs/hono` at `src/context.ts`… |
| Q43 | 2 | 149 | 2,272 | 1,281 | 9,883 | 8,602 | - Top-level directories in `withastro/astro` include `.agen… |
| Q44 | 5 | 489 | 1,643,617 | 4,346 | 27,023 | 22,677 | - Astro SSR entry point: `withastro/astro` defines `renderP… |
| Q45 | 3 | 368 | 51,062 | 2,617 | 23,023 | 20,406 | - PR `#7336` in `trpc/trpc` is `fix(server): handle React 1… |
| Q46 | 4 | 703 | 358 | 2,594 | 9,367 | 6,773 | - `colinhacks/zod`: `42,772` stars, last pushed `2026-05-19… |
| **Σ** | **202** | **23,325** | **9,165,081** | **244,314** | **1,718,746** | **1,474,432** | |
