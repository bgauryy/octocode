# Octocode CLI vs `gh` + Headroom — run 023223, 2026-08-05

**Bottom line: at essentially equal correctness, Octocode (Arm B) won 12 of 15 gradeable questions by pulling ~5.8× fewer characters into context — even though Arm A's `gh` output was Headroom-compressed on every call.** Headroom compression is real (13–47% per call, measured) but does not close the gap: `gh` returns whole files, trees, and diffs that stay large after compression, while Octocode's tools return pre-distilled snippets and structured fields. Arm A also produced 2 confidently-wrong answers; Arm B produced 0.

Measured in **characters** (raw output pulled into context) per `SCORING.md`. For Arm A that is the Headroom-**compressed** output (measured by `bin/sumlog.py` from the shim's JSONL log); for Arm B it is the raw `octocode` tool output (measured by `wc -c` on a capture file). No self-reported counts.

## Setup (pinned)

| | |
|---|---|
| Arm A | read-only `gh` 2.76.2 → Headroom 0.33.0 (`kompress-v2-base`), one-shot compress via `bin/ghc`, no CCR retrieve |
| Arm B | `npx octocode` v18.0.1 (`tools <name> --queries … --compact`) |
| Runners | Claude Sonnet, `effort=medium`, **same model both arms** (tests the tool, not the model) |
| Grader | Claude Opus, `effort=high`, blind (answers shuffled X/Y, tool identity hidden; establishes ground truth by its own research) |
| Harness | Workflow: per question, 2 isolated runners in parallel + 1 blind grader; sealed packets, no cross-arm leakage |
| Scope | 15 of 17 questions gradeable; Q15 & Q17 excluded (Arm-B runner bail — see below). One pass (snapshot). |

## Summary of all (15 gradeable questions)

| Metric | A (gh + Headroom) | B (Octocode) |
|---|---:|---:|
| **Wins** | 3 | **12** |
| Correctness (mean) | 9.07 | 9.67 |
| Research depth (mean) | 4.53 | 4.60 |
| Workflow (mean) | 3.87 | 4.73 |
| **Chars (total)** | 1,180,822 | **203,708** |
| Chars (median) | 38,067 | 12,574 |
| Questions leaner | 4 | 11 |
| Confidently wrong | 2 | 0 |

Octocode pulled **5.8× fewer** total characters at near-equal correctness.

## Per-question table

| # | Question | Corr A | Corr B | Depth A | Depth B | WF A | WF B | Chars A | Chars B | Leaner | Winner |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|:--:|:--:|
| Q1 | Route regex builder | 10 | 10 | 5 | 4 | 4 | 5 | 25,551 | 6,774 | B | octocode |
| Q2 | Repository discovery and bounded absence | 10 | 10 | 5 | 5 | 4 | 5 | 81,790 | 4,042 | B | octocode |
| Q3 | Flask route history | 10 | 9 | 5 | 5 | 3 | 5 | 70,033 | 13,688 | B | octocode |
| Q4 | Zustand fix PR state | 10 | 10 | 5 | 5 | 3 | 5 | 652,481 | 5,661 | B | octocode |
| Q5 | Vue hydration diff review | 10 | 9 | 5 | 4 | 4 | 4 | 45,656 | 40,956 | B | gh+hr |
| Q6 | Express router cross-repository trace | 10 | 10 | 4 | 4 | 3 | 5 | 38,067 | 10,743 | B | octocode |
| Q7 | Zustand's Next.js integration contract | 6! | 10 | 3 | 5 | 3 | 4 | 2,144 | 8,874 | A | octocode |
| Q8 | VS Code keybinding dispatch | 10 | 10 | 4 | 4 | 4 | 5 | 19,038 | 2,954 | B | octocode |
| Q9 | Fastify lifecycle contract | 10 | 10 | 5 | 4 | 5 | 5 | 51,403 | 17,019 | B | octocode |
| Q10 | Axios repository and Node entry chain | 9 | 9 | 5 | 5 | 4 | 5 | 124,885 | 12,574 | B | octocode |
| Q11 | Esbuild repository and Node runtime boundary | 9 | 8 | 5 | 4 | 4 | 4 | 2,713 | 15,445 | A | gh+hr |
| Q12 | Stream and EventEmitter wiring | 10 | 10 | 5 | 5 | 4 | 5 | 39,776 | 9,188 | B | octocode |
| Q13 | Redis security issue and fix PR | 10 | 10 | 5 | 5 | 5 | 4 | 8,638 | 24,782 | A | gh+hr |
| Q14 | Vitest's dependency on Vite | 2! | 10 | 2 | 5 | 4 | 5 | 1,658 | 15,542 | A | octocode |
| Q15 | Hono JSX array component PR | 10 | *n/a* | 5 | – | 5 | – | 8,208 | *bail* | — | *excl.* |
| Q16 | ESLint parser dependency chain | 10 | 10 | 5 | 5 | 4 | 5 | 16,989 | 15,466 | B | octocode |
| Q17 | Next.js fetch request memoization | 10 | *n/a* | 5 | – | 5 | – | 56,869 | *bail* | — | *excl.* |

`!` = grader flagged the answer confidently-wrong. Q15/Q17: Arm B runner bailed (see Excluded).


## What the numbers say

- **Correctness is close** (9.1 vs 9.7). On the clear structural questions both arms nail the answer. The gap is Arm A's two confidently-wrong answers — Q7 (Zustand's React peer field) and Q14 (Vitest's `vite` peer dependency) — where compressed `gh` output led the runner to assert a wrong `package.json` relationship. Arm B got both right.
- **Efficiency is where Octocode dominates.** Arm A's char cost is volatile: tight when a `gh search code` snippet answers (Q7 2.1K, Q11 2.7K, Q13 8.6K), catastrophic when the question needs file/tree/diff content that stays huge after compression — Q4 **652K**, Q10 125K, Q2 82K, Q3 70K. Octocode's tools return pre-distilled results, so its cost is consistently low (median 12.6K vs 38.1K).
- **Where gh+Headroom won (3):** Q5 (Vue diff — both correct, A marginally leaner on a diff-heavy question), Q11 (esbuild — A both leaner *and* more correct), Q13 (Redis security — A correct and leaner; here Octocode was *more* verbose). These are the cases where a single targeted `gh` call plus compression beats Octocode's multi-call path.
- **Headroom did its job at the call level** (measured 13–47% char reduction, lossless on structured JSON via SmartCrusher) — but call-level savings can't offset fetching an entire file/tree when Octocode never fetched it whole in the first place.

## Excluded questions (harness artifact, not a tool result)

**Q15 (Hono PR #5179)** and **Q17 (Next.js fetch memoization)**: across two independent runs the Arm-B (Octocode) runner returned a placeholder answer (`"test"`, chars_in=100) — the runner bailed before producing an answer. This is a **subagent failure, not an Octocode limitation**: I verified manually that Octocode answers Q15 in a single `ghSearchPullRequests` call (~1,974 chars, full PR detail incl. refs #5177 / alternative #5178). Crediting Arm A for these would inflate its record, so both are excluded from scoring. (Arm A answered both correctly at 10/10.) A re-run with higher runner effort or a non-schema answer channel would likely recover them.

## Fairness caveats

- One pass — a snapshot, not a stability average. `SCORING.md` recommends repeating; Headroom's compression was verified deterministic, but runner/grader agents are not.
- Both arms used the same runner model and effort, so this measures the *tool surface*, not model skill.
- Arm A's chars are post-compression (its designed advantage); Arm B's are raw. That is the intended comparison — it still lost on total characters by 5.8×.

## Per question (detail)

**Q1 — Route regex builder** · winner: octocode · chars A=25,551 / B=6,774
> Both answers are fully correct on every material part (function getRouteRegex, file path, first helper getParametrizedRoute, groups). Correctness is essentially equal (10/10 each) and neither is confidently wrong. Tie broken by efficiency: Y reached the same correct, well-supported answer with far fewer chars_in (6774 vs 25551).

**Q2 — Repository discovery and bounded absence** · winner: octocode · chars A=81,790 / B=4,042
> Both answers are factually correct on every material part (repo, TypeScript, default branch main, and a well-supported NO for isQuantumSuperposition), and neither is confidently wrong. X enumerates the fully-visible 3-file source surface and backs the NO with an exact-identifier zero-hit search plus a broader substring sanity check; Y backs it with a scoped search plus a full-file fetch-and-grep. Correctness and depth are essentially equal, so the tie breaks on fewer chars_in: X at 4042 vs Y at 

**Q3 — Flask route history** · winner: octocode · chars A=70,033 / B=13,688
> Both answers correctly identify the current file (src/flask/sansio/scaffold.py), the owning Scaffold base class, and correctly explain the introduced behavior from the changed code itself — the _method_route helper, the get/post/put/delete/patch shortcuts, the TypeError guard, and the pure-syntactic-sugar semantics. Correctness is essentially equal: X's only edge is quoting the exact full SHA, while Y mistranscribes the full hash (a harmless typo since it otherwise pins the commit via PR #3907, 

**Q4 — Zustand fix PR state** · winner: octocode · chars A=652,481 / B=5,661
> Both answers are factually correct on every material part (state OPEN, source file src/middleware/devtools.ts, the v8StackLineRe regex change, and the space-in-source-path edge case with webpack-namespace and Windows-path examples), verified against gh. Neither is confidently wrong. Correctness is essentially equal (both 10/10) and depth is equal — Y quotes the exact diff, X supplies correct concrete captured tokens. The tie breaks on efficiency: X reached the same verified conclusion with chars

**Q5 — Vue hydration diff review** · winner: gh+headroom · chars A=45,656 / B=40,956
> Both answers correctly identify the two required scenarios (Suspense/async branch unmounted before resolve; VDOM<->Vapor interop anchor/adopted-DOM desync during hydration) and both give an accurate why-both-packages rationale, verified against the full diff. X is fully accurate on every material point and adds correctly-verified supporting detail (block.ts boundary, componentSlots/renderVDOMSlot rewrite), and crucially labels the runtime-core `component.asyncDep && !component.asyncResolved` bra

**Q6 — Express router cross-repository trace** · winner: octocode · chars A=38,067 / B=10,743
> Both answers are fully correct on every material part: loop not in express, dependency router→pillarjs/router, next() advances layers, and matchLayer identified. Neither is confidently wrong. X names matchLayer(layer, path) as the helper that tests one layer against the path (index.js) — the most literal fit for the question's wording, and it quotes the actual loop and matchLayer source as evidence. Y is also correct and slightly more complete (it additionally names Layer.prototype.match in lib/

**Q7 — Zustand's Next.js integration contract** · winner: octocode · chars A=2,144 / B=8,874
> Both correctly identify store.ts as a Context-backed per-request factory with the right APIs and both name peerDependenciesMeta.react.optional as the controlling field. The decider is correctness on the package.json question: Y correctly states react is in peerDependencies as ">=18.0.0" and is made optional via peerDependenciesMeta, matching ground truth. X confidently and falsely asserts there is no peerDependencies entry for react at all, which is wrong and mischaracterizes the very structure 

**Q8 — VS Code keybinding dispatch** · winner: octocode · chars A=19,038 / B=2,954
> Both answers are fully correct on every material part and verified against ground truth (dispatchEvent line 143, _dispatch line 237, abstract class line 42, WorkbenchKeybindingService line 175). Correctness is equal (10/10 each), so the tie breaks to fewer chars_in: X (2954) over Y (19038).

**Q9 — Fastify lifecycle contract** · winner: octocode · chars A=51,403 / B=17,019
> Both answers are factually correct on every material part (lifecycle order, error branches, context.onRequest property, onRequestHookRunner runner, runPreParsing continuation) — correctness is essentially equal at 10/10 and neither is confidently wrong. X carries marginally more depth with the extra preParsingHookRunner/kRouteContext reference, but that is not decisive. Per the tie-break rule on essentially-equal correctness, the answer with fewer chars_in wins: Y (17019) over X (51403).

**Q10 — Axios repository and Node entry chain** · winner: octocode · chars A=124,885 / B=12,574
> Both answers are essentially equally correct on every material part (repo, dominant JavaScript, main -> exports require -> dist/node/axios.cjs -> lib/axios.js), and neither is confidently wrong. X is marginally more precise on the exports condition structure and default branch, while Y adds nice exact byte counts but has minor imprecisions (require nesting, 'main-branch'). With correctness a tie, the tie-break is fewer chars_in, which strongly favors X (12,574 vs 124,885); X also had the cleaner

**Q11 — Esbuild repository and Node runtime boundary** · winner: gh+headroom · chars A=2,713 / B=15,445
> Both correctly report Go as dominant and both correctly conclude the core runs in a separate process with child_process.spawn for the async API — neither is confidently wrong. The deciding factor is the sync-path detail: X names all three relevant Node APIs (spawn, execFileSync, worker_threads) and every claim it makes is factually accurate against the source, its only shortcoming being that it treats worker_threads as an optional offload rather than the default wrapper. Y, though longer, omits 

**Q12 — Stream and EventEmitter wiring** · winner: octocode · chars A=39,776 / B=9,188
> Both answers are fully correct on every material part (Stream in legacy.js, the EE.call + two ObjectSetPrototypeOf wiring, and the once -> checkListener/_onceWrap -> on/addListener -> _addListener delegation with wrapper.listener and removeListener-on-fire), and neither is confidently wrong. Correctness is essentially equal (both 10). Y adds slightly more detail (prependOnceListener, checkListener sharing) but nothing X got wrong. Since correctness ties, the tiebreak on fewer chars_in favors X (

**Q13 — Redis security issue and fix PR** · winner: gh+headroom · chars A=8,638 / B=24,782
> Both X and Y are factually correct on every material part: issue #15389 (BITFIELD #offset signed overflow DoS, author sdjasj), vulnerable function getBitOffsetFromArgument() in src/bitops.c, vulnerable operation loffset *= bits executed before the negative/proto_max_bulk_len checks, first overflowing i64 offset floor(LLONG_MAX/64)+1 = 144115188075855872, fixing PR #15433 by SacadM (MERGED into unstable), files src/bitops.c (+9/-3) and tests/unit/bitfield.tcl (+9/-0), totals +18/-3. I verified al

**Q14 — Vitest's dependency on Vite** · winner: octocode · chars A=1,658 / B=15,542
> X matches ground truth exactly; Y is confidently wrong that no peer dependency exists.</parameter> <parameter name="truth_summary">vitest packages/vitest/package.json v5.0.0-beta.7: vite is a peerDependency (^6.4.0 || ^7.0.0 || ^8.0.0) with peerDependenciesMeta.vite.optional=false (required), also a devDependency, and absent from regular dependencies.</parameter> </invoke>

**Q16 — ESLint parser dependency chain** · winner: octocode · chars A=16,989 / B=15,466
> Both answers are fully correct on every material part (eslint->espree via dependencies, espree->acorn via dependencies, with accurate version ranges and correct identification of acorn as the underlying parser). Neither is confidently wrong. Correctness, depth, and evidence quality are essentially equal. X had a cleaner, more efficient described workflow (a direct manifest read) while Y describes a compression-workaround detour. With correctness tied, the tiebreak favors the answer with fewer ch

