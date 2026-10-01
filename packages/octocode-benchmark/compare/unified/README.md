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
node run.mjs --run-id <id> --model claude-sonnet-5-5 --probes           # workers answer (+ overhead and isolation probes)
node judge.mjs --run-id <id> --model claude-opus-5-5                  # blind judge, both answer orders
node report.mjs --run-id <id>                 # results/<id>/REPORT.md
node reflect.mjs --run-id <id>                # results/<id>/reflections/<worker>/REFLECT.md
```

Prerequisites:
- macOS with working `sandbox-exec` (other platforms fail closed); verified solver isolation runs before each session;
- concrete versioned worker and judge model names, never rolling aliases;
- the latest local build (`node skills-dev/octocode-dev/scripts/dev.mjs build:dev` from the repo root);
- `gh` logged in;
- the classification key in `~/.octocode/.env` (for clasify);
- the cloned repos listed in [repos/README.md](../../../../octocode-local-testing/repos/README.md).

## What each run records

- **Worker session:**
  - Each run is a fresh empty working directory, with no user settings, CLAUDE.md or memory (`--setting-sources ""`).
  - The worker doc is appended to the system prompt, and the user message is only the question (plus the checkout path for local questions).
- **Tokens:** authoritative final Claude `result.usage`, independently reconciled with `modelUsage`. Provisional assistant usage is retained with discrepancies; incomplete/disagreeing final accounting invalidates a session.
  - **Total** = finalized fresh input + cache write + cache read + output.
  - **Overhead/research** are estimates based on initial context × request count; they are not verified per-request accounting or evidence of fixed context after compaction.
  - **Weighted tokens** are unknown until a concrete model tariff is frozen. Cache TTL breakdown and actual reported cost are retained. Claude usage excludes classification provider calls unless separately instrumented; never call this total system cost.
- **Other metrics:** cost, tool calls by name, turns and time.
- **Reflection:** the worker's own session is resumed with `REFLECT.md` (one turn, no tools). Its tokens are recorded separately and never counted in the benchmark numbers.
- **Judge:**
  - It sees the question, the answer key, and both answers as X/Y in random order, with tool names scrubbed.
  - It verifies claims against the source with read-only `rg`/`gh`/`git`, and scores quality 0–10: correctness 0–5, completeness 0–3, evidence 0–2.
  - It judges every pair in both orders, with a tie-break call when the scores disagree by more than 2.
- **Efficiency** = quality per 10k raw total tokens. Weighted efficiency remains unknown without a frozen tariff; reported Claude session cost excludes classification provider cost.

## Isolation and validity

Each solver runs in a verified macOS sandbox. It cannot read evaluator artifacts, prior benchmark results, `.octocode`, original Claude history/memory, or gh configuration. Sources are read-only; writes are confined to a per-session directory that is deleted after reflection. Direct network access is denied except the session's exact evaluator gateway endpoint/socket. A denied boundary probe fails closed before billed sessions.

The rg/gh worker uses the real CLI through its documented `http_unix_socket` configuration. The evaluator-owned gateway attaches upstream credentials and accepts only REST GET/HEAD. Implicit POST (`gh api -f`), GraphQL POST, and all other methods are rejected before upstream execution. Direct GitHub traffic cannot bypass the gateway. This REST-only capability is explicitly declared in the worker subject.

The Octocode worker uses a small stdio bridge to an evaluator-owned **actual native MCP server**, configured with the permitted corpus roots. Write tools are unavailable; a separate inherited native sandbox also denies writes to every corpus root, including LSP children. Native MCP retains provider/auth access outside the solver filesystem. Claude's model connection uses a CONNECT proxy restricted to a frozen list of model service hosts. The solver receives only its own model authentication, never upstream GitHub/classifier credentials.

Each native MCP session has a separate persistent stats home. Per-question records preserve actual reported classification input/output totals and known/unknown successful usage counters. Grouped questions and cached tool calls do not equal provider requests. Failed provider attempts have no independently reported billing, so classification cost and total system cost remain unknown. Judge retries retain every attempt's accounting; incomplete attempt usage cannot certify a complete report.

Run/resume freezes model/limits/Claude version, prompts/questions/references, worker docs/profiles, gateway policy, build/contract/dependency/config hashes and corpus state. Invalid/failed probes, MCP disconnection, malformed/incomplete usage, signal termination, mutated inputs/corpus, missing workers or stale judgments cause a nonzero exit. Judge/report require complete valid matched pairs. Numeric quality still requires independent judge calibration and a predeclared release threshold; a complete report is diagnostic evidence, not automatic production approval.

Reference files are never protected by chmod. The solver OS boundary prevents access to them and historical results even through encoded paths or interpreter commands. Questions and grader rubric/reference content remain unchanged.

## Adding

- **A question:** add it to `questions/questions.json` and `QUESTIONS.md` as a plain developer question, plus a judge key in `references/<id>.md`.
- **A worker:** add `workers/<id>/WORKER.md` and `profile.json`.
