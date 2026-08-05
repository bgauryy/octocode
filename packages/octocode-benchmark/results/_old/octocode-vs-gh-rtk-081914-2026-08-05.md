# Octocode CLI vs `gh` + `rtk` — 2026-08-05 08:19:14 UTC

**Outcome: RTK wins on correctness.** The RTK arm scored 10.000 mean correctness versus 9.647 for Octocode. Octocode was leaner—485,117 total campaign characters versus 542,592 (10.6% fewer, including its 14,308-character catalog/setup read)—but correctness takes precedence. Octocode's Q14 answer falsely classified Vite as a regular dependency, and its Q11 answer omitted the synchronous `execFileSync` process path.

## Run contract

- Questions: all 17 shared GitHub questions.
- Budget: at most 8 research calls per question.
- Arm A: read-only `gh` through `rtk gh`; `gh 2.76.2`, `rtk 0.41.0`.
- Arm B: `npx octocode tools …` only; Octocode `18.0.1`.
- Roles: two isolated runners and one blind independent grader.
- Mutable refs frozen at campaign start (`2026-08-05T08:19:14Z`) and verified between `08:22:13Z` and `08:22:51Z`.
- Character unit: complete raw CLI output shown to each runner, not tokens.

Representative frozen refs: Next.js `ab09c1f4b45d2ee316353ff4352d7efbbef396b2`; Flask `6a2f545bfd8ed31e19066a299296917e034aca58` and historical `705e52684a9063889c16a289695a2e4429df6887`; Express `a3714473feb3d2908add734d340e7755fd85e0a3`; Router v2.2.0 `e6d6b609fc355e558174ccd5b1db646f739fe88c`; Vitest `96e40feeae35a35e185ba2dd718253459bda2b30`; Next.js fetch memoization `ab09c1f4b45d2ee316353ff4352d7efbbef396b2`.

## Per-question table

| Question | Correctness A | Correctness B | Depth A | Depth B | Workflow A | Workflow B | Chars A | Chars B | Leaner / preferred |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---|
| Q1 | 10 | 10 | 5 | 5 | 4 | 5 | 15,301 | 11,121 | B |
| Q2 | 10 | 10 | 5 | 5 | 5 | 4 | 78,334 | 248,887 | A |
| Q3 | 10 | 10 | 5 | 5 | 4 | 4 | 48,417 | 15,926 | B |
| Q4 | 10 | 10 | 5 | 5 | 5 | 5 | 8,782 | 12,880 | A |
| Q5 | 10 | 10 | 5 | 5 | 5 | 5 | 18,835 | 56,440 | A |
| Q6 | 10 | 10 | 5 | 5 | 5 | 3 | 23,530 | 16,027 | B |
| Q7 | 10 | 10 | 5 | 5 | 5 | 5 | 7,461 | 5,570 | B |
| Q8 | 10 | 10 | 5 | 5 | 2 | 5 | 58,625 | 7,671 | B |
| Q9 | 10 | 10 | 5 | 5 | 4 | 5 | 86,398 | 9,118 | B |
| Q10 | 10 | 10 | 5 | 4 | 4 | 4 | 14,011 | 8,585 | B |
| Q11 | 10 | 8 | 5 | 3 | 5 | 4 | 23,084 | 7,008 | **A (correctness)** |
| Q12 | 10 | 10 | 5 | 5 | 4 | 5 | 44,747 | 6,330 | B |
| Q13 | 10 | 10 | 5 | 5 | 5 | 5 | 10,249 | 14,940 | A |
| Q14 | 10 | 6 | 5 | 3 | 5 | 5 | 5,575 | 5,535 | **A (correctness)** |
| Q15 | 10 | 10 | 5 | 5 | 5 | 5 | 22,780 | 27,751 | A |
| Q16 | 10 | 10 | 5 | 5 | 4 | 4 | 21,329 | 5,343 | B |
| Q17 | 10 | 10 | 5 | 5 | 5 | 5 | 55,134 | 11,677 | B |

## Summary

| Metric | A (`gh` + RTK) | B (Octocode) |
|---|---:|---:|
| Correctness (mean) | **10.000** | 9.647 |
| Research depth (mean) | **5.000** | 4.706 |
| Workflow (mean) | 4.471 | **4.588** |
| Question calls | **60** | 62 |
| Question chars | 542,592 | **470,809** |
| Setup/catalog chars | 0 | 14,308 |
| **Campaign chars (total)** | 542,592 | **485,117** |
| Summed elapsed | **87.958s** | 207.8s |
| Per-question preferences | 7 | **10** |

Octocode used 71,783 fewer question-level characters (13.2%) and 57,475 fewer campaign characters after setup (10.6%). That efficiency does not overturn the correctness-first result.

## Per-question detail

- **Q1:** Both found `getRouteRegex`/`getNamedRouteRegex` in `route-regex.ts` and the `getParametrizedRoute` / `getNamedParametrizedRoute` / `getSafeKeyFromSegment` chain. Octocode was shorter.
- **Q2:** Both correctly discovered `sindresorhus/is`, TypeScript/main, and bounded the absent `isQuantumSuperposition` export with package/source/search evidence. Octocode repeated large reads; RTK was much leaner.
- **Q3:** Both correctly located `Scaffold.route` and established from the historical diff that `705e5268` added `_method_route` plus `get/post/put/delete/patch` decorators and rejected explicit `methods`. Octocode was shorter despite more calls.
- **Q4:** Both correctly reported PR #3531 open and the V8 stack-regex fix for source paths containing spaces. RTK was leaner.
- **Q5:** Both supported multiple Suspense/Vapor/VDOM hydration cases and the runtime-core/runtime-vapor ownership split. RTK was leaner.
- **Q6:** Both traced Express's `router: ^2.2.0` dependency to `pillarjs/router`, `Router.prototype.handle`/`next`, and `matchLayer`. Octocode used the full eight-call budget; it was shorter but had weaker workflow.
- **Q7:** Both found a Context-backed per-provider store factory and Zustand's optional React peer metadata. Octocode was shorter.
- **Q8:** Both found `WorkbenchKeybindingService`, `AbstractKeybindingService`, and `dispatchEvent`. RTK incurred two code-search rate-limit failures; exact frozen file reads still supported its answer. Octocode was much shorter.
- **Q9:** Both reproduced the documented lifecycle and `context.onRequest` → `onRequestHookRunner` wiring. RTK repeated a documentation read; Octocode was much shorter.
- **Q10:** Both traced Axios CommonJS resolution to `dist/node/axios.cjs` and the source entry under `lib/`. RTK supplied the clearer Rollup build bridge; Octocode was shorter.
- **Q11:** Both identified a separate native service process. Only RTK named both async `child_process.spawn` and synchronous `child_process.execFileSync`; Octocode omitted the requested synchronous boundary detail.
- **Q12:** Both accurately traced `Stream` inheritance and `EventEmitter.prototype.once` through `_onceWrap`, `on`/`addListener`, `_addListener`, removal, and `ReflectApply`. Octocode was much shorter.
- **Q13:** Both correctly identified issue #15389, PR #15433, `getBitOffsetFromArgument`, the first overflowing `i64` offset, changed files, and `+18/-3`. RTK was leaner.
- **Q14:** Frozen ground truth is `peerDependencies.vite`, `devDependencies.vite`, and `peerDependenciesMeta.vite.optional=false`; Vite is absent from regular `dependencies`. RTK was correct. Octocode confidently claimed a nonexistent regular dependency.
- **Q15:** Both correctly reported merged PR #5179, array-returning JSX function components, `src/jsx/base.ts`, issue #5177, and closed alternative PR #5178. RTK was leaner.
- **Q16:** Both correctly traced ESLint → Espree → Acorn/Acorn-JSX and cited dependency fields. Octocode was much shorter.
- **Q17:** Both correctly identified `patchFetch`, `createDedupeFetch`, `createPatchedFetcher`, `React.cache`, cache-key inputs, and the explicit-signal/non-GET-or-HEAD/keepalive bypasses. Octocode was much shorter.

## Fairness caveats

- Arm B incurred one 14,308-character schema/catalog read. It is included in campaign totals but not assigned to a question row.
- RTK's Q8 code searches hit GitHub search rate limiting; direct frozen `/contents` reads remained available and were counted.
- Both arms used reruns when needed to recover exact instrumentation markers; all duplicate output was counted.
- Repository metadata exposed GitHub's primary language rather than exact language percentages for Q10/Q11.
- This is one isolated pass per arm, so it is a snapshot rather than a stability claim.

## Bottom line

RTK produced a fully correct 17-question run. Octocode was generally much more character-efficient and won 10 per-question preferences, but its Q14 false claim and Q11 omission make RTK the correctness-first winner for this run.
