# Octocode vs bare POSIX baseline — local React checkout

Local capability suite (same family as `octocode-vs-ast-grep`): **10 questions**
against a frozen local checkout of `facebook/react` — no network, no GitHub.
The only variable is the tool surface.

| Arm | Tools |
|---|---|
| **Control (C)** | none — memory only (contamination detector; React is famous, run C first) |
| **Baseline (A)** | `grep` / `find` / `cat` / `ls` / `wc` / `awk` / `sed` only |
| **Treatment (B)** | `node packages/octocode/out/octocode.js tools …` only |

## Frozen target

- Checkout: `packages/octocode-benchmark/context/react` — the shared frozen corpus
  (clone command in `benchmark/compare/octocode-vs-ast-grep/README.md` "Corpus"; gitignored)
- Commit: `9ceb1e7d9e20bd0302cf6ab31b038c5ec673178d` — verify with
  `git -C $ROOT rev-parse HEAD` before any run; re-seed ground truth if it moved.
- The codebase is Flow-typed `.js`.

## Roles

- `questions.md` — **solver-facing**. Questions only; frozen once any solver
  starts. Contains no answers, no expected values, no method or tool hints.
- `ground-truth.json` — **judge-only**. Solvers must never read it. All
  verification lives there: an **LLM judge in fresh, blind context** grades
  every answer against it, recomputing each oracle at run time on the pinned
  commit — never trusting a solver's claim or a stale seed value.

## Harness

Follow the shared method in [`../README.md`](../README.md): 3 arms, ≥3 solvers
per arm, every command logged as `{id, cmd, exit, ms, bytes, tokens, tool}`,
grading blind and in fresh context.
