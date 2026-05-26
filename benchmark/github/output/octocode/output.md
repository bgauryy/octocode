# Run octocode

| Agent | Questions | Calls | In Chars | Out Chars | Tool ms | Q wall ms | Reasoning ms |
|-------|----------:|------:|---------:|----------:|--------:|----------:|-------------:|
| octocode | 20 / 20 | 141 | 39,521 | 402,386 | 210,513 | 814,198 | 603,685 |

> **Tool ms** = Σ wall time on tool calls. **Q wall ms** = Σ wall time per question from `set-q.sh` to `record.sh`. **Reasoning ms** = Q wall − Tool (approx time the LLM spent thinking between calls).

## MCP init context (one-time per-session cost)

| Calls | In chars | Out chars (schemas + instructions loaded into agent context) | ms |
|------:|---------:|-------------------------------------------------------------:|---:|
| 57 | 114 | 547,477 | 19,213 |

Breakdown: `_initialize`=6,923 chars, `_tools/list`=159,789 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars, `_initialize`=6,923 chars.

> This cost is attributed to octocode (loaded once at session start). gh has no equivalent context-loading step — surfacing it is what makes the comparison honest.


| Q | Calls | In Chars | Out Chars | Tool ms | Q wall ms | Reasoning ms | Answer (one line) |
|---|------:|---------:|----------:|--------:|----------:|-------------:|-------------------|
| Q1 | 4 | 1,055 | 7,148 | 5,040 | 17,847 | 12,807 | - Next.js call site: `packages/next/src/server/app-render/a… |
| Q2 | 4 | 1,135 | 13,722 | 22,960 | 38,486 | 15,526 | - In `facebook/react`, thrown promises/wakeables are handle… |
| Q3 | 6 | 1,727 | 10,785 | 7,237 | 26,236 | 18,999 | - Yes. `vercel/next.js` uses React `startTransition` in cli… |
| Q4 | 12 | 3,315 | 20,102 | 15,940 | 47,671 | 31,731 | - `vercel/next.js` Pages Router: `packages/next/src/server/… |
| Q5 | 3 | 646 | 3,646 | 2,834 | 17,466 | 14,632 | - `facebook/react` `packages/`: `dom-event-testing-library`… |
| Q6 | 2 | 499 | 46,715 | 6,393 | 22,657 | 16,264 | - Problem solved: PR `#27733` tries to add reload-and-profi… |
| Q7 | 8 | 2,376 | 14,386 | 11,119 | 37,390 | 26,271 | - Next.js App Router path: `packages/next/src/server/app-re… |
| Q8 | 3 | 707 | 5,422 | 4,596 | 15,715 | 11,119 | - `TanStack/query`: found. React usage is in `packages/reac… |
| Q9 | 13 | 3,686 | 27,898 | 18,256 | 63,705 | 45,449 | - Vue entry point: `vuejs/core` uses `baseCreateRenderer(..… |
| Q10 | 10 | 2,714 | 20,118 | 13,639 | 52,375 | 38,736 | - Vue state/reactivity primitives live in `vuejs/core` unde… |
| Q11 | 10 | 2,856 | 28,638 | 11,999 | 48,886 | 36,887 | - `$state` runtime values in Svelte 5 are represented by si… |
| Q12 | 5 | 1,372 | 13,402 | 4,647 | 27,279 | 22,632 | - Vue 3 processes template `v-if`/`v-else-if`/`v-else` in `… |
| Q13 | 23 | 6,891 | 40,302 | 28,836 | 114,811 | 85,975 | - The `Postpone` component is in `vercel/next.js` at `packa… |
| Q14 | 3 | 835 | 11,835 | 3,227 | 18,914 | 15,687 | - `createSignal` is implemented in `solidjs/solid` at `pack… |
| Q15 | 9 | 2,602 | 28,102 | 10,844 | 52,836 | 41,992 | - Svelte 5 reactivity lives in both compiler and runtime. T… |
| Q16 | 8 | 2,086 | 16,683 | 10,217 | 47,562 | 37,345 | - In Zod v4 core, the core parse implementation is `package… |
| Q17 | 1 | 208 | 1,752 | 1,549 | 23,079 | 21,530 | - `compose()` is implemented in `honojs/hono` at `src/compo… |
| Q18 | 3 | 755 | 15,475 | 3,762 | 35,253 | 31,491 | - `runTest(test, runner)` is in `vitest-dev/vitest` at `pac… |
| Q19 | 12 | 3,567 | 32,326 | 17,160 | 71,525 | 54,365 | - Astro SSR entry: `withastro/astro` has `renderPage(...)` … |
| Q20 | 2 | 489 | 43,929 | 10,258 | 34,505 | 24,247 | - Problem solved: PR `trpc/trpc#7336` fixes React 19 coerci… |
| **Σ** | **141** | **39,521** | **402,386** | **210,513** | **814,198** | **603,685** | |
