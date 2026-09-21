# Jev benchmarks — whole-task results and historical experiments

**Current interface:** `octocode clasify` is the only public semantic-assessment command. Jev remains the provider/model family. Legacy routes and standalone skill runners referenced below are historical and no longer runnable in the current checkout. See the [current contract](OCTOCODE_CLASIFY.md) and the [frozen real-provider evaluation](../.octocode/octocode-eval-benchmark/jev-real-api-2026-09-21/REPORT.md).

These experiments use different protocols and meters and do not establish current whole-task savings. Later five-bug and six-task host-metered evaluations did not show total-token savings. The completed source-path experiment also failed its host-token target. Keep host and provider usage separate and verify patch quality before claiming an efficiency win.

Audience: users interpreting the historical measurements and developers
extending or re-running the provider benchmark. Jev is a typed probabilistic judgment
service: you send bounded state plus typed questions (`noul` → P(yes),
`choice` → a distribution over named options, `score` → a distribution over
ordered levels); deterministic code applies the answer. Jev supplies no facts
and its output is never citable evidence.

## Source-path offloading: five paired bug fixes (2026-09-20)

Ten fresh agents used the same frozen local CLI, model and budgets. Five assisted
agents read the skill and sent Jev unread paths before implementation bodies
entered their context. All five actually received judgments before dependent
reads. This measures a prescribed workflow, not optional adoption.

| Measured total | CLI only | CLI + Jev | Change |
|---|---:|---:|---:|
| Host input + output, including cached input | 1,690,815 | 2,734,508 | +61.73% |
| Uncached host input + output | 225,855 | 292,908 | +29.69% |
| CLI calls | 46 | 72 | +56.52% |
| Summed solver seconds | 787.00 | 953.02 | +21.10% |
| Supplied-test passes | 5/5 | 5/5 | equal |
| Independently accepted sound fixes | 3/5 | 3/5 | equal |

Provider usage is separate: 31,268 input + 950 output tokens across five calls.
Independent review rejects both minimatch fixes for severe regex backtracking
and both permissive-pattern fixes for invalid conditional annotation leakage.
Both arms therefore achieve three sound fixes, despite five supplied-test passes. Agents caught two source-refuted
Jev judgments. Three initial plans stayed unchanged; the others gained bounded
refinements, with no major fix-branch change demonstrated.

**Reject automatic source-path judgment as a token-saving bug-fix protocol.**
Offloading worked mechanically, but preparation and subsequent verification
added work. Prefer a call only when its possible answers can avoid substantial
reading or change an unresolved action. Creating more answer schemas would not
address the observed failure.

Limitations: one run per case/arm, five bugs across three repositories, prescribed
skill/call policy, and common wrapper/catalog repair overhead. Assisted agents
also incurred schema-selector repair overhead. Concurrent review/build activity
confounds wall time; the timing difference is not a clean provider-latency effect.
No universal model ranking or probability calibration is inferred. Full meters,
patch reviews, frozen manifests and transcripts are retained under
`.octocode/octocode-eval-benchmark/jev-source-questions/` (see `REPORT.md`).

## Historical two-tool protocol

| Tool | Use when | Never for |
|---|---|---|
| `jevReasoning` | Source-path claims that can avoid substantial host reading, or unresolved evidence-based choices | Cheap exact checks; unchanged votes |
| `jevScout` | Filtering candidate files can avoid expensive irrelevant reads | A known cheap target; using skip to prove absence |

These retired tools required `OCTOCODE_CLASSIFICATION_API` and returned provider-billed `usage` per call. A
`blocked` or `needs_evidence` outcome is a correct result: retrieve what it
names instead of reframing the packet. Scout verdicts are provisional — reopen
the returned anchors before asserting anything, and never report absence from a
skip. At the time, the retired `octocode-jev-reasoning-loop` skill owned the
reference runners. The current [semantic-assessment research guide](CLASIFY_RESEARCH_GUIDE.md)
keeps the durable LOCATE → ASSESS → VERIFY boundary.

## Agent WITH vs WITHOUT Jev — the head-to-head (2026-09-19)

Same Sonnet agent, identical 9 tasks (6 sealed real-bug localizations + 3
GitHub fan-outs), labels hidden from every arm. Agent (worker-model) tokens
and Jev provider tokens are separate meters billed to different services.

| Arm | Correct | Agent tokens | Jev tokens | Tool calls | Time |
|---|---|---|---|---|---|
| WITH Jev, v1: agent hand-authors packets | 9/9 | 67,651 | ~29.7 k (est.) | 14 | 108 s |
| **WITHOUT** Jev (own reasoning + grep/reads) | 9/9 | 54,895 | 0 | 27 | 100 s |
| **WITH Jev, v2: zero-authoring driver** | **9/9** | **34,852** | 30,750 (measured) | **5** | **41 s** |

In this historical stage-limited comparison, quality tied. v1 used more
agent tokens while hand-writing packets and inspecting schemas. Changing
the ergonomics (`.octocode/octocode-eval-benchmark/jevpeek-scout/run-case.mjs`: one command per case, the
driver builds the packet and returns the verdict plus billed usage) changed
the measured result: **36 % fewer agent tokens than the control, 5 tool calls instead
of 27, and 2.4× faster** — with the Jev spend isolated on its own meter. The comparison excludes discovery and combines driver changes with Jev
usage; it does not establish whole-task savings or isolate the model effect.
Skip scouting when an exact search already settles the shortlist. Caveats: n = 9, one run per arm, discovery stage
excluded, v1 Jev meter estimated from identical prior runs. Raw:
`ab-results.json`.

## Measured results vs read-everything baselines (2026-09-18/19, jev-1.13.0)

Host tokens use a bytes/4 approximation; Jev tokens are billed truth. Suites,
raw results, and answer keys live in
`.octocode/octocode-eval-benchmark/jevpeek-scout/`.

| Suite | Quality | Cost |
|---|---|---|
| Find-the-file, held-out (6 cases, TS+Rust, policy frozen pre-suite) | 7/7 true files read, 0 near-miss false positives, Brier 0.007 | 0.36× read-everything, 0.49× lexical prefilter |
| Bug solving, 6 real fixed bugs (symptom → culprit) | rank-1 **6/6**, culprit recall 6/6, 0 false-skips | 0.40× read-everything, **1.45 s and ~4.2 k Jev tokens per issue** |
| PR triage, live GitHub (items mode) | 1 detail-fetch instead of 8, twice, both correct | one batched call per case |
| JS ↔ native parity | held-out 30/30 exact; bug suite 15/15 action-class (the one exact flip proved provider variance on canonically identical packets) | — |
| Sonnet worker, 20 GitHub questions with jev tools available | **19.5/20 (97.5 %)** against a pre-sealed key | 90.8 k tokens, 45 tool calls, 304 s (~4.5 k tokens/question) |

Two findings worth as much as the scores: the Sonnet worker made **zero**
scout calls on the 20-question suite and was right — every question had a
known target, and the gate doctrine transferred to an uncoached model; scout
value appears only under genuine fan-out (the fan-out suite v2 runs 6 such
cases: rank-1 5/6). Second, the original suites recorded zero false-skips
(23/23 true targets read); the adversarially evolved fan-out suite has since
produced the first two — a near-threshold skip of the top-ranked truth and a
polarity-trap miss ("removes the implementation" matched a distractor titled
"implement …") — logged as next-loop candidates, not tuned away.

## Reproduce

```sh
export OCTOCODE_CLASSIFICATION_API=...           # or ~/.octocode/.env
cd .octocode/octocode-eval-benchmark/jevpeek-scout
node heldout-runner.mjs [--native]    # find-the-file suite (JS or native tool)
node bugbench6-runner.mjs             # 6-issue suite, native, timed
node parity-check.mjs                 # no-network drift guard: class parity + wire-packet byte-compare
node jev-ledger.mjs                   # two-LLM token accounting from run artifacts
```

`--native` drives the debug binary
(`packages/octocode-native/target/debug/octocode tools jevScout …`); without it
the reference runner `skills/octocode-jev-reasoning-loop/scripts/scout.mjs`
executes. Policy v2 is frozen — tune thresholds only against a fresh held-out
suite, never the one evaluating them.

## Honest limits

All suites are author-labeled (grep-verified before sealing, but no independent
labeler yet); one Jev model version; ~140 decisions total — enough to falsify,
thin for calibration intervals. Near-threshold candidates flip
read ↔ gray_read across provider samples on identical packets, so regression
gates compare the read|gray_read class, never exact actions. GA recommendation
of `jevScout` in tool instructions waits on an independently labeled
≥100-decision suite (`.octocode/rfc/jev-scout-production/`).
