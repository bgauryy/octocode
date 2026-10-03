# Clean lab
Load before you dispatch evaluated workers or audit leakage. A fresh conversation alone does not isolate a trial.

| Role | May see | Must not receive |
|---|---|---|
| Solver (evaluated worker) | User task, frozen subject instructions, allowed tools, raw task inputs | Grader questions, reference answers, private tests, failure labels, previous solutions, parent research conclusions |
| Developer or optimizer | Development tasks, traces, feedback; budgeted validation summaries | Sealed test contents or per-case feedback used to tune |
| Evaluator or judge | Task, frozen rubric, submitted artifact, permitted evidence, evaluator-only references | Candidate identity, preferred winner, optimizer commentary |

The controller owns the full manifest; never send it to the solver. Build an allowlisted solver packet; never copy the case object and strip keys.

## Before each trial
1. Start a fresh session: no inherited conversation, summaries, scratchpad, or prior attempts. Give both arms the same production-equivalent system instructions and subject skill.
2. Isolate the workspace; reset fixture and service state. Audit git history, sibling folders, home memory, caches, retrieval indexes, logs, environment variables, and shared communication surfaces for prior-trial or evaluator material.
3. Inspect the runner's real export, including metadata copied into environment variables. Keep private tests, answer keys, grader scripts, and report folders outside the solver's readable filesystem and credentials. A folder name or ignore instruction is not an access boundary; verify inaccessibility with the worker's own tool identity.
4. Freeze tool and network policy. Keep online research if production needs it; check and record benchmark-answer retrieval risk. Reset writable caches; run memory or warm-cache conditions as separate arms.
5. Hash the serialized solver input, subject, fixture, catalog, and access configuration; audit unexpected fields. Metadata cannot prove the worker never read a leaked answer; keep access receipts and tool traces.
6. Seal the answer and final state before grading. Evaluator feedback never re-enters a scored attempt. A repair is a separately labeled trial with its own budget.

- If the host cannot restrict shared storage or inherited context, label the run isolation-limited and exploratory; claim no clean held-out acceptance.

## Task quality without hints
- A realistic task can name a required output location, allowed API, or business constraint. It must not add bug locations, rubric checklists, expected answers, or a preferred tool sequence.

Next: choose graders with `references/graders.md`.
