# Run octocode

| Agent | Questions | Calls | In Chars | Out Chars | Tool ms | Q wall ms | Reasoning ms |
|-------|----------:|------:|---------:|----------:|--------:|----------:|-------------:|
| octocode | 46 / 46 | 49 | 20,241 | 220,328 | 108,095 | 185,238 | 77,143 |

> **Tool ms** = Σ wall time on tool calls. **Q wall ms** = Σ wall time per question from `set-q.sh` to `record.sh`. **Reasoning ms** = Q wall − Tool (approx time the LLM spent thinking between calls).

## MCP init context (one-time per-session cost)

| Calls | In chars | Out chars (schemas + instructions loaded into agent context) | ms |
|------:|---------:|-------------------------------------------------------------:|---:|
| 49 | 98 | 339,227 | 20,654 |

Breakdown: `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars.

> This cost is counted against octocode (loaded once at session start). gh has no equivalent context-loading step — surfacing it is what makes the comparison honest.


| Q | Calls | In Chars | Out Chars | Tool ms | Q wall ms | Reasoning ms | Answer (one line) |
|---|------:|---------:|----------:|--------:|----------:|-------------:|-------------------|
| Q1 | 1 | 261 | 1,330 | 695 | 49,719 | 49,024 | - `useState` is exported from `packages/react/src/ReactHook… |
| Q2 | 1 | 818 | 4,481 | 4,572 | 5,088 | 516 | - In `vercel/next.js`, the App Router navigation primitive … |
| Q3 | 1 | 348 | 564 | 781 | 1,290 | 509 | - React captures a thrown promise/wakeable in `packages/rea… |
| Q4 | 1 | 334 | 3,314 | 717 | 1,222 | 505 | - Yes: Next.js App Router imports and uses React's `startTr… |
| Q5 | 1 | 831 | 4,484 | 4,636 | 5,137 | 501 | - `vercel/next.js`: the App Router server renderer is in `p… |
| Q6 | 1 | 307 | 1,883 | 809 | 1,323 | 514 | - `renderToReadableStream` is exported from React DOM serve… |
| Q7 | 1 | 296 | 1,961 | 1,877 | 2,399 | 522 | - `packages/next/src/server/app-render/app-render.tsx` impo… |
| Q8 | 1 | 296 | 1,738 | 1,321 | 1,826 | 505 | - In `pmndrs/zustand` `src/vanilla.ts`, `createStore` has t… |
| Q9 | 1 | 244 | 2,349 | 1,058 | 1,569 | 511 | - `packages/` contains the main Next.js packages including … |
| Q10 | 1 | 260 | 8,134 | 4,676 | 5,253 | 577 | - `packages/next/src/server/` includes routing/rendering su… |
| Q11 | 1 | 719 | 3,457 | 1,037 | 1,522 | 485 | - `facebook/react` `packages/` is a many-package renderer/r… |
| Q12 | 1 | 673 | 2,550 | 5,049 | 5,635 | 586 | - `pmndrs/zustand`, `pmndrs/jotai`, and `reduxjs/redux-tool… |
| Q13 | 1 | 317 | 2,362 | 1,565 | 2,107 | 542 | - Top TypeScript testing repositories created since 2022 wi… |
| Q14 | 1 | 278 | 10,329 | 2,047 | 2,603 | 556 | - `- #28596`; `"Guard against legacy context not being supp… |
| Q15 | 1 | 303 | 25,963 | 3,119 | 3,618 | 499 | - `facebook/react` PR `#27733` is an unmerged React PR; the… |
| Q16 | 1 | 605 | 18,163 | 3,947 | 4,461 | 514 | - `facebook/react`: the most recent merged PR mentioning `h… |
| Q17 | 1 | 325 | 13,561 | 2,246 | 2,743 | 497 | - The Next.js React 18 peer dependency bump is in a merged … |
| Q18 | 1 | 653 | 3,655 | 2,590 | 3,117 | 527 | - Next.js calls/imports `renderToReadableStream` in `packag… |
| Q19 | 1 | 770 | 3,508 | 4,501 | 5,029 | 528 | - `TanStack/query`: `useSyncExternalStore` is found in Reac… |
| Q20 | 1 | 592 | 2,472 | 2,552 | 3,101 | 549 | - Vue's renderer/reconciler entry is `packages/runtime-core… |
| Q21 | 1 | 627 | 923 | 2,578 | 3,304 | 726 | - Vue's primitive lives in `packages/reactivity/src/`: `ref… |
| Q22 | 1 | 296 | 3,649 | 925 | 1,713 | 788 | - In `sveltejs/svelte`, `$state` is a rune compiled to runt… |
| Q23 | 1 | 294 | 659 | 749 | 1,584 | 835 | - Vue processes `v-if` / `v-else-if` / `v-else` in `package… |
| Q24 | 2 | 629 | 1,461 | 2,663 | 4,085 | 1,422 | - `packages/next/src/server/config-schema.ts` defines a Zod… |
| Q25 | 1 | 264 | 2,754 | 2,089 | 2,843 | 754 | - `packages/vite/src/client/` contains browser client/HMR r… |
| Q26 | 2 | 1,044 | 4,990 | 7,938 | 9,200 | 1,262 | - Compare `vitejs/vite`, `evanw/esbuild`, `webpack/webpack`… |
| Q27 | 1 | 283 | 9,945 | 2,111 | 2,847 | 736 | - React's `use()` hook was introduced in a merged `facebook… |
| Q28 | 1 | 266 | 9,076 | 1,926 | 2,516 | 590 | - The last five merged PRs in `vitejs/vite` are the five to… |
| Q29 | 1 | 599 | 2,920 | 2,608 | 3,147 | 539 | - Next.js PPR uses a `Postpone` component in `packages/next… |
| Q30 | 1 | 288 | 1,478 | 817 | 1,406 | 589 | - `solidjs/solid` implements `createSignal` in its reactive… |
| Q31 | 1 | 561 | 3,914 | 2,553 | 3,158 | 605 | - Svelte 5 reactivity is both compiler and runtime: the com… |
| Q32 | 1 | 292 | 1,139 | 1,002 | 1,578 | 576 | - In `colinhacks/zod` v4 core, `parse` is defined in `packa… |
| Q33 | 1 | 289 | 1,522 | 1,520 | 2,113 | 593 | - `honojs/hono` implements middleware chaining in `src/comp… |
| Q34 | 1 | 337 | 2,646 | 1,339 | 1,919 | 580 | - `initTRPC.create()` in `packages/server/src/unstable-core… |
| Q35 | 1 | 310 | 3,399 | 1,448 | 1,986 | 538 | - `runTest(test, runner)` in `packages/runner/src/run.ts` m… |
| Q36 | 1 | 365 | 3,099 | 1,417 | 1,983 | 566 | - `extractDirectives()` in `packages/astro/src/runtime/serv… |
| Q37 | 1 | 376 | 2,629 | 1,417 | 1,962 | 545 | - `renderPage()` is defined in `packages/astro/src/runtime/… |
| Q38 | 1 | 243 | 574 | 927 | 1,494 | 567 | - `trpc/trpc` `packages/` includes core server/client packa… |
| Q39 | 1 | 262 | 1,473 | 973 | 1,547 | 574 | - `biomejs/biome` has top-level directories for Rust crates… |
| Q40 | 1 | 609 | 3,528 | 4,970 | 5,507 | 537 | - Compare `prisma/prisma`, `drizzle-team/drizzle-orm`, and … |
| Q41 | 1 | 276 | 8,251 | 1,976 | 2,660 | 684 | - The last five merged PRs in `vitest-dev/vitest` are the f… |
| Q42 | 1 | 324 | 3,834 | 1,434 | 2,170 | 736 | - `honojs/hono` defines `Context` in `src/context.ts` with … |
| Q43 | 1 | 469 | 2,003 | 1,154 | 1,872 | 718 | - `withastro/astro` top-level directories include `packages… |
| Q44 | 1 | 595 | 1,404 | 2,693 | 3,224 | 531 | - Astro's SSR entry is `packages/astro/src/runtime/server/r… |
| Q45 | 1 | 270 | 23,166 | 3,003 | 3,514 | 511 | - `trpc/trpc` PR `#7336` fixes a React 19 compatibility pro… |
| Q46 | 2 | 843 | 3,634 | 6,070 | 7,144 | 1,074 | - Compare `colinhacks/zod`, `trpc/trpc`, `honojs/hono`, and… |
| **Σ** | **49** | **20,241** | **220,328** | **108,095** | **185,238** | **77,143** | |
