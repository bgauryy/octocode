# Clean lab
Load before dispatching evaluated workers or auditing leakage. Why: a fresh conversation alone does not isolate a trial.

## Separate roles and access
| Role | May see | Must not receive |
|---|---|---|
| Solver / evaluated worker | User task, frozen subject instructions, allowed tools and raw task inputs | Grader questions, reference answers, private tests, failure labels, previous solutions, parent research conclusions |
| Developer / optimizer | Development tasks, traces, feedback; budgeted validation summaries | Sealed test contents or per-case feedback used to tune the candidate |
| Evaluator / judge | Task, frozen rubric, submitted artifact, permitted evidence and evaluator-only references | Candidate identity, preferred winner, optimizer commentary |

The experiment controller owns the full manifest. Do not send that manifest to the solver. Build an allowlisted solver packet; never copy the entire case object and remove a few obvious keys.

## Before each trial
1. Start a fresh session with no inherited parent conversation, summaries, scratchpad, or prior attempts. Apply the same production-equivalent system instructions and subject skill to both arms.
. Allocate an isolated workspace and reset fixture/service state. Audit git history, sibling folders, home memory, caches, retrieval indexes, logs, environment variables, and shared communication surfaces for prior-trial or evaluator material.
3. Inspect the actual runner export, including sample metadata copied into environment variables. Keep private tests, answer keys, grader scripts/questions, and report directories outside the solver's readable filesystem and tool credentials. A folder name or an instruction to ignore it is not an access boundary. Verify inaccessible paths through the same tool identity the worker uses.
4. Freeze tool/network policy. Online research may remain available when production requires it; check for benchmark answer retrieval and record that contamination risk. Reset writable caches; document intentional production memory or warm-cache conditions as separate experimental arms.
5. Record hashes of the actual serialized solver input, subject, fixture, catalog, and access configuration; audit unexpected fields. Metadata alone cannot prove the worker never read a leaked answer—retain access receipts and tool traces where available.
6. Seal the submitted answer and final environment state before grading. Evaluator feedback never re-enters a scored attempt. A repair is a separately labeled trial with its own budget.

If the host cannot restrict shared storage or inherited context, label the run isolation-limited and exploratory. Do not claim a clean held-out acceptance. Confirmed leakage makes the affected comparison INVALID; fix isolation and rerun both arms.

## Task quality without hints
A realistic task can name a required output location, allowed API, or business constraint. It must not add “check this buggy branch,” rubric checklists, expected answers, or a preferred tool sequence solely to help pass the eval. Natural user questions are solver input; evaluator questions about that answer are private grading input.

Example: “Repair incorrect rounding in invoice totals; preserve the public API” is a task. “Use decimal arithmetic in calculateTax; the grader checks half-cent cases” supplies a solution and private test hints.

For multi-agent systems, keep the production communication topology within a trial. Isolation separates trials and evaluator roles; it does not remove legitimate worker-to-worker dependencies from the system under test. An evaluator must not coach the solver during scoring.

Next: packet ownership → `subagent-cookbook.md`; judge isolation → `llm-judge.md`; exposed holdouts → `held-out-and-guards.md`.
