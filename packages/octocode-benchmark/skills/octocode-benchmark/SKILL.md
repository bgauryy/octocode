---
name: octocode-benchmark
description: "Use when orchestrating, running, judging, or reporting any benchmark under packages/octocode-benchmark. Tells you how to turn a plain instruction (e.g. 'compare octocode vs gh+rtk') into a full run: pick the suite, freeze the harness, pick one model for all arms, spawn runner/judge sub-agents per question, summarize each question, and roll every suite up into one result."
---

# Octocode Benchmark — Orchestrator Playbook

You are the **orchestrating agent**. You are handed a short instruction and you
turn it into a complete, comparable benchmark run. You NEVER solve questions or
judge answers yourself — you resolve the request, freeze the harness, pick the
model, spawn and seal sub-agents (runners + judges), summarize every question,
then roll every suite into one result.

## Start here — turn the instruction into a run

You will be told something like *"compare octocode vs gh"*, *"benchmark octocode
against gh+rtk"*, or *"run the ast-grep suite"*. Resolve it in this order:

1. **Pick the suite** from the treatment/baseline named (table below). If the
   user names a baseline only, the treatment is always the Octocode surface for
   that domain. If they say "run all", run all three suites in one report.
2. **Load** [`references/suites.md`](references/suites.md) for that suite's bank
   path, arm allowlists, corpus, and `questionBankHash`.
3. **Pick the model** (section below) — ONE model+settings for every arm in the run.
4. **Preflight & freeze** (run sequence step 1), then execute the sequence.

| If asked to compare… | Suite | Arm A (baseline) | Arm B (treatment) | Bank | Questions |
|---|---|---|---|---|---|
| octocode vs `gh` | `octocode-vs-gh` | `gh` CLI only | Octocode MCP remote | `questions/github/research-v2/` | Q1–Q14 |
| octocode vs `gh`+`rtk` | `octocode-vs-gh-rtk` | `gh` + `rtk` shaping | Octocode MCP remote | `questions/github/research-v2/` | Q1–Q14 |
| octocode vs `ast-grep` | `octocode-vs-ast-grep` | `ast-grep` CLI (local) | Octocode CLI (local) | `questions/local-code/ast-grep-react-v2/` | Q1–Q10 |

Every suite runs **three arms**: Control C (no tools), Baseline A, Treatment B.
C must complete and be checked for contamination before A/B are scored.

## Pick the model — one model, all arms

The tool surface is the ONLY variable. The model must be identical across C, A,
and B or the comparison is void.

- **Runner (solver) model:** one capable model, same `modelSettings` (temperature 0,
  same step/turn budget, same retry policy) for every arm. Use the user's named
  model if given; otherwise pick the strongest model you have reliable tool-call
  access to and record it. Record it in `manifest.md` and `provenance.arms.*.model`.
- **Judge model:** a strong model, run in **fresh context per question per stage**.
  It may be the same family as the runner but MUST NOT be an agent that saw any solve.
  Do not weaken the judge to save tokens — judge cost is not part of the VRPT denominator.
- **Never** mix models or settings between arms in one run. Evolve the model only
  *between* runs, and write a new `manifest.md` when you do.

## How to spawn each sub-agent — load the right reference

| What you need to do | Reference |
|---|---|
| Brief a runner/solver agent for any arm (per-question loop) | [`references/spawn-runner.md`](references/spawn-runner.md) |
| Brief a judge agent for stage 1 or 2 (per-question) | [`references/spawn-judge.md`](references/spawn-judge.md) |
| Suite-specific parameters (corpus, tool allowlists, suite paths) | [`references/suites.md`](references/suites.md) |
| File contracts — what every agent must write + kpi.json shape | [`references/outputs.md`](references/outputs.md) |
| **How to measure tokens** + the VR/VRPT comparison algorithm | [`references/measurements.md`](references/measurements.md) |

## Hard gates — enforced by you, never delegated

- `ground-truth.json` is JUDGE-ONLY. Never include it in a runner agent's context.
- No runner agent may see another arm's output, logs, or judge artifacts.
- One model + settings for every arm. Never mutate questions, rubrics, budgets, model, or verdict rules mid-run.
- Token counts come from the runner agent's telemetry. **Confirm the per-question runner-token sensor at preflight** — a verdict-eligible run REQUIRES `tokenSource: "runner"`. If per-question runner tokens are unavailable, use the labeled estimate from [`references/measurements.md`](references/measurements.md), set `tokenSource: "estimated"`, report only `VRPT-est` (byte proxy), and set the efficiency verdict to `DRAFT — tokens not captured`. Never fabricate a runner count; never claim a WIN from estimated tokens.
- **Hard caps are enforced by you.** Each question's `maxToolCalls` is a hard cap for every arm; a trial that exceeds it is `taskStatus: invalid` and re-run. For gh-rtk Arm A, any `gh` call with `rawBytes` > 50 KB must be shaped through `rtk` before reaching the solver, else the trial is invalid and re-run.
- **A WIN requires power:** `nEligible ≥ 12`, `k ≥ 3`, and non-overlapping 95% CIs on the deciding metric; otherwise the verdict is `INCONCLUSIVE (underpowered)` (see [`references/measurements.md`](references/measurements.md)).
- Control (no-tools) arm MUST finish and be evaluated for contamination before A/B are scored.
- Blinding is your responsibility: strip arm names, tool mentions, model, tokens, and metadata before handing anything to a stage-1/2 judge.
- `kpi.json` MUST validate against `schemas/kpi.schema.json`. The only conformant exemplar to copy is `fixtures/compare-run-example/kpi.json` — see [`references/outputs.md`](references/outputs.md).

## Stdout — emit this from every agent so runs are observable

```
[ORC]     PREFLIGHT suite=<name> bank=<path> questions=<N> model=<id> settings=<temp,budget>
[CTRL]    Q<NN> START
[CTRL]    Q<NN> DONE  tokens=<N> calls=0 ms=<N>
[ORC]     CONTAMINATION Q<NN> control_score=<score> status=<clean|contaminated>
[RUNNER]  Q<NN> START arm=<octocode|gh|gh-rtk|ast-grep>
[RUNNER]  Q<NN> TOOL  <toolName> raw_bytes=<N> read_bytes=<N>
[RUNNER]  Q<NN> DONE  runner_tokens=<N|Unavailable> est_tokens=<N> calls=<N> ms=<N>
[ORC]     SEAL  Q<NN> X=<arm> Y=<arm>
[JUDGE]   Q<NN> STAGE1 judge=<1|2|3> anchor <i>/<k> <repo@ref:path> <PASS|FAIL|DRIFT>
[JUDGE]   Q<NN> STAGE1 judge=<1|2|3> DONE C_X=<1-10> P_X=<1-10> R_X=<1-10> winner=<X|Y|tie>
[ORC]     Q<NN> STAGE1 AGG C_X=<mean> P_X=<mean> R_X=<mean> winner=<X|Y|tie> judgeStd=<std>
[JUDGE]   Q<NN> STAGE2 DONE flow_X=<1-5> flow_Y=<1-5> toolUsed_X=<yes|no|na>
[ORC]     UNBLIND Q<NN> X=<arm> Y=<arm>
[ORC]     Q<NN> vr_A=<N> vr_B=<N> vrpt_A=<N> vrpt_B=<N> tokens_A=<N> tokens_B=<N>
[ORC]     KPI medianVRPT_A=<N> medianVRPT_B=<N> medianVR_A=<N> medianVR_B=<N> BoverA=<N>
[ORC]     DONE  suite=<name> verdict=<WIN|TIE|LOSS|DRAFT|INCONCLUSIVE>
```

## Run sequence

```
1  preflight: load references/suites.md; pick model; confirm the runner-token
     sensor (record runner|estimated in manifest); hash & freeze bank (shasum
     must match ground-truth.questionBankHash); resolve every mutable ref/SHA;
     write manifest.md (model, settings, tool versions incl. baselines.rtk,
     tokenSource, questionBankHash, resolved SHAs, oracle date, solver count k)
2  spawn control arm (no tools) → collect → mark contaminated Qs (C >= 1.0)
3  spawn arm A runner → loop all questions → collect answers + Q<NN>.jsonl logs
4  spawn arm B runner → loop all questions → collect answers + Q<NN>.jsonl logs
5  seal: randomly assign each Q-pair to Candidate X / Candidate Y;
     write judge-mapping-SEALED.json (reveal only after all judge stages done)
6  per question: spawn 3 stage-1 judges IN PARALLEL (blind, independent context each)
     → Q<NN>_verdict_1/2/3.json; orchestrator aggregates:
       scores = mean(C,P,R across 3 judges); winner = majority vote (2/3);
       judgeStd = stdev across the 3 scores → Q<NN>_verdict.json
     Note: all 3 judges run concurrently — wall-clock = one judge call, not three.
7  unblind stage 1
8  per question: spawn stage-2 judge (sealed logs) → Q<NN>_flow.json
9  unblind stage 2 + logs
10 SUMMARIZE EACH QUESTION: per-question row (C/P/R mean, judgeStd, VR, VRPT,
     tokens) computed per references/measurements.md
11 SUMMARIZE THE SUITE: compute per-arm median VRPT / median VR / B-over-A per
     references/measurements.md; apply the pre-registered verdict rule; write
     <suite>.md (4 sections) + the suite's kpi.json rollup
12 SUMMARIZE ALL: if multiple suites ran, one kpi.json + one SUMMARY.md across
     every suite; append results/<suite>.md (latest run first)
```

Repeat steps 1–11 for each suite when running more than one; do step 12 once.

## Done — a run is complete when

- One model + settings used for C, A, and B; recorded in manifest + provenance ✓
- All solver outputs and logs sealed before any judge saw them ✓
- Stage-1 scores are the mean of 3 independent parallel judges; judgeStd reported ✓
- Stage-2 flow judge run after stage-1 is sealed ✓
- Every question summarized (VR + VRPT per arm) using per-question tokens ✓
- Per-arm rollup uses **median** VRPT (never ratio-of-totals) with the VR ≥ 0.6 floor ✓
- `kpi.json` validates against `schemas/kpi.schema.json` (exemplar: `fixtures/compare-run-example/kpi.json`) ✓
- `results/<suite>.md` updated, latest run first; losses/ties reported as prominently as wins ✓
- False-confidence counts reported; contaminated questions flagged (not dropped) ✓
