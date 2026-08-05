# Octocode CLI vs `gh` + Headroom — run 073749, 2026-08-05

**Bottom line: at near-equal correctness, Octocode (Arm B) answered all 17 questions pulling 5.6× fewer characters into context than Headroom-compressed `gh` (Arm A), and made zero confidently-wrong answers to Arm A's two.** Under a *blind three-role* protocol (isolated Runner A, isolated Runner B, blind Grader), Octocode won 11 questions, gh+Headroom won 6. Arm A's six wins were all cases where its full-file / full-diff reads surfaced a nuance the leaner Octocode path under-weighted — and Arm A paid for them: on the questions Arm A *lost* it repeatedly spent 50K–330K characters where Octocode spent <15K. Headroom compression is real on structured JSON (SmartCrusher, 7–66% per call, measured) but **does not compress source/file content in 0.33.0** (routes to `protected:analysis_context` / `code_aware:1.00`, ~0%), which is exactly where `gh`'s cost explodes.

Measured in **characters** (raw output pulled into context) per `SCORING.md`. Arm A = Headroom-**compressed** output measured by the shim's JSONL log (`bin/sumlog.py` / a quote-tolerant `out_chars` summer). Arm B = raw `octocode` tool output measured by a logging wrapper (`tmp/bin/octo` → char count). No self-reported counts — the runners never scored their own context.

## Setup (pinned)

| | |
|---|---|
| Arm A | read-only `gh` 2.76.2 → Headroom 0.33.0 (`kompress-v2-base`), one-shot compress via `bin/ghc`, no CCR retrieve |
| Arm B | `octocode` v18.0.1 (`tools <name> …`) via wrapper `tmp/bin/octo` (authoritative char log) |
| Runners | claude-opus-4-5, **same model both arms** (measures the tool surface, not the model) |
| Grader | claude-opus-4-5, blind — answers shuffled X/Y (random coin per question), tool identity hidden; establishes ground truth by its own `gh`/web research |
| Harness | `pi -p --no-session` spawns each of the 3 roles as a fresh isolated context; sealed packets, no cross-arm/grader leakage; 480s watchdog per role |
| Scope | **17 of 17 gradeable** (Q15 & Q17 recovered vs the prior run's Arm-B bail). One pass (snapshot). |

## Summary of all (17 questions)

| Metric | A (gh + Headroom) | B (Octocode) |
|---|---:|---:|
| **Wins** | 6 | **11** |
| Correctness (mean) | 8.88 | **9.41** |
| Research depth (mean) | 4.29 | 4.47 |
| Workflow (mean) | 4.53 | 4.65 |
| **Chars (total)** | 1,217,394 | **215,660** |
| Chars (median) | 55,510 | **9,771** |
| Questions leaner | 4 | **13** |
| Confidently wrong | **2** | 0 |

Octocode pulled **5.64× fewer** total characters at higher mean correctness.

## Per-question table

| # | Question | Corr A | Corr B | Dep A | Dep B | WF A | WF B | Chars A | Chars B | Leaner | Winner |
|---|---|--:|--:|--:|--:|--:|--:|--:|--:|:--:|:--:|
| Q1 | Route regex builder | 9 | 6 | 4 | 4 | 4 | 4 | 55,510 | 10,360 | B | gh+hr |
| Q2 | Repository discovery & bounded absence | 9 | 10 | 5 | 5 | 5 | 5 | 153,796 | 13,147 | B | octocode |
| Q3 | Flask route history | 10 | 10 | 5 | 5 | 5 | 5 | 68,231 | 7,472 | B | octocode |
| Q4 | Zustand fix PR state | 10 | 8 | 5 | 4 | 5 | 4 | 329,220 | 4,559 | B | gh+hr |
| Q5 | Vue hydration diff review | 9 | 9 | 4 | 5 | 4 | 5 | 90,016 | 59,355 | B | octocode |
| Q6 | Express router cross-repo trace | 10 | 9 | 5 | 4 | 5 | 4 | 117,520 | 9,771 | B | gh+hr |
| Q7 | Zustand's Next.js integration contract | 6! | 10 | 4 | 4 | 5 | 5 | 2,717 | 8,865 | A | octocode |
| Q8 | VS Code keybinding dispatch | 10 | 10 | 4 | 5 | 4 | 5 | 76,606 | 8,757 | B | octocode |
| Q9 | Fastify lifecycle contract | 10 | 10 | 4 | 4 | 5 | 5 | 51,088 | 8,659 | B | octocode |
| Q10 | Axios repo & Node entry chain | 8 | 9 | 4 | 4 | 4 | 4 | 26,175 | 12,733 | B | octocode |
| Q11 | Esbuild repo & Node runtime boundary | 8 | 9 | 4 | 5 | 4 | 5 | 20,145 | 5,958 | B | octocode |
| Q12 | Stream & EventEmitter wiring | 10 | 10 | 4 | 5 | 4 | 5 | 75,855 | 11,422 | B | octocode |
| Q13 | Redis security issue & fix PR | 10 | 10 | 4 | 5 | 5 | 5 | 11,733 | 16,050 | A | gh+hr |
| Q14 | Vitest's dependency on Vite | 2! | 10 | 4 | 4 | 4 | 4 | 16,863 | 6,988 | B | octocode |
| Q15 | Hono JSX array component PR | 10 | 10 | 5 | 4 | 5 | 4 | 5,698 | 6,989 | A | gh+hr |
| Q16 | ESLint parser dependency chain | 10 | 10 | 4 | 4 | 4 | 5 | 8,806 | 14,750 | A | gh+hr |
| Q17 | Next.js fetch request memoization | 10 | 10 | 4 | 5 | 5 | 5 | 107,415 | 9,825 | B | octocode |

`!` = grader flagged the answer confidently-wrong. Winner by `SCORING.md`: correctness-first; a confidently-wrong answer cannot win; equal correctness broken by **fewer chars** (leaner). Two rows (Q13, Q16) are correctness-ties where the blind grader *preferred* Octocode on depth but `gh+Headroom` was leaner — `SCORING.md`'s char tiebreaker awards them to Arm A.

## What the numbers say

- **Correctness is close but Octocode is ahead** (9.41 vs 8.88). The gap is Arm A's two confidently-wrong answers — **Q7** (falsely claimed Zustand's `package.json` has no `peerDependencies` block) and **Q14** (falsely claimed Vitest's `main` doesn't declare `vite` as a peer, calling it dev-only). Both are `package.json`-relationship questions where compressed/partial `gh` output misled the runner; Octocode got both right. This reproduces the *same two confidently-wrong questions* the prior run flagged.
- **Efficiency is where Octocode dominates.** Arm A is volatile: cheap when a `gh search code` snippet answers (Q7 2.7K, Q15 5.7K, Q16 8.8K), catastrophic when the question needs file/tree/diff content that stays huge after compression — Q4 **329K**, Q2 154K, Q6 118K, Q17 107K, Q5 90K. Octocode's tools return pre-distilled snippets/fields, so its cost is consistently low (median **9.8K** vs 55.5K).
- **Arm A's six wins earn their keep — at a price.** Q1, Q4, Q6 are cases where reading the *whole* file/diff let Arm A catch a nuance the leaner path missed (Q1 "named" parameter groups → `getNamedRouteRegex`; Q4 the true committed regex `(?:new |async )?`; Q6 exact layer-loop line anchors). Q13/Q15/Q16 are correctness-ties Arm A won on chars. But Q1/Q4/Q6 cost Arm A 55K/329K/118K respectively — the correctness edge came from bulk context, not efficiency.
- **Headroom did its job on structured JSON** (measured 7–66% char reduction, lossless via SmartCrusher on trees / `--json` lists) — but **cannot compress source/file content in 0.33.0**: file fetches route to `router:protected:analysis_context` (0%), and even with `protect_analysis_context=False` they route to `code_aware:1.00` (~0%). So call-level savings can't offset fetching an entire file/tree/diff that Octocode never fetched whole.

## Methodology note — Headroom 0.33.0 compresses JSON, not code

The checked-in shim (`compress_user_messages=True, protect_recent=0`) compresses only the SmartCrusher/JSON path. Raw `gh api …/contents` file bytes and `pr diff` prose are **protected** (`analysis_context`) and pass through at 0%. The matchup README's compression table only ever measured a tree and an issue-list (both JSON), so this case wasn't in it. Consequence for fairness: Arm A's *leanest legitimate path* must prefer snippet-bearing `gh search code` and avoid full-file fetches — this run let each Runner A choose its own path, and the volatility above reflects real runner choices, not a handicap. Verified with `bin/hr_compress.py` transforms output; recorded here so future runs don't expect file-content compression that 0.33.0 does not provide.

## Per question (detail)

Grader is blind; reasons below are the grader's own words (X/Y un-blinded to A/B after tabulation).

- **Q1 — Route regex builder** · winner gh+hr · A=55,510 / B=10,360 — *"'Named parameter groups' points to getNamedRouteRegex→getNamedParametrizedRoute (real named capture groups); A nails it and covers the alternative, B misreads emphasis and picks the positional-group getRouteRegex."* (A corr 9, B corr 6)
- **Q2 — Repository discovery & bounded absence** · winner octocode · A=153,796 / B=13,147 — *"Both correct NO with bounded evidence; B's line count (2038) is accurate while A states 2037."* (A 9, B 10)
- **Q3 — Flask route history** · winner octocode (tie corr, B leaner) · A=68,231 / B=7,472 — *"Both fully correct and evidence-backed; equivalent quality."* (10/10)
- **Q4 — Zustand fix PR state** · winner gh+hr · A=329,220 / B=4,559 — *"Both right on state/file/edge case, but A captured the true committed diff `(?:new |async )?` and flagged a body-vs-diff discrepancy B mislabeled."* (A 10, B 8)
- **Q5 — Vue hydration diff review** · winner octocode · A=90,016 / B=59,355 — *"Both accurate on scenarios and the two-runtime split; B adds verified review-comment/mechanism depth."* (9/9, B preferred)
- **Q6 — Express router cross-repo trace** · winner gh+hr · A=117,520 / B=9,771 — *"Both correct; A's line anchors (loop 235-237, matchLayer 517) all verified exactly with HEAD SHAs, while B mislabels a handle line and hedges more."* (A 10, B 9)
- **Q7 — Zustand's Next.js integration contract** · winner octocode · A=2,717 / B=8,865 — *"Both nail verdict/APIs, but A falsely claims zustand's package.json has no peerDependencies block (it does); B reproduces both blocks correctly."* (**A confidently wrong**, corr 6; B 10)
- **Q8 — VS Code keybinding dispatch** · winner octocode (tie corr, B leaner) · A=76,606 / B=8,757 — *"Both fully correct; B adds verified runtime-path nuance (keydown calls _dispatch directly)."* (10/10, B preferred)
- **Q9 — Fastify lifecycle contract** · winner octocode (tie corr, B leaner) · A=51,088 / B=8,659 — *"Both fully correct and well-evidenced — net equal."* (10/10, tie)
- **Q10 — Axios repo & Node entry chain** · winner octocode · A=26,175 / B=12,733 — *"Both correct; B directly captured the exports map (the crux) verbatim while A only inferred it."* (A 8, B 9)
- **Q11 — Esbuild repo & Node runtime boundary** · winner octocode · A=20,145 / B=5,958 — *"Both correct on repo/Go/separate-process; B accurately names spawn+execFileSync roles, A mischaracterizes execFileSync."* (A 8, B 9)
- **Q12 — Stream & EventEmitter wiring** · winner octocode (tie corr, B leaner) · A=75,855 / B=11,422 — *"Both fully correct and verbatim-accurate; B adds the stream.js re-export + primordials confirmation."* (10/10, B preferred)
- **Q13 — Redis security issue & fix PR** · winner gh+hr (tie corr, A leaner) · A=11,733 / B=16,050 — *"Both fully correct; B adds verified per-file additions/deletions and base branch, slightly deeper."* (10/10; grader preferred B on depth, `SCORING.md` char tiebreaker → A)
- **Q14 — Vitest's dependency on Vite** · winner octocode · A=16,863 / B=6,988 — *"B correctly reads current main (required peer, not a dependency); A is confidently wrong claiming main has no peer and vite is dev-only."* (**A confidently wrong**, corr 2; B 10)
- **Q15 — Hono JSX array component PR** · winner gh+hr · A=5,698 / B=6,989 — *"Both correct; A additionally verified #5178 closed and issue #5177 title rather than leaving them assumed."* (10/10, A leaner + preferred)
- **Q16 — ESLint parser dependency chain** · winner gh+hr (tie corr, A leaner) · A=8,806 / B=14,750 — *"Both correct and well-evidenced; A has cleaner anchors while B hit wrapper-mangling issues it had to work around."* (10/10; grader preferred B, char tiebreaker → A)
- **Q17 — Next.js fetch request memoization** · winner octocode (tie corr, B leaner) · A=107,415 / B=9,825 — *"Both fully correct and verified; B slightly deeper (cloneResponse/PR ref, NEXT_PATCH_SYMBOL, explicit third bypass condition)."* (10/10, B preferred)

## Fairness caveats

- **One pass — a snapshot, not a stability average.** `SCORING.md` recommends ≥3 passes; Headroom's compression was verified deterministic, but the opus-4-5 runner/grader agents are not. Repeat for a stable claim.
- **Same runner model and effort both arms**, so this measures the *tool surface*, not model skill.
- **Arm A's chars are post-compression** (its designed advantage); Arm B's are raw. That is the intended comparison — Arm A still lost total characters by 5.64×.
- **Q13 & Q16** are correctness-ties the grader preferred Octocode on but `gh+Headroom` was leaner; scored to Arm A strictly by the char tiebreaker. Reasonable graders could call these ties.
- **Q16 note:** Arm B's runner reported the `octo` wrapper mangling a shell-quoted query it had to work around — a harness artifact of this run's wrapper, not an Octocode limitation; it still answered correctly.

## Reproduce

Harness under `compare/octocode-vs-gh-headroom/tmp/run/` (gitignored): `run_arm.sh <n> <A|B>` spawns an isolated runner; `grade.sh <n>` blind-grades with a random X/Y coin (mapping in `Q<n>.map`); `tmp/bin/aggregate.py` un-blinds and tabulates. Arm-A chars from `bin/sumlog.py` (or the quote-tolerant `tmp/bin/sumA.py`); Arm-B chars from `tmp/bin/octo` → `tmp/bin/sumocto.py`.
