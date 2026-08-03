# Spawn a Judge Agent

Load when briefing a judge. Stage 1 judges must be fully blind — you (the
orchestrator) are responsible for stripping all arm-identifying information
before handing anything to the judge. Stage 2 uses sealed logs and must run
after stage 1 is complete and sealed.

**Judge model:** use a strong model in a **fresh context per question per stage**.
It may match the runner's model family but must never be an agent that saw a solve.
Judge tokens are NOT part of the VRPT denominator — do not down-tier the judge to
save cost. Keep the judge model fixed across all questions in a run.

## Blinding — your job before spawning any judge

Before handing files to a stage 1 judge:
1. Copy `answer_Q<NN>.md` to `judging/Q<NN>_candidate_X.md` and `judging/Q<NN>_candidate_Y.md` using a random assignment recorded in `judge-mapping-SEALED.json`.
2. Strip from both files: arm name, model, token counts, tool names, timestamps, paths with arm-specific prefixes.
3. The judge never sees `judge-mapping-SEALED.json` until after all stage 1 files for that question are sealed.

## Stage 1 — Three parallel blind judges (per question)

Spawn **3 independent agents simultaneously** for each question — same inputs, independent contexts, no communication. Parallel execution means wall-clock = one judge call, not three. After all three complete, the orchestrator aggregates.

**Aggregation (orchestrator does this, not a judge):**
- `correctness`, `precision`, `recall` → **mean** of the 3 judges' scores (round to 1 decimal)
- `winner` → **majority vote** (2/3 must agree; if 1-1-1 split, use the judge whose scores are closest to the mean as tiebreaker)
- `judgeStd` → stdev of the 3 sets of scores (flag questions with judgeStd > 1.5 as low-confidence)
- Write aggregated `Q<NN>_verdict.json`; keep raw `Q<NN>_verdict_1/2/3.json` alongside

Give each of the 3 judges:
- The question text (from `questions.md`)
- `Q<NN>_candidate_X.md` and `Q<NN>_candidate_Y.md` (blinded)
- The oracle's `requiredClaims` and `acceptedVariants` from `ground-truth.json` — ONLY for `VERIFIED_WITH_REVERIFICATION_CONTRACT` suites; for `UNVERIFIED_DRAFT` do NOT include ground truth

System prompt for stage 1:
```
You are a blind benchmark judge. You have not seen either solver's process.
You receive two answers (Candidate X, Candidate Y) to a research question.

YOUR CORE DUTY — anchor verification:
For each candidate, spot-check ≥3 decisive anchors (the citations the central claims
rest on — not the easiest ones). Fetch each cited file at the cited ref/path via a
surface outside both arms (remote: raw.githubusercontent.com or api.github.com;
local: re-read the frozen checkout at its pinned SHA). Record each as:
  PASS  — region exists and says what the answer claims
  DRIFT — right file/claim, stale line numbers or counts
  FAIL  — file, symbol, or behavior does not exist (fabrication)

Emit per anchor: [JUDGE] Q<NN> STAGE1 anchor <i>/<k> <repo@ref:path> <PASS|FAIL|DRIFT>

Score each candidate 1–10 on three dimensions:

  Correctness (1–10): Did the answer get it right?
    Any FAIL anchor → ≤ 2. Every clause correct + all decisive anchors PASS → 8–10.
    Right direction but one gap → 4–6.

  Precision (1–10): No false outputs / hallucinations?
    FAIL anchors and fabricated claims → 1–4. DRIFT only → 6–8. Nothing wrong stated → 9–10.

  Recall (1–10): No missing info?
    Missed required topics or whole capability areas → 1–4. Minor gaps → 6–7.
    Found everything important → 9–10.

Question-type rules:
- Premise trap (repo/integration may not exist): full credit requires proving the premise
  true/false with evidence. Fabricating the described architecture → Correctness ≤ 2.
- Identity trap (name collision): verify the candidate established the subject's real
  identity before tracing.
- Comparison questions: every clause needs evidence on BOTH sides; Recall ≤ 5 if one side unsupported.
- Absence claims: multiple independent probes required for Recall ≥ 7.

Write judging/Q<NN>_verdict_<JUDGE_NUMBER>.json exactly
(JUDGE_NUMBER is 1, 2, or 3 — set by the orchestrator when spawning you;
do NOT write to the shared Q<NN>_verdict.json which the orchestrator creates):
{
  "q": "Q<NN>",
  "X": {"correctness": 8, "precision": 9, "recall": 7, "note": "one-line summary"},
  "Y": {"correctness": 5, "precision": 6, "recall": 8, "note": "..."},
  "winner": "X",
  "justification": "one paragraph — which better answers every clause and why"
}

Emit: [JUDGE] Q<NN> STAGE1 judge=<JUDGE_NUMBER> DONE C_X=<score> P_X=<score> R_X=<score> C_Y=<score> P_Y=<score> R_Y=<score> winner=<X|Y|tie>
```

## Stage 2 — Flow judge (sealed logs, after stage 1 aggregated + sealed)

Only after `Q<NN>_verdict.json` exists and is sealed. A fresh agent sees the
unblinded answer files + call logs (`Q<NN>.jsonl` for both arms). Logs reveal the
arm — this stage's scores cannot retroactively affect stage 1.

```
Score flow 1–5 per arm from the trajectory:
  capability fit, call discipline (vs maxToolCalls), pagination/rate-limit handling,
  cross-checks, honest Unknowns vs confident gaps.
Record toolUsed: did the arm exercise the question's capabilityPoint? (yes | no | na)
Write judging/Q<NN>_flow.json:
{
  "q": "Q<NN>",
  "X": {"flow": 4, "toolUsed": true,  "flowNote": "..."},
  "Y": {"flow": 3, "toolUsed": false, "flowNote": "..."}
}
Emit: [JUDGE] Q<NN> STAGE2 DONE flow_X=<1-5> flow_Y=<1-5> toolUsed_X=<yes|no|na>
```

## Anti-patterns to include in every judge prompt (void the verdict)

- Checking only easy anchors or fewer than 3 decisive ones
- Using tool names, verbosity, or code style to guess which arm produced the answer
- Treating two candidates' agreement as verification
- Scoring from the candidates' own quotes without fetching sources
- Editing scores after seeing the arm mapping or another judge's output
