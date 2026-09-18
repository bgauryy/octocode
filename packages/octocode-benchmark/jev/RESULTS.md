# Terra research with and without Jev — measured results

Run: `terra-20260918-v1`, 18 September 2026. Canonical location: `/Users/bgaryy/code/octocode/packages/octocode-benchmark/jev`.

## Verdict

**CONTINUE — report-only pilot; no demonstrated causal Jev benefit.** All 22 Terra answers are complete and the treatment made **11 successful remote Jev calls**. On the ten GitHub questions, baseline scored 10.0/10 and treatment 9.8/10. On the React bug, runtime-informed adjudication scored baseline 7/10 and treatment 9/10. Combined quality is therefore **9.73/10 in both arms**. This is a single paired run, not evidence of equivalence.

The safer treatment bug proposal is encouraging, but its pre-Jev plan already contained the relevant guard. No recorded direction change can confidently be attributed to Jev. Keep the optional crossroads policy; these results do not justify calling Jev at every research step.

## Goal

Measure whether Jev improves Terra's evidence-backed research decisions and final answers using Octocode, including verification and repair of a reported React bug.

## KPI

- Primary metric: independently graded final-answer quality, higher is better; baseline **9.73**, result **9.73**, delta **0.00** after runtime adjudication. A positive paired delta would justify replication, not production acceptance.
- Pass guardrail: score at least 8 with no major false claim. Baseline **10/11**, treatment **11/11** after adjudication.
- Major false claims: baseline **1**, treatment **0** identified by the grading/runtime audit. This is not an exhaustive claim-count measurement.
- Attribution: **0/11 demonstrated Jev-caused direction changes**; ten corroboration/refinement cases and one forced probe without a decision opportunity.
- Efficiency: **25 → 28 Octocode research invocations**, plus **11 Jev calls**. No demonstrated speed or total-token saving.

## Loop level and budget

Experiment-level, one frozen paired trial: ten unchanged canonical GitHub questions plus React issue #37655. Two fresh `gpt-5.6-terra` agents at high effort, one per arm; each answered all eleven cases. A third fresh Terra agent prepared source-backed gold before seeing answers and then graded both arms.

Maximum twelve Octocode research invocations per case; at most five query rows per invocation. Schema inspections are logged separately. Baseline used no Jev. Treatment made exactly one call per case to `jev-1.13.0`, with retries disabled and a ten-second timeout. This deliberately forced-call design includes simple lookup controls and is not the normal optional routing policy.

## Subject and harness

`CONTRACT.md`, `questions.json`, and `run-case.mjs` were frozen before worker research. `frozen.json` records their hashes. Gold was independently sealed with SHA-256 `74a8ef357b7d1a686f3bea8631e7bf26ae938d59430ff9a8d78e590b94ef6db2` and remains unchanged.

The wrapper retained requests, responses, timestamps, hashes, provider token usage, and failures. The curator used correctness /4, coverage /2, citations /2, and calibration /2. Runtime evidence later falsified a high-scoring patch: original grades remain intact, with a separate `curator/adjudicated-grades.json` correction. Neither arm received the parent's runtime findings while researching.

The benchmark protocol moved out of the standalone skill into this directory. Historical results remain available separately; this run does not overwrite them. The user-requested package directory owns these artifacts instead of the eval skill's usual workspace artifact directory.

## Answer quality

All original answers, including incorrect or unverified proposals, are preserved in `ANSWERS.md`. Scores below are model judgments, except where runtime evidence directly falsifies a claim.

| Case | Research subject | Without Jev | With Jev | Finding |
|---|---|---:|---:|---|
| Q1 | Next.js route regex return | 10 | 10 | Direct lookup; no decision opportunity |
| Q2 | `is` nonexistent export | 10 | 10 | Bounded absence; extra checks already planned |
| Q3 | Flask route ownership/history | 10 | 10 | Source and historical diff agree |
| Q4 | Axios redirect dependency chain | 10 | 10 | Cross-repository trace |
| Q5 | Vue hydration PR | 10 | 10 | Relevant patch/test interpretation |
| Q6 | Express/router ownership | 10 | 8 | Treatment's Express-side delegation/absence evidence weaker |
| Q7 | Next.js/Zustand Context factory | 10 | 10 | Provider evidence refines existing conclusion |
| Q8 | VS Code keybinding dispatch | 10 | 10 | Class/base-method trace |
| Q9 | Fastify hook lifecycle | 10 | 10 | Ordering and invocation chain |
| Q10 | Axios language and entry points | 10 | 10 | Package exports versus source entry distinguished |
| BUG | React #37655 diagnosis/repair | 7 | 9 | Runtime-corrected; see below |
| **Mean, ten questions** | | **10.00** | **9.80** | Both pass 10/10 |
| **Mean, all eleven** | | **9.73** | **9.73** | Baseline passes 10/11; treatment 11/11 |

Initially the curator scored baseline BUG 10 and treatment BUG 9. The baseline's strong source trace concealed a defective concrete edit. Runtime correction changed baseline BUG to 7, not the gold or original score. Treatment remains 9: cautious prose is not a delivered, verified implementation, and its answer omitted the genuine-mismatch regression test.

## Did Jev help decision making?

Jev returned valid typed judgments in all eleven cases. This verifies real API execution, not the correctness of the host's final reasoning.

- Q2 claimed a decision change, but its before-plan already named the same manifest and export-source inspections subsequently performed.
- Q7 claimed a change, but the factory conclusion was unchanged; a provider inspection improved support for a known uncertainty.
- BUG retained the same provisional diagnosis and guarded repair direction before and after Jev.
- The other cases show corroboration, not a new or corrected direction. No harmful direction change was identified.

The evidence supports **a functioning additional judgment layer**, not a demonstrated improvement caused by that layer. The host supplies Jev's candidate choices; several decks contrasted an already-supported choice with a plainly weak alternative. Some packets relied on host summaries instead of sufficient exact excerpts. These make the exercise easier and limit what its valid outputs establish.

## Flow, calls, time and tokens

| Measured quantity | Without Jev | With Jev |
|---|---:|---:|
| Octocode research invocations | 25 | 28 |
| Parseable query rows | 47 | 37 |
| Malformed JSON requests | 0 | 1 |
| Tool-error rows | 1 | 1 |
| Schema invocations, separately counted | 6 | 1 |
| Successful Jev calls | 0 | 11 |
| Jev input/output tokens | 0 / 0 | 8,461 / 903 |
| Summed Octocode call duration | 33.552 s | 24.136 s |
| Summed Jev call duration | 0 | 10.514 s |
| First logged event → last answer-file write | 278.708 s | 619.948 s |
| Host input/output tokens | Unavailable | Unavailable |

The treatment added three research invocations (+12%). Fewer query rows do not prove less work: batching differs and the malformed request contributes no parseable rows. Jev averaged approximately 0.956 seconds per call. Summed tool durations exclude reasoning and can overlap; they are not elapsed investigation time. The event-to-answer spans include writing but exclude setup and post-answer completion; they are not full host end-to-end time. Some agent-entered timestamps were estimates, so this report does not use them for timing conclusions. Total tokens and total cost cannot be compared without host telemetry.

## React bug: actual reproduction and solution candidate

The [reported React issue](https://github.com/react/react/issues/37655) describes repeated `use(thenable)` suspension with non-stateful hooks between suspensions. Octocode source reads traced the [DEV hook recorder](https://github.com/react/react/blob/71f725593739d2cb5866a282a1075d581831722f/packages/react-reconciler/src/ReactFiberHooks.js#L311), [replay reset](https://github.com/react/react/blob/71f725593739d2cb5866a282a1075d581831722f/packages/react-reconciler/src/ReactFiberHooks.js#L791), and [dispatcher switch](https://github.com/react/react/blob/71f725593739d2cb5866a282a1075d581831722f/packages/react-reconciler/src/ReactFiberHooks.js#L1098). The same mount recorder was confirmed at v19.2.8.

Separately from both evaluated arms, the parent ran actual React DOM **19.2.8** in jsdom, in seven isolated processes. The unpatched build reproduced a false hook-order warning and six recorded context hooks where the example executes five. A debug-value variant also reproduced the problem.

| Runtime matrix | Original React | Baseline's literal patch | Parent implementation of treatment prose | Parent cursor-aware candidate |
|---|---|---|---|---|
| Two thenables with context between | False warning | False warning | No false warning | No false warning |
| Debug-value between suspensions | False warning | False warning | No false warning | No false warning |
| Single, same-promise, state-between, adjacent controls | All four clean | All four regress | All four clean | All four clean |
| Genuine hook-order violation | Warns | Warns | Warns | Warns |

The baseline's literal edit truncates at a cursor that mount recording does not advance and lacks the guard needed to distinguish the relevant replay transition. Its translated patch fails all six non-violation checks. The treatment proposed truncating only once before the first newly mounted hook after replay, but supplied no executable guard. A **parent-authored** once-per-replay flag/reset implements that interpretation and passes all seven checks. It must not be credited as Terra-produced code, Terra runtime verification, or a causal Jev gain.

The independent candidate in `repro/candidate-source.patch` instead advances the DEV mount cursor, validates already-recorded slots, and appends only beyond the recorded prefix. It passes the same seven checks. Both are experimental local candidates: full reconciler, Strict Mode, hydration, render-phase update, aborted replay, and DevTools coverage remain untested. No upstream fix or PR was submitted. See `repro/README.md` for commands and raw JSON results.

This verifies a supplied public bug; it does **not** measure blind bug detection. Both arms knew the issue's proposed mechanism.

## Checks run

- `node summarize.mjs`: all 22 case artifact sets complete; all 11 treatment calls present; frozen harness hashes and logged receipts valid; invocation caps satisfied. These are mechanical checks, not certification of all semantic protocol requirements.
- `node compile-answers.mjs`: all 22 answers compiled without rewriting their claims.
- Canonical Q1–Q10 comparison: prompts unchanged. Sealed gold hash unchanged.
- `node reproduce.mjs`: original behavior and controls reproduced, exit 0.
- `node build-candidate.mjs` and candidate reproduction: exit 0; seven checks pass.
- Baseline-proposal reproduction: expected exit 1; retained results show six non-violation failures.
- Treatment-interpretation reproduction: exit 0; seven checks pass.
- Standalone Jev skill review after benchmark relocation: zero errors and zero warnings.
- Report structure validator and scoped whitespace checks: pass.

## Limitations and next experiment

One run per arm is insufficient for causal attribution or significance. Each arm was fresh at suite entry, not fresh per question; baseline overlapped research across cases, despite the intended in-order workflow. Public questions can be familiar to the model. Source branches, caches, scheduling, and batching differ. Isolation was instruction-based, not a process sandbox. Gold preparation was independent, but grading was not fully blinded, and its initial bug-patch error demonstrates why runtime checks matter. Concurrent repository edits also prevent claiming a fully immutable environment; the three frozen harness files and sealed gold remained unchanged.

Next: fresh independent Terra runs per case, repeated and counterbalanced on pinned source snapshots; capture real host tokens and complete elapsed time. Include genuinely competing hypotheses, exact evidence packets, clean bug-detection controls and hidden executable repair tests. Compare optional Jev routing with no Jev and forced Jev separately. Require a verified useful change in the next action—not an `adviceApplied` flag—before calling it a decision-making gain.

The eval skill drove the frozen comparison, separate runtime adjudication, and report-only verdict. The skills and prompt-optimizer checks kept benchmark instructions separate from runtime skill guidance and made the before/after decision boundary explicit.
