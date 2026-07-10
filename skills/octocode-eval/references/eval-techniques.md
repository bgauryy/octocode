# Eval Techniques
Load when choosing how to grade a trial. Why: wrong grader → false confidence or false failure.

## Vocabulary (Anthropic)
**Task** · **trial** · **grader** · **transcript/trace** · **outcome** (env final state) · **eval harness** · **agent harness** · **suite**.
Outcome ≠ what the agent *said* — check real state.

## Grader mix
| Kind | Use when | Examples |
|---|---|---|
| **Code / deterministic** | Objective checks | regex, unit tests, lint/type, state checks, tool presence |
| **Binary (BinEval-style)** | Interpretable failures | atomic yes/no; `failureSignature` |
| **Model / LLM judge** | Open-ended quality | rubric, pairwise, multi-judge; allow “Unknown”; calibrate |
| **Human** | Gold / calibration | SME spot-checks |
| **Council** | Contested synthesis | multi-model peer rank |

Prefer deterministic first. LLM for nuance. Humans to calibrate.

## Coding checks
- **fail-to-pass** — previously failing tests now pass (bug fixed)
- **pass-to-pass** — previously passing tests still pass (no regression)
Both needed; tests passing ≠ merge-ready taste/security.

## Capability vs regression
Capability: hard, low pass rate. Regression: near-100%. Balance should-fire and should-not-fire cases.

## Non-determinism
- **pass@k** — ≥1 success in k trials (often pass@1 for coding)
- **pass^k** — all k succeed (reliability)
Don’t trust a single green.

## Anti-patterns
Opaque holistic scores · path-only grading · ambiguous tasks · harness cheats · saturated benches without transcript reads

Next: benches → `benchmarking.md`; cases → `eval-harness.md`.
