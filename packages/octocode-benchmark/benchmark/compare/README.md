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

## Method

1. **Two arms, same task.** Arm A gets only its baseline toolchain; Arm B gets
   only the Octocode CLI (`node packages/octocode/out/octocode.js`). Same model,
   same prompts, same step budget. Freeze the harness for the run.
2. **Independent solvers.** Run ≥3 solver agents per arm so a single-agent fluke
   doesn't decide a cell.
3. **Log every command** as `{id, cmd, exit, ms, bytes, tokens}` — **tokens are
   the primary cost metric** alongside correctness.
4. **Blind, deterministic grading.** Grade outcomes against `ground-truth.json`
   (never which arm produced them). Prefer exact `file:line` / PR# / count
   oracles; use the rubric only for open-ended parts.

## Metrics (per question, per arm)

| Metric | Meaning |
|---|---|
| correctness | rubric score vs oracle (0 / 0.5 / 1.0) |
| tokens | total output tokens across the solve (primary cost) |
| turns | tool calls to reach the answer (fewer = better) |
| wall-clock | seconds to answer |
| false-confidence | wrong answer asserted as proof (must not increase) |

**Decision rule.** Octocode "wins" a suite only if it improves mean correctness
**and** does not regress token cost beyond a pre-registered ceiling (e.g.
correctness ≥ baseline at ≤ 1.5× tokens). Report dropped/timed-out questions
explicitly — silent truncation reads as coverage that didn't happen.

## Validity

- Ground truth is verified by a method **outside both arms** (e.g. WebFetch
  against `raw.githubusercontent.com` / `api.github.com`), never by either
  toolchain grading itself.
- GitHub facts (PR/issue state, line numbers, counts) are **time-sensitive** —
  re-verify before trusting an old snapshot; record the run date.
- Absence traps ("does repo Y define Z?" where it does not) test that a tool
  reports honest absence instead of a confident false negative.
