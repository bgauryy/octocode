# Mental models for decisions and issue discovery

Load when a hidden assumption, uncertain choice, misleading metric or consequential failure needs a better question. Use these as investigation lenses, not proof or a mandatory checklist. Keep the domain skill's scope, permissions, verification and output.

## Choose the lens

Start with the exposed assumption. Select only models that could change the next action; stop adding lenses when they repeat the same check.

| Model | Trigger and deciding question | Action / proof |
|---|---|---|
| Black Swan | Familiar benchmarks: which environmental assumption could fail outside the examples? | Perturb inputs, ordering and dependencies in isolation; check containment and record blind spots. Finite tests cannot cover unknown events. |
| Antifragility | Recurring failure: what mechanism would turn it into durable improvement? | Keep the reproducer; validate a regression case or proposed policy change on the failure and different cases. Recovery alone is resilience; a recorded lesson is not demonstrated improvement. |
| Via Negativa | Added complexity: what could be removed while preserving required behavior? | Compare a simpler baseline; establish owners and consumers before deletion and replay acceptance/failure checks. Preserve evidence, validation and required recovery. |
| Optionality | Closing future paths: which reversible step would buy deciding evidence? | State recovery cost; verify restoration and effects rollback cannot undo. Preserve useful options without delaying settled work. |
| Second-Order Thinking | Local improvement: what happens when consumers or other agents react? | Trace retries, queues, caches and downstream transitions. Measure the full path to detect shifted cost, risk or work. |
| Inversion | Convincing happy path: what reachable chain could cause a forbidden outcome? | Work backward to guards and invariants, then trace actual entry points. Test the strongest explanation that would make the suspected failure harmless. |
| Goodhart's Law | Better score: could it rise without achieving the goal? | Freeze external acceptance and a quality guardrail; test a shortcut against them. Include failures, omissions and retries. A proxy increase alone proves no gain. |
| Bayesian Thinking | Conflicting evidence: is the observation more likely under the leading explanation or its rival? | Choose a distinguishing observation; update the conclusion and reason. Count a shared source once. Use qualitative confidence when priors or likelihoods are unjustified. |
| Knightian Uncertainty | Unknown likelihoods: which choice avoids unacceptable loss across plausible conditions? | Vary assumptions; report where the choice changes and the next deciding observation. Keep unsupported probabilities unknown; unknown is not negligible. |
| OODA Loop | Invalidated plan: what changed, and which next action would distinguish explanations? | Observe, orient to goal/constraints, decide, act and compare with expectation. After a non-informative repeat, change hypothesis or method. Stop on verified acceptance or a named blocker. |
| Lindy Effect | Novelty-driven complexity: does a proven approach satisfy the current workload? | Compare maintained, supported alternatives under the same constraints and failures. Account for survivorship bias; age alone proves neither correctness nor fit. |
| Tail-Risk Thinking | Good averages: which reachable severe failure could cascade or become irreversible? | Probe containment in isolation; verify state, recovery and affected scope. Keep impact, likelihood and confidence separate. Severity requires evidence. |

## Uncertainty, reversibility and survival

Apply these checks when consequences or a changing environment make a wrong assumption costly. Continue useful authorized work; ask only for a missing decision or authority that evidence cannot supply.

| Concern | Action | Evidence to require |
|---|---|---|
| Detect a wrong understanding | Name observations that would invalidate the current model. When one appears, refresh evidence, narrow the claim or change the hypothesis before the affected action. Hold that action if a necessary precondition cannot be established. | The contradiction, changed assumption, next deciding observation and unaffected work that can continue |
| Preserve reversibility | Prefer a preview, draft, isolated trial or staged change when it can answer the question before commitment. Check restoration against the actual state; account for effects that rollback cannot undo. | Recovery mechanism, its limits and a proving check proportionate to the consequence |
| Separate success from survival | Track task success and failure containment independently. Treat required security, integrity and authorization invariants as acceptance conditions; faster or more frequent success cannot offset violating them. Use the owner's limits, not invented quotas. | Violations, worst observed consequence, affected scope and recovery result beside success, cost and latency; untested risks remain unknown |

For long-running work, recheck relevant state before consequential transitions and after interruption. Preserve pending effects and cumulative exposure through checkpoints; reconcile uncertain outcomes before retrying. Revalidate conclusions whose inputs changed. A run with no observed severe failure does not establish zero risk.

## Issue-hunting loop

Use this technique inside the existing hypothesis-checking workflow; one agent can perform it sequentially. Scale the work to the consequence and unresolved evidence.

| Step | Do | Keep |
|---|---|---|
| Frame | State the intended outcome, invariant, scope and observable acceptance condition. Separate the real goal from its proxy. | Contract or requirement and current evidence |
| Challenge | Select a relevant lens. Write a reachable failure hypothesis, the assumption it challenges and the strongest rival. | Candidate, not a finding |
| Probe | Choose the smallest authorized check whose possible results separate the explanations. Exact-read source; confirm callers when identity matters; use execution for runtime claims. | Expected observation, actual result and source or run identity |
| Update | Mark the hypothesis confirmed, refuted or unresolved. Follow new evidence rather than trying every model. | Disproof, remaining gap and next deciding check |
| Improve | For an authorized repair, change the cause and replay the reproducer plus affected acceptance checks. In a review or test plan, hand off the repair or case without executing it. | Before/after evidence or an explicitly unexecuted proposal |
| Close | Finish when acceptance is verified and no deciding gap remains within scope, or report a blocker and unchecked scope. Propose durable learning only from validated results and within authority. | Domain output; unresolved risks remain separate from proven defects |

For a design without code, replace execution with an explicit scenario walk and label the result inferred or untested. A severe hypothetical consequence sets investigation priority; it does not turn a possibility into a confirmed defect. Report conclusions, evidence and uncertainty without printing private deliberation or the full lens checklist.

## Turn a lens into a check

| Observation | Useful lenses | Next check |
|---|---|---|
| A faster research agent reads fewer result pages | Goodhart + Via Negativa | Hold the requested evidence constant and reconstruct the selected results. Compare time only after completeness and correctness pass. |
| A write succeeds remotely but its acknowledgment is lost | Inversion + Second-Order + Tail Risk | In a sandbox, drop the acknowledgment and retry. Inspect durable state for duplicate effects; reconcile an uncertain outcome before replaying it. |
| A new dependency is proposed while usage is uncertain | Optionality + Knightian + Lindy | Compare the existing approach with a reversible prototype under explicit workload scenarios; record which missing evidence would justify adoption. |

## Concept boundaries and sources

These are engineering applications, not evidence that naming models improves agents. Known edge cases are not automatically Black Swans; incident learning alone does not demonstrate mathematical antifragility.

- [Taleb: The Black Swan, prologue](https://www.fooledbyrandomness.com/TheBlackSwan.pdf): unexpected events with extreme impact and retrospective explanations.
- [Taleb: Antifragile](https://www.fooledbyrandomness.com/Antifragile.htm): benefiting from variability goes beyond merely resisting it.
- [Google DeepMind: Specification gaming](https://deepmind.google/blog/specification-gaming-the-flip-side-of-ai-ingenuity/): satisfying an objective's specification can miss the intended outcome.
