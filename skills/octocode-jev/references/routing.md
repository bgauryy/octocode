# Crossroads routing

Load before preparing a research request or evaluating Jev's benefit. Why: optional routing and treatment efficacy are different measurements.

## Eligibility

Classify the host's state as observable routing data, not a confidence adjective.

| Route | Required state | Next action |
| --- | --- | --- |
| `deterministic` | A verified lookup, test, rule or source path settles the proposition. | Decide from that evidence; do not call Jev. |
| `missing_fact` | The proposition depends on an unobserved source, binding, runtime result or scope. | Retrieve or run the missing observation; do not ask Jev to invent it. |
| `disputed_inference` | Two plausible interpretations each have attributable evidence, no affordable deterministic check remains, and choosing between them changes the next action. | Make one bounded Jev call, then verify its selected basis. |

Record the route, claim, strongest counterclaim, evidence IDs for each side, missing or deciding observation, whether an affordable deterministic check remains, and whether the judgment changes the next action. A quota, task difficulty, elapsed time or large context does not create a disputed inference.

## Evaluation rule

Predesignate disputed-inference cases independently of the evaluated agent. On those cases require exactly one Jev call in the treatment arm and keep the evidence packet fixed across arms. In an efficacy trial, exactly one is both the minimum and the cap; do not take the normal follow-up branch from `research.md`. Grade routing separately on a mixed suite of deterministic, missing-fact and disputed-inference cases.

A treatment run with zero Jev calls is invalid for measuring Jev benefit, not evidence of no benefit. Freeze hidden gold, packet contents, budgets and graders before either arm runs. Report routing precision/recall separately from correction, corruption, final correctness, unsupported claims, elapsed time and exact API usage.

Next: for `disputed_inference`, return to [research reasoning checks](research.md) to build and execute the packet; for the other routes, return the recorded host action without a Jev call.
