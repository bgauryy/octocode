# Run gh

| Agent | Questions | Calls | In Chars | Out Chars | Tool ms | Q wall ms | Reasoning ms |
|-------|----------:|------:|---------:|----------:|--------:|----------:|-------------:|
| gh | 20 / 20 | 75 | 7,774 | 2,828,586 | 59,071 | 234,651 | 175,580 |

> **Tool ms** = Σ wall time on tool calls. **Q wall ms** = Σ wall time per question from `set-q.sh` to `record.sh`. **Reasoning ms** = Q wall − Tool (approx time the LLM spent thinking between calls).


| Q | Calls | In Chars | Out Chars | Tool ms | Q wall ms | Reasoning ms | Answer (one line) |
|---|------:|---------:|----------:|--------:|----------:|-------------:|-------------------|
| Q1 | 14 | 1,402 | 768,003 | 12,285 | 43,644 | 31,359 | - `vercel/next.js`: the Pages Router primitive is `Router.p… |
| Q2 | 1 | 96 | 37,403 | 931 | 1,028 | 97 | - In `facebook/react`, a thrown promise/thenable is handled… |
| Q3 | 2 | 216 | 20,653 | 1,726 | 8,048 | 6,322 | - Yes. `vercel/next.js` uses React `startTransition` in cli… |
| Q4 | 6 | 599 | 455,721 | 4,637 | 15,877 | 11,240 | - `vercel/next.js`: App Router SSR is in `packages/next/src… |
| Q5 | 3 | 176 | 1,248 | 1,999 | 2,196 | 197 | - `facebook/react` `packages/`: many narrowly scoped React … |
| Q6 | 1 | 112 | 33,377 | 1,240 | 10,420 | 9,180 | - Problem: PR `facebook/react#27733` tries to make `react-d… |
| Q7 | 9 | 1,145 | 38,628 | 6,303 | 32,035 | 25,732 | - Next.js call site: `packages/next/src/server/app-render/a… |
| Q8 | 8 | 904 | 46,646 | 5,803 | 30,998 | 25,195 | - `TanStack/query`: found. React package uses `React.useSyn… |
| Q9 | 2 | 180 | 374,965 | 1,885 | 2,050 | 165 | - Vue entry point: `vuejs/core` `packages/runtime-core/src/… |
| Q10 | 3 | 244 | 271,427 | 2,299 | 2,509 | 210 | - Vue primitive: `ref()`/`reactive()` in `packages/reactivi… |
| Q11 | 6 | 694 | 48,102 | 5,234 | 23,533 | 18,299 | - Svelte 5 `$state` runtime representation is in `packages/… |
| Q12 | 2 | 203 | 14,030 | 1,360 | 1,528 | 168 | - Vue 3 handles `v-if` / `v-else-if` / `v-else` in `package… |
| Q13 | 5 | 541 | 498,134 | 3,923 | 17,999 | 14,076 | - `Postpone` is in `packages/next/src/server/app-render/dyn… |
| Q14 | 2 | 206 | 80,761 | 1,543 | 1,686 | 143 | - `createSignal` is defined in `solidjs/solid` at `packages… |
| Q15 | 3 | 302 | 42,969 | 1,951 | 11,718 | 9,767 | - Svelte 5 reactivity lives in both compiler and runtime: t… |
| Q16 | 2 | 196 | 10,456 | 1,294 | 7,646 | 6,352 | - Zod v4 core defines `parse` in `packages/zod/src/v4/core/… |
| Q17 | 1 | 59 | 2,990 | 687 | 785 | 98 | - `honojs/hono` implements `compose()` in `src/compose.ts`;… |
| Q18 | 1 | 77 | 43,812 | 798 | 871 | 73 | - `runTest(test, runner)` in `packages/runner/src/run.ts` m… |
| Q19 | 3 | 326 | 10,726 | 2,073 | 10,956 | 8,883 | - Astro SSR entry: `withastro/astro` `packages/astro/src/ru… |
| Q20 | 1 | 96 | 28,535 | 1,100 | 9,124 | 8,024 | - Problem: PR `trpc/trpc#7336` fixes React 19 coercing tRPC… |
| **Σ** | **75** | **7,774** | **2,828,586** | **59,071** | **234,651** | **175,580** | |
