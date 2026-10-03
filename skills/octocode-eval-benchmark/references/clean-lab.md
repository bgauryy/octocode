# Clean lab
Load before you dispatch evaluated workers or audit leakage. A fresh conversation alone does not isolate a trial.

| Role | May see | Must not receive |
|---|---|---|
| Solver (evaluated worker) | User task, frozen subject instructions, allowed tools, raw task inputs | Grader questions, reference answers, private tests, failure labels, previous solutions, parent research conclusions |
| Developer or optimizer | Development tasks, traces, feedback; budgeted validation summaries | Sealed test contents or per-case feedback used to tune |
| Evaluator or judge | Task, frozen rubric, submitted artifact, permitted evidence, evaluator-only references | Candidate identity, preferred winner, optimizer commentary |

The controller owns the full manifest; never send it to the solver. Build an allowlisted solver packet; never copy the case object and strip a few keys.

## Before each trial
1. Start a fresh session: no inherited conversation, summaries, scratchpad, or prior attempts. Give both arms the same production-equivalent system instructions and subject skill.
2. Allocate an isolated workspace; reset fixture and service state. Audit git history, sibling folders, home memory, caches, retrieval indexes, logs, environment variables, and shared communication surfaces for prior-trial or evaluator material.
3. Inspect the runner's real export, including metadata copied into environment variables. Keep private tests, answer keys, grader scripts, and report folders outside the solver's readable filesystem and credentials. A folder name or an ignore instruction is not an access boundary; verify inaccessibility with the worker's own tool identity.
4. Freeze tool and network policy. Keep online research if production needs it; check for benchmark answer retrieval and record that risk. Reset writable caches; run intentional memory or warm-cache conditions as separate arms.
5. Hash the serialized solver input, subject, fixture, catalog, and access configuration; audit unexpected fields. Metadata cannot prove the worker never read a leaked answer; keep access receipts and tool traces.
6. Seal the answer and final state before grading. Evaluator feedback never re-enters a scored attempt. A repair is a separately labeled trial with its own budget.

- If the host cannot restrict shared storage or inherited context, label the run isolation-limited and exploratory; claim no clean held-out acceptance.
- Confirmed leakage makes the comparison INVALID; fix isolation and rerun both arms.

## Task quality without hints
- A realistic task can name a required output location, allowed API, or business constraint. It must not add bug locations, rubric checklists, expected answers, or a preferred tool sequence to help pass.
- Natural user questions are solver input; evaluator questions about the answer are private grading input.
- Example: "Repair incorrect rounding in invoice totals; preserve the public API" is a task. "Use decimal arithmetic in calculateTax; the grader checks half-cent cases" leaks the solution and private tests.
- Multi-agent subjects keep their production communication topology inside a trial. Isolation separates trials and evaluator roles, not legitimate worker dependencies. An evaluator never coaches the solver during scoring.

Next: workflow roles → `references/multi-agent.md`; judge isolation → `references/llm-judge.md`; exposed holdouts → `references/held-out-and-guards.md`.
