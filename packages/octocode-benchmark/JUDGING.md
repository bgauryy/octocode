# Judge Protocol

One doc for everything a judge does. Scoring math: [`SCORING.md`](SCORING.md).
Run mechanics: [`INSTRUCTIONS.md`](INSTRUCTIONS.md).
A judge reads THIS file, the question, and the two blinded candidates — nothing else.

## Who the judge is

A fresh agent that did not observe either solve. It never sees arm names, paths,
logs, token counts, or the sealed X/Y mapping. Fresh context per question
prevents cross-question style-matching.

## Stage 1 — outcome judging (per question, blind)

**Inputs:** question text + `Q<NN>_candidate_X.md` + `Q<NN>_candidate_Y.md`
(run-metadata stripped, tool phrases scrubbed by the orchestrator).

**Verification duty — the core of the job.** For each candidate:

1. Spot-check **≥3 decisive anchors** (the citations the answer's central claims
   rest on — not the easiest ones). Fetch each cited file at the cited ref/path
   via a surface **outside both arms**:
   - Remote suites: `raw.githubusercontent.com` / `api.github.com`
   - Local suites: re-read the frozen checkout at its pinned SHA
2. Classify each anchor:
   - **PASS** — region exists and says what the answer claims
   - **DRIFT** — right file/claim, stale line numbers or counts
   - **FAIL** — file, symbol, or behavior does not exist (fabrication)
3. For absence claims ("no integration exists"): verify the candidate showed
   **multiple independent probes**. One empty search proves nothing.

**Scores — output 1–10 for each:**

| Field | Scale | Anchor guidance |
|---|---|---|
| **Correctness** | 1–10 | Any FAIL anchor → ≤ 2 (VR = 0). Every clause correct + all decisive anchors PASS → 8–10. Right direction but one gap → 4–6. |
| **Precision** | 1–10 | FAIL anchors and fabricated claims → 1–4. DRIFT only → 6–8. Nothing wrong stated → 9–10. |
| **Recall** | 1–10 | Missed required topics or whole capability areas → 1–4. Minor gaps → 6–7. Found everything important → 9–10. |

**Question-type rules:**

- **Premise trap** (repo/integration may not exist): full credit requires proving
  the premise true/false with evidence. Fabricating the described architecture →
  Correctness ≤ 2.
- **Identity trap** (name collision): verify the candidate established the
  subject's real identity before tracing.
- **Comparison questions**: every clause needs evidence on BOTH sides; Recall ≤ 5
  if one side is unsupported.
- **Absence claims**: multiple independent probes required for Recall ≥ 7.
- **Reasoning-over-primitives**: verify cited primitives are real (Precision);
  judge the reasoning's soundness (Correctness), not whether a runnable proof exists.

**Output contract — write `judging/Q<NN>_verdict.json`:**

```json
{
  "q": "Q01",
  "X": {
    "correctness": 8,
    "precision": 9,
    "recall": 7,
    "note": "one-line summary of strengths/gaps"
  },
  "Y": {
    "correctness": 5,
    "precision": 6,
    "recall": 8,
    "note": "..."
  },
  "winner": "X",
  "justification": "one paragraph — which better answers every clause and why"
}
```

Emit: `[JUDGE] Q<NN> STAGE1 judge=<N> DONE C_X=<score> P_X=<score> R_X=<score> C_Y=<score> P_Y=<score> R_Y=<score> winner=<X|Y|tie>`

## Stage 2 — flow judging (per question, sealed logs, after stage 1 sealed)

Only after `Q<NN>_verdict.json` exists and is sealed. A fresh agent sees the
unblinded answer files + call logs (`Q<NN>.jsonl` for both arms). Logs reveal the
arm — this stage's scores cannot retroactively affect stage 1.

Score `flow` 1–5 per arm from the trajectory: capability fit, call discipline
vs the soft cap, pagination/rate-limit handling, cross-checks, honest Unknowns.
Record `toolUsed`: did the arm exercise the question's `capabilityPoint`?

**Output — write `judging/Q<NN>_flow.json`:**

```json
{
  "q": "Q01",
  "X": {"flow": 4, "toolUsed": true,  "flowNote": "..."},
  "Y": {"flow": 3, "toolUsed": false, "flowNote": "..."}
}
```

Emit: `[JUDGE] Q<NN> STAGE2 DONE flow_X=<1-5> flow_Y=<1-5> toolUsed_X=<yes|no|na>`

A correct answer without the expected workflow stays correct but is labeled
"answered without the expected workflow."

## Control-arm (no-tools) scoring — contamination detection only

The control arm is scored for **contamination**, not quality. Score it on
**anchor-level recall only**: does the no-tools answer state the required atomic
claims from the rubric (specific file/path/line/PR#/issue#/count), verified the
same way as the solver arms?

- A control answer that describes the broad architecture but cites **no verified
  anchor** scores **0** — this is *not* contamination. Guessing the shape of a
  famous system from parametric memory is not fact retrieval.
- Mark a question **contaminated** only when the control reaches the rubric's
  required anchors (`controlCorrectness ≥ 1.0`). Contaminated questions stay
  visible but are excluded from the primary mean — never silently dropped or
  down-weighted.
- Record `controlCorrectness` per question so the report's guardrails section
  can show the basis of every contamination flag.

## Anti-patterns (each one voids the verdict)

- Scoring from snippets or the candidates' own quotes without fetching sources.
- Checking only the easiest anchors, or fewer than 3 decisive ones.
- Using tool names, style, or verbosity to guess arms — and letting it bias scores.
- Treating two candidates' agreement as verification.
- Reading ground truth for an `UNVERIFIED_DRAFT` suite as if it were an oracle.
- Flagging a question contaminated because the control *sounded* plausible —
  contamination requires anchor-level recall, not narrative.
- Producing an efficiency verdict from estimated-only tokens, k=1, or
  overlapping 95% CIs (see [SCORING.md](SCORING.md)).
- Editing scores after seeing the arm mapping or another judge's output.
