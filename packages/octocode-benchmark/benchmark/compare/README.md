# Comparison Benchmarks — Octocode vs Others

Three head-to-head suites, **10 questions each**. Same LLM, same questions, same
budget — the only variable is the **tool provider**. This answers the one
question that matters: *does an agent solve real code tasks better with Octocode
than with the standard tools?*

| Suite | Arm A (baseline) | Arm B | What it stresses |
|---|---|---|---|
| [octocode-vs-gh](octocode-vs-gh/) | `gh` CLI only | Octocode CLI | GitHub research: code search, file fetch, PRs, history, structure |
| [octocode-vs-gh-rtk](octocode-vs-gh-rtk/) | `rtk` (grep/read/ls/tree/find/git/json/wget) + `gh` CLI | Octocode CLI | Full research-flow shapes across 9 repos, 3 languages |
| [octocode-vs-ast-grep](octocode-vs-ast-grep/) | `ast-grep` CLI | Octocode CLI (`localSearchCode`) | Structural/AST search — parity where both apply, and where Octocode goes beyond AST (LSP/text/remote/minify) |

Each suite folder has:

- `questions.md` — **solver-facing**. Frozen once any solver starts. Never contains answers.
- `ground-truth.json` — **judge-only**. Solvers must never read it.
- `README.md` — the question map + per-suite notes.

## The three arms (not two)

A tool-provider A/B needs a **control** or the result is unattributable — a
strong LLM can answer famous-repo questions from memory, scoring both tooled arms
high regardless of tool quality (construct-validity failure).

| Arm | Tools | Role |
|---|---|---|
| **Control (C)** | none — LLM answers from memory only, no tool calls | Contamination detector. If C already scores high on a question, that question measures memory, not tools → **down-weight or replace it**. |
| **Baseline (A)** | the suite's baseline toolchain (`gh` / `rtk`+`gh` / `ast-grep`) | The thing Octocode must beat. |
| **Treatment (B)** | Octocode CLI only | The system under test. |

Run C first. A question earns its place only if `C < A,B` — i.e. tools actually
matter for it.

## Method

1. **Freeze the harness.** Same model, prompts, step budget. Arm A gets only its
   baseline toolchain; Arm B only `node packages/octocode/out/octocode.js`; Arm C
   no tools. Evolve the suite only *between* runs, never mid-run.
2. **Independent solvers, aggregated.** ≥3 solvers per arm. Report **pass@1 mean
   correctness** (capability) **and** **pass^k** (did all k solvers succeed —
   reliability). Don't trust a single green; report variance.
3. **Log every command** as `{id, cmd, exit, ms, bytes, tokens, tool}` — these
   are the deterministic **anchors** (tokens = primary cost metric; `tool` names
   which tool was called, for trajectory grading).
4. **Grade in fresh context, blind.** The judge is a separate agent that never
   saw the solve and does not know which arm produced an answer. A verifier
   sharing the executor's context is not independent.

## Grading — two layers

Correctness alone is not enough for a tool comparison. Grade both **outcome** and
**tool-use trajectory**, deterministic-first.

### Layer 1 — outcome (deterministic where possible)

| Part shape | Grader kind | Example |
|---|---|---|
| PR#/issue#, file path, symbol name, regex literal, count, YES/NO | **code / deterministic** | `answer.pr === 15035` |
| ordered lists, deltas, source/test split | **code** | set/sequence compare |
| "explain the trade-off", RCA in own words | **calibrated LLM judge** | rubric in `ground-truth.json`, allow "Unknown" |

Each `ground-truth.json` question's `scoring` field is a DAG-style partial-credit
rubric (1.0 / 0.5 / 0). Score deterministic parts by code; route only open-ended
parts to the judge.

### Layer 2 — tool-use (attributes the win to the tools)

Reference-free trajectory check (`trajectory-grading.md`, **unordered** match):
did the arm actually use the differentiating capability the question targets?
Each question carries a `capabilityPoint` — the grader confirms Arm B exercised
it and records whether Arm A *could* (often it cannot):

- Q "structural" → Arm B called `localSearchCode mode:"structural"` (not text).
- Q "callers" → Arm B called `lspGetSemantics` (not a text grep).
- Q "rate limit" → count each arm's GitHub **code-search** calls.

A correct answer reached **without** the differentiating tool still counts for
outcome, but the tool-use layer records it as "answered without the tool" — which
is itself a finding (the tool didn't help here).

## Metrics (per question, per arm)

| Metric | Kind | Meaning |
|---|---|---|
| correctness (pass@1 mean) | primary | rubric score vs oracle (0 / 0.5 / 1.0) |
| reliability (pass^k) | primary | all k solvers correct? |
| quality | secondary | `1–5` — is the final answer exact, concise, well-anchored? (judge-scored) |
| tokens | guardrail (untunable) | total output tokens across the solve |
| time (wall-clock) | guardrail | seconds to answer |
| turns | guardrail | tool calls to answer |
| false-confidence | guardrail (untunable) | wrong answer asserted as proof — must not increase |
| tool-used | trajectory | did the arm use the differentiating tool? |
| control-lift | validity | `B − C` and `A − C`; if ~0 the question is contaminated |

**Decision rule (pre-register before running).** Octocode "wins" a suite iff mean
correctness improves over baseline **on questions with control-lift > 0** (i.e.
uncontaminated) **and** no guardrail regresses past its ceiling (e.g. correctness
≥ baseline at ≤ 1.5× tokens, false-confidence not up). Report dropped/timed-out
and contaminated (control-passed) questions explicitly — silent truncation reads
as coverage that didn't happen.

## Validity gates

- **Runnable-sensor gate.** A suite is a **ship-gate only when every oracle is
  independently verified and frozen.** Today: `octocode-vs-gh-rtk` is verified
  (ship-gate). `octocode-vs-gh` and `octocode-vs-ast-grep` carry
  `draft-verify-before-scoring` oracles — they are **orientation only** until a
  one-time verification pass freezes them. Do not report a "win" from a draft
  suite.
- **Ground truth outside both arms.** Verify with a method neither arm uses
  (WebFetch `raw.githubusercontent.com` / `api.github.com`), never by a toolchain
  grading itself. Parity oracles (vs-ast-grep Q1–Q5) whose oracle is "both tools
  agree" additionally need an **independent third-method spot-check** on a sample
  (manual/`grep -c`) — agreement alone is self-referential (both tools can be
  wrong the same way).
- **Contamination.** Famous repos/PRs are high-leakage; the control arm detects
  it per question. Prefer targets pinned to commits/PRs *after* the model's
  training cutoff where possible.
- **Time-sensitivity.** PR/issue state, line numbers, and counts drift —
  re-verify before a scored run and record the run date.
- **Cheat resistance.** Never edit graders/questions mid-run to move a number
  (REJECT). ~0% on a question → debug the task/grader first, not the tool.
- **Capability vs regression balance.** Keep should-fire (hard) and
  should-not-fire cases (e.g. the vs-gh Q6 absence trap) so honesty is scored,
  not just recall.
