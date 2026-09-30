# Unified Octocode benchmark

Two AI workers answer the same code-research questions, and a blind judge grades them. The only difference between the workers is what their instruction document gives them:

| Worker | Tools | Instructions |
|---|---|---|
| `octocode` | Octocode MCP tools (latest local build, clasify included); no shell | [workers/octocode/WORKER.md](workers/octocode/WORKER.md) |
| `rg-gh` | A shell: `rg`, `gh` and any Linux command; no Octocode | [workers/rg-gh/WORKER.md](workers/rg-gh/WORKER.md) |

Both workers use the same model (Sonnet 5.5) and the same goal paragraph. Neither doc teaches solution steps. The judge is Opus 5.5.

## Folder

| Path | What it is |
|---|---|
| `questions/QUESTIONS.md`, `questions/questions.json` | The 30 questions: 10 GitHub (PR review, GitHub code research, bug root-cause analysis) and 20 local (cloned repos at pinned commits). They are plain developer asks, with no hints, steps or tool names |
| `references/` | **Judge-only** answer keys: key facts, acceptable variations, common wrong answers. Never shown to workers |
| `workers/<id>/WORKER.md` + `profile.json` | A worker: instructions (the subject under test) plus tool wiring. Add a worker by adding a folder; the harness never branches on the worker id |
| `REFLECT.md` | The reflection prompt each worker answers after each question |
| `run.mjs` | Runs every (question × worker) as a fresh headless Claude Code session, then the worker's reflection |
| `judge.mjs` | Blind pairwise judging |
| `report.mjs` | Tables per question and totals |
| `reflect.mjs` | Merges each worker's reflections into one `REFLECT.md` |
| `results/<run-id>/` | Manifest, per-run records and reports. Raw streams are gitignored |

## Flow

```bash
cd packages/octocode-benchmark/compare/unified
node run.mjs --run-id <id> --probes           # workers answer (+ overhead and isolation probes)
node judge.mjs --run-id <id>                  # blind judge, both answer orders
node report.mjs --run-id <id>                 # results/<id>/REPORT.md
node reflect.mjs --run-id <id>                # results/<id>/reflections/<worker>/REFLECT.md
```

Prerequisites:
- the latest local build (`node skills-dev/octocode-dev/scripts/dev.mjs build:dev` from the repo root);
- `gh` logged in;
- the classification key in `~/.octocode/.env` (for clasify);
- the cloned repos listed in [repos/README.md](../../../../octocode-local-testing/repos/README.md).

## What each run records

- **Worker session:**
  - Each run is a fresh empty working directory, with no user settings, CLAUDE.md or memory (`--setting-sources ""`).
  - The worker doc is appended to the system prompt, and the user message is only the question (plus the checkout path for local questions).
- **Tokens:** read from Claude's per-request usage.
  - **Total** = every request's input (fresh + cache write + cache read) + output.
  - **Overhead** = the first request's input × the number of requests: system prompt, tool definitions, worker doc and question, re-read every turn.
  - **Research** = total − overhead.
- **Other metrics:** cost, tool calls by name, turns and time.
- **Reflection:** the worker's own session is resumed with `REFLECT.md` (one turn, no tools). Its tokens are recorded separately and never counted in the benchmark numbers.
- **Judge:**
  - It sees the question, the answer key, and both answers as X/Y in random order, with tool names scrubbed.
  - It verifies claims against the source with read-only `rg`/`gh`/`git`, and scores quality 0–10: correctness 0–5, completeness 0–3, evidence 0–2.
  - It judges every pair in both orders, with a tie-break call when the scores disagree by more than 2.
- **Efficiency** = quality per 10k tokens.

## Isolation

- The `octocode` worker has no shell. The `rg-gh` worker has a full shell (only GitHub write commands are denied, for safety).
- Every run is scanned for answer-key text in tool results, and a match marks the run invalid.
- Checkouts must be unmodified after a run.
- Both workers can reach public GitHub, so isolation covers local answer keys, not the internet.

## Adding

- **A question:** add it to `questions/questions.json` and `QUESTIONS.md` as a plain developer question, plus a judge key in `references/<id>.md`.
- **A worker:** add `workers/<id>/WORKER.md` and `profile.json`.
