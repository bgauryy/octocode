# Bug-Triage Benchmark — Jev vs Baseline
**Suite ID:** `bug-triage-v1`
**Frozen:** yes — do not edit cases, rubric, cap, or judge rules after the first run starts.
**Location:** `skills/octocode-jev-reasoning-loop/benchmark/`

---

## Goal

Measure whether the `octocode-jev-reasoning-loop` skill improves a research agent's ability to
diagnose and propose fixes for 10 real ambiguous GitHub bugs — bugs where at least two competing
root-cause hypotheses exist before reading source.

This is the **disputed-inference population** the prior benchmarks failed to test. Prior runs
forced Jev on deterministic lookups; this suite uses only cases that require an inference choice
after source evidence is gathered.

---

## Arms

| Arm | Octocode | Jev | Protocol |
|---|---|---|---|
| `baseline` | ≤ 8 calls, cite source at ref | none | Research, diagnose, propose fix |
| `treatment` | ≤ 8 calls, cite source at ref | optional via skill GATE | THINK → GATE → call Jev only at `disputed_inference` fork; record before/after decision |

Treatment agents **must not** force a Jev call on deterministic sub-steps. Jev is called at most
once per genuine `disputed_inference` gate. A treatment case with 0 Jev calls because no fork
was reached is valid and counted separately.

---

## Cases

All 10 cases are defined in `cases.json`. Each case specifies:
- `id` — BUG-01 … BUG-10
- `repo` — `owner/repo`
- `issueNumber` — GitHub issue number
- `title` — short label
- `hypotheses` — 2–3 competing root-cause directions **sealed before agents see source**
- `anchorRef` — commit/tag the agents must use for all source reads

Hypotheses are revealed to the judge only (not to either agent arm). They define whether a case
qualifies as `disputed_inference`.

---

## KPIs

### Primary
**Paired diagnosis quality delta (treatment − baseline)**, graded 0–10 per case:
- Correctness of root-cause identification /4
- Specificity and source-groundedness of fix proposal /3
- Research flow quality (evidence before claim, no unsupported assertions) /2
- Calibration (claims proportional to evidence; no false confidence) /1

Positive paired delta across ≥6/10 cases is required before claiming benefit. A single run is
exploratory; report-only pilot verdict.

### Leading indicators (per treatment case)
- `jevCalled` — bool; was a Jev call made?
- `gateClassification` — `deterministic` | `missing_fact` | `disputed_inference`
- `decisionChangedByJev` — bool; did before→after differ on primary hypothesis?
- `jevInputTokens`, `jevOutputTokens`, `jevLatencyMs`
- `octocodeCalls` count; `octocodeQueryRows` count

### Guardrails
- No major false claims (verifiable source contradicts a stated fact)
- No treatment arm using >8 Octocode calls
- At least 8/10 treatment cases must have a valid Jev request saved (even if call = 0)
- Capacity preflight passes before freeze (see harness.mjs)

---

## Grading

One independent judge agent per case sees both arms in randomized order (Response A / B),
re-opens cited sources at the pinned `anchorRef` (≤3 Octocode calls), and scores each arm on
the 4-dimension rubric above. De-blinded only in `inspect.mjs` aggregation.

Judge must record:
- `scoreA`, `scoreB` (0–10 each)
- `winner` — `A` | `B` | `tie`
- `majorFalseClaimA`, `majorFalseClaimB` — bool
- `jevChangeEvident` — bool (treatment only; was there observable evidence of Jev-shifted reasoning?)
- `rationale` — 2–4 sentences

---

## Decision rule

| Outcome | Verdict |
|---|---|
| Positive paired delta ≥6/10 cases, no guardrail violation | `ACCEPT — replicate` |
| Mixed (3–5 wins) or guardrail violation | `CONTINUE — adjust` |
| Negative delta or <3 wins | `REJECT — do not route by default` |

A positive pilot does not justify forced Jev routing — only optional gate-routed routing.

---

## Integrity

`harness.mjs --freeze` hashes CONTRACT.md, cases.json, judge.mjs, and inspect.mjs into
`frozen.json` before spawning agents. Any file change after freeze invalidates the run.

Runs land under `runs/baseline/BUG-{id}/` and `runs/treatment/BUG-{id}/`.
Grades land under `grades/BUG-{id}/`.
`inspect.mjs` verifies completeness and integrity before aggregating.
