Load when isolating eval roles, preparing each trial, auditing leakage, or evaluating a multi-agent subject.

# Eval isolation

## Roles and access

- **Solver** sees user task, frozen subject instructions, allowed tools, raw task inputs; never grader questions, references, private tests, failure labels, previous solutions, or orchestrator research conclusions.
- **Developer or optimizer** sees development tasks, traces, feedback, budgeted validation summaries; never sealed test contents or per-case tuning feedback.
- **Evaluator or judge** sees task, frozen rubric, submitted artifact, permitted evidence, evaluator-only references; never candidate identity, preferred winner, optimizer commentary.
- Build the solver packet from an allowlist, never by stripping keys; the controller retains the evaluator material.

## Before each trial

- Fresh session: no inherited conversation, summaries, scratchpad, or prior attempts; both arms get identical production-equivalent system instructions and subject skill.
- Isolated workspace; reset fixtures, services, writable caches (warm-cache or memory conditions are separate arms); audit git history, sibling folders, home memory, caches, indexes, logs, environment variables, shared channels for prior-trial or evaluator material.
- Private tests, answer keys, grader scripts, reports stay unreadable to the solver's filesystem and credentials, verified with the worker's own tool identity (names and ignore rules are no boundary).
- Freeze tool and network policy; online research only when production needs it, answer-retrieval risk recorded.
- Inspect the runner's real export: the actual solver input, environment and access configuration; retain the evidence needed to establish what was visible and whether isolation held.
- Seal answer and final state before grading; feedback never re-enters a scored attempt.
- Host cannot restrict shared storage or inherited context: label the run isolation-limited and exploratory; no clean sealed-test acceptance.

## Multi-agent subjects

- The trial orchestrator owns the frozen goal, actor selection and sequencing, one trial ledger, evidence merge, acceptance, and reported result. Each actor has a bounded duty and returns evidence; no actor re-enters selection or recursively delegates. Keep the production communication topology inside an isolated trial and use only host-authorized worker tools.
- Add no agents or barriers to fit the benchmark; compare against a simpler baseline; add verifiers or stages only for a measured failure.
- Actors return result, status, evidence; partial stays partial; consumers wait only for outputs they need.
- Measure the outcome at the graph boundary; per-node outcomes, stage latency, routing or merge errors locate failures; total cost, time, collisions, access violations are guardrails. More nodes or finished workers is not improvement.
- Outcome drops: replay nodes with frozen inputs; blame bad upstream data, a failed node, or a faulty merge only when local evidence reproduces it.
- Before fan-out, settle each agent's workspace, merge owner, disagreement rule; fix shared-state races with isolation, not prompts.
- Primary up, untunable guardrail down (Goodhart): stop and reframe.
- Missing anchors: executed tests with exit codes, deterministic verifiers, frozen criteria the subject cannot change.
- Opaque state, no checkpoint or resume, unbounded permissions, missing human gates: add a suite case once a trace shows it.
