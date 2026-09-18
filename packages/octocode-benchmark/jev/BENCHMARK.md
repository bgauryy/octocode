# Benchmark Jev in the local Octocode research flow

Use this runbook to measure whether Jev improves conclusions produced by an Octocode research agent. The benchmark separates two questions:

1. Does the agent recognize when Jev is appropriate?
2. When you apply Jev to a real inference crossroads, does it improve the final answer?

Do not combine these questions into one score. An optional arm can correctly make zero Jev calls and therefore provide no evidence about Jev's treatment effect.

## Benchmark contract

This runbook's three-class claim benchmark is the historical baseline. For `octocode-jev-reasoning-loop`, create a new suite version with `hypothesis_triage` as a fourth routing class; never relabel or overwrite the sealed baseline. Give triage cases observations, explicit hypotheses, unknowns and candidate checks. Freeze which checks are useful and what observation would distinguish the hypotheses. Missing facts can block a conclusion while leaving a useful next-check judgment possible.

Use `scripts/jev.mjs evaluate` for triage packets and keep exact requests and responses together with harness-generated hashes. The two-question bound `scripts/research.mjs` contract below applies to claim checks only. In triage efficacy cases grade the selected check, whether the host preserves the hypothesis as provisional, the evidence obtained after that check, and final correctness. Match retrieval budgets between arms; a fixed-packet pilot alone cannot measure investigation speed or tool savings. Include irrelevant checks, no useful candidate, misleading hunches, local evidence and versioned external evidence.

Score three stages separately: Jev's typed choice, the host's case-specific application, and the final research outcome. A generic status sentence fails application even when the typed choice is correct. Keep exact host model/sampling metadata and record unavailable token telemetry as null with a reason. The September 18 replication failed at host composition; it did not reproduce the initial final-answer lift.

Freeze the following contract before either evaluated arm runs:

- **Subject:** one bounded Jev judgment over a fixed evidence packet.
- **Host:** the same fresh agent model and reasoning configuration in both arms.
- **Jev model:** a pinned version, not a moving alias.
- **Primary routing KPI:** macro-F1 over `deterministic`, `missing_fact`, and `disputed_inference`.
- **Primary efficacy KPI:** paired final-correctness delta on valid disputed-inference cases.
- **Guardrails:** unsupported claims, corruption rate, packet parity, source verification, and protocol validity.
- **Budget:** one host turn, a fixed answer limit, zero retrieval after packet freeze, and exactly one Jev call in each treatment case.
- **Decision rule:** a pilot reports observations only; a product decision requires a larger independently sealed replication.

Store permanent suite definitions under a versioned benchmark directory. Store requests, envelopes, answers, and grades alongside that suite, outside the shipped skill.

## Build the suite

Create two sections with hidden labels and hidden gold.

### Route cases

Include all three route classes:

| Route | Public packet state | Correct action |
| --- | --- | --- |
| `deterministic` | A verified rule, test, source path, or exact result settles the claim. | Decide from evidence and skip Jev. |
| `missing_fact` | A source, binding, runtime result, scope, or account observation is absent. | Retrieve the fact outside the frozen evaluation or report insufficiency; skip Jev. |
| `disputed_inference` | Two source-backed interpretations remain, no affordable deterministic check settles them, and the choice changes the next action. | Prepare one bounded Jev call. |

Do not expose the route label in case IDs, ordering, filenames, or packet wording. Grade the route separately from conclusion quality.

### Efficacy section

Predesignate disputed-inference cases independently of the evaluated agents. Each case must include:

- One falsifiable claim and its strongest plausible counterclaim.
- Neutral, attributable evidence for both interpretations.
- Unique evidence IDs and caller-owned evidence-basis IDs.
- Explicit scope, revision, and unknowns.
- A hidden conclusion with required semantic atoms.
- Required original-source anchors.
- Unsupported-claim traps.
- A byte-stable packet shared by baseline and treatment.

Reject a case when another exact lookup can settle it, evidence exists for only one side, or the decision does not change the next action.

Start a pilot with at least four disputed pairs, two deterministic controls, and two missing-fact controls. Treat it as report-only. Use at least 30 independently sealed disputed pairs and include controls across multiple fresh runs before changing product policy.

## Define the arms

Use fresh independent host contexts.

### Baseline arm

- Read the frozen packet only.
- Make zero Jev calls.
- Use no repository, network, retrieval, or additional model calls.
- Return the route, conclusion, source evidence IDs, and unsupported-claim self-audit.

### Treatment arm

- Read the byte-identical frozen packet.
- Make exactly one remote Jev call with retries disabled.
- Pin the Jev model version.
- Do not retrieve evidence, mutate the packet, or call another model.
- Recheck the saved bound envelope locally.
- Verify the selected basis against the supplied original-source evidence.
- Return the same answer shape as baseline plus exact Jev usage.

Zero calls, more than one call, a nonzero research exit, model drift, packet drift, or extra evidence retrieval invalidates the pair. Do not score an invalid pair as zero improvement.

## Preflight every request

Run curator-only local validation before sealing the suite. Materialize every treatment request and pass it through:

```sh
node <skill-dir>/scripts/jev.mjs evaluate \
  --input <request.json> \
  --dry-run
```

The request must contain only `model`, `state`, and `questions`. Choice criteria map labels to descriptions. Every question has explicit instructions. The preflight makes no remote call and is not part of either evaluated arm.

If preflight fails, repair the harness before sealing. If a frozen run reveals a harness defect, preserve that run as a sensor failure, create a new suite version, and restart both arms. Never edit a sealed suite in place.

## Run one treatment case

Use the atomic research command:

```sh
node <skill-dir>/scripts/research.mjs \
  --input <request.json> \
  --retries 0 \
  --timeout-ms 10000 > <envelope.json>
```

Then validate the saved binding without another API call:

```sh
node <skill-dir>/scripts/check-research.mjs \
  --request <request.json> \
  --response <envelope.json>
```

Exit `0` means the advice is internally coherent and bound to the request. It does not prove that the selected source is true or fresh. The host still verifies the original evidence.

## Record each case

Record at least these fields:

```json
{
  "suite_id": "...",
  "case_id": "...",
  "arm": "baseline | treatment",
  "route": "deterministic | missing_fact | disputed_inference",
  "packet_sha256": "...",
  "host_model": "...",
  "host_prompt_tokens": null,
  "host_completion_tokens": null,
  "host_turns": 1,
  "tool_calls": 0,
  "remote_jev_api_calls": 1,
  "jev_model": "jev-1.13.0",
  "jev_request_sha256": "...",
  "jev_response_sha256": "...",
  "jev_provider_input_tokens": 0,
  "jev_provider_output_tokens": 0,
  "research_exit_code": 0,
  "local_envelope_checks": 1,
  "started_at_utc": "...",
  "ended_at_utc": "...",
  "elapsed_ms": 0
}
```

Use integer zero for no-call counters. Use `null` only when the provider does not expose a token field, and state that limitation. Never store credentials.

## Grade routing

Compute a confusion matrix over the frozen suite's routes (three in the historical baseline; four in a hypothesis-triage suite), then report:

- Precision, recall, and F1 for each route.
- Macro-F1 across the suite's route classes.
- Overall route accuracy.
- False-positive Jev routes, which measure wasted calls.
- False-negative disputed routes, which measure missed opportunities.

Do not award routing credit because the final conclusion happened to be correct. A deterministic answer labeled disputed and a disputed answer labeled deterministic are routing errors even when their conclusions survive.

## Grade efficacy

For each protocol-valid pair, grade the final conclusion against hidden semantic atoms, required evidence anchors, and unsupported-claim traps. Report:

- Baseline and treatment correctness.
- Paired correctness delta.
- Correction rate: baseline wrong and treatment right.
- Corruption rate: baseline right and treatment wrong.
- Unsupported-claim rate for both arms.
- Valid treatment rate.
- Jev calls, provider tokens, bytes, and elapsed time.

Keep routing metrics and efficacy metrics in separate sections. Jev does not receive credit for a routing decision made by the host.

## Manage context

Treat Jev's context as a scarce decision packet, not a transcript. Target 1,000-4,000 input tokens and start with an 8,000-token hard cap. Preserve deciding anchors before background explanation. Do not attempt to fill a nominal 34k window; the applicable model and request limits can differ, and large packets can reduce instruction density.

Include only externally shareable task data. Never include credentials, private chain of thought, an entire conversation, or unrelated repository content.

## Apply the decision gate

Use the following outcomes:

- **Invalid:** the harness, packet parity, call count, model pin, or isolation gate failed. Repair the harness and start a new suite version.
- **Continue:** a small pilot shows a possible correction signal without corruption. Expand with independently sealed cases.
- **Accept optional routing:** replicated disputed-case correctness improves while corruption and unsupported claims stay within guardrails, and routing recall is operationally useful.
- **Reject broader routing:** treatment does not improve replicated disputed cases, corrupts correct answers, or adds unacceptable cost.

Do not accept default Jev routing from latency, confidence, or token savings alone. Final correctness and corruption are the decision metrics.

## Report the run

Every report includes:

- Goal, KPI, loop level, subject, and frozen harness version.
- Suite composition and route-class counts.
- Baseline and treatment host configuration.
- Integrity checks and invalid cases.
- Routing confusion matrix and macro-F1.
- Paired efficacy, corrections, corruptions, and unsupported claims.
- Exact Jev usage and unavailable telemetry.
- Limitations, verdict, and the next decision-changing experiment.

Keep the public suite, hidden gold, KPI contract, receipt, arms, envelopes, grade, and report immutable after grading.

## Related skill guidance

- Use [crossroads routing](../../../skills/octocode-jev-reasoning-loop/references/routing.md) to classify local research decisions.
- Use [research reasoning checks](../../../skills/octocode-jev-reasoning-loop/references/research.md) to build and verify the Jev packet.
- Use [context management](../../../skills/octocode-jev-reasoning-loop/references/context.md) to compact large packets.
- Use `octocode-eval-benchmark` to seal cases, define KPIs, grade held-out results, and validate the final report.
