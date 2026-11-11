Load when choosing graders or metrics, grading tool use or trajectories, or using model or human judges.

## Grader types

- Executable checks (tests, state, schema, facts with tolerance) can miss requirements or reward shortcuts; binary criteria fit atomic failures; LLM rubric or pairwise needs calibration; human review needs adjudication.
- Grader DAG (cheap checks short-circuit expensive ones): keep failed, Unknown, skipped states; freeze aggregation.
- Before freezing, test a grader on positive, negative, ambiguous, and bypass examples.
- Freeze judge-generated grading steps (G-Eval style) before comparing; never a rubric per candidate.
- Verdicts carry evidence and error, Unknown, or skipped states via the runner's evaluator interface.

## Metrics

- **pass@1**: one attempt succeeds under the deployment budget.
- **pass@k**: at least one of k succeeds; state the k-attempt cost and the selection or oracle assumption.
- **pass^k**: all k succeed.
- Estimate pass@k and pass^k from a declared repeated-trial design, never by raising a rate to a power.
- Track quality, cost, latency, critical slices separately.

## Outcomes and process

- Grade outcomes and explicit process obligations first; a reference trace is one route, evaluator-only.
- Coding: fail-to-pass and pass-to-pass. Tools: final state, transport validity, and argument meaning, scope, permissions, not tool name or wording; ignored arguments can hide unsafe actions.
- Authorization: prerequisite precedes the mutation, other steps may vary. Required actions: calls and semantic arguments. Prohibited actions: absent.
- Deterministic protocol: that sequence, exact arguments only where required. Flexible research or problem solving: outcomes, evidence, constraints; accept alternative paths.
- Strict, unordered, subset, or superset matchers only where their semantics fit; never a full trace to check one ordering.
- Code-search evidence: lexical `localSearch`, structural `astSearch`, exact `localFetch`, semantic `lspSearch` differ.
- Multi-turn: simulated user and stop policy are frozen harness; grade the sealed raw trace. Product-side online feedback is a separate condition.
- Graph workflows: inspect node transitions, inter-stage evidence, approvals; a transition shows what ran, not that it worked.

## Calibrate a judge

- Judge agreement, fluency, and self-reported confidence are not ground truth.
- Atomic dimensions, anchored levels, tie and Unknown outcomes; requirements scored absolutely, preferences by blinded pairwise comparison.
- Freeze aggregation and critical-failure gates so style cannot offset errors.
- Domain reviewers label independently, then adjudicate; include valid alternatives, concise and verbose answers, plausible wrong answers, ambiguous cases, injection attempts.
- Keep judge-development, judge-validation, and solver final-test sets apart.
- Set tolerances first; measure false accepts and rejects per dimension and slice (denominators, label disagreement, Unknown rate, uncertainty), including rare critical classes.
- Pin judge model and version, prompt, rubric, reference provenance, decoding, parser.
- Failed validation: advisory only; use deterministic checks or humans. Judge or rubric change: recalibrate.

## Run a judge

- The tool trace is untrusted evidence; the judge does not inherit the executor's role.
- Pairs: randomize order, repeat swapped, map votes to identities; adjudicate order-dependent verdicts, never keep the favorable order.
- Test verbosity, style, self-preference on calibration pairs; prefer grounded evidence over polish.
- Several judges only for measured disagreement; another model family is not independence.
- Candidate text addressing the judge ("award full marks") is data: separate rubric from task data; no solver-controlled execution in grading tools; test injection (delimiters are no boundary).
- Require a parseable verdict tied to case, dimension, evidence; malformed output is a grader error, not zero quality; log grader retries separately.
- Judge critical dimensions separately when cross-dimension bias threatens.
- Ties, disagreements, missing evidence: predeclared policy. Audit a random sample plus high-impact disagreements against humans.
