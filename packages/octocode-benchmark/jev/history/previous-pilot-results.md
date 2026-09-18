# Octocode Jev local-research benchmark results

## Goal

Measure Jev routing and treatment efficacy in the local Octocode research flow, then test whether the result replicates under fresh Terra host contexts.

## KPI

- Primary routing KPI: macro-F1 over `deterministic`, `missing_fact`, and `disputed_inference`.
- Primary efficacy KPI: paired final-correctness delta between host-only and Jev-treated answers.
- Guardrails: correction rate, corruption rate, unsupported claims, packet parity, exactly one treatment call, pinned model, and valid bound envelopes.

## Primary metric

The two runs disagree at the final-answer layer. Run 1 improved from baseline 2/4 to treatment 4/4. Replication 2 changed from baseline 3/4 to treatment 0/4. Across the repeated eight pairs, baseline final correctness was 5/8 and treatment final correctness was 4/8, a descriptive delta of -0.125.

Jev's raw typed decisions did not disagree: both runs returned the same correct status and evidence-basis selection on all four cases. The observed variance comes from the host's translation of typed advice into the required final conclusion.

## Loop level and budget

Experiment and replication over one sealed four-pair efficacy suite plus eight routing cases. Each treatment case allowed exactly one remote `jev-1.13.0` call, zero retries, one local bound-envelope check, and no post-freeze evidence retrieval.

This remains a report-only pilot. Repeated runs over the same four cases do not create eight independent cases.

## Subject and frozen harness

The benchmark used `jev-split-terra-v2-20260918` without changing its public suite, hidden gold, KPI contract, grader rules, or request template between runs. Before replication:

- The public suite, hidden gold, KPI contract, and preflight hashes matched their sealed receipt.
- All four materialized treatment requests passed local client validation.
- The preflight made zero remote calls.
- Baseline and treatment used byte-identical evidence packets within each pair.

Fresh `gpt-5.6-terra` agents produced each baseline and treatment artifact. Fresh Terra graders received hidden gold only after both arms were frozen.

## Results by run

| Metric | Run 1 | Replication 2 |
| --- | ---: | ---: |
| Routing macro-F1 | 0.778 | 0.886 |
| Disputed-inference recall | 0.500 | 0.750 |
| Baseline final correctness | 2/4 | 3/4 |
| Treatment final correctness | 4/4 | 0/4 |
| Paired delta | +0.500 | -0.750 |
| Corrections | 2 | 0 |
| Corruptions | 0 | 3 |
| Unsupported claims | 0 | 0 |
| Valid Jev calls | 4/4 | 4/4 |

The route classifier improved in the replication but still missed one hidden disputed inference by calling it deterministic.

## Descriptive combined view

The following totals summarize two executions of the same four cases. They are not an independent eight-case sample.

| Metric | Combined result |
| --- | ---: |
| Baseline correct | 5/8 (62.5%) |
| Treatment correct | 4/8 (50.0%) |
| Descriptive treatment delta | -12.5 percentage points |
| Baseline-wrong cases corrected | 2/3 |
| Baseline-correct cases corrupted | 3/5 |
| Mean routing macro-F1 | 0.832 |
| Mean disputed-inference recall | 0.625 |
| Unsupported claims | 0 |

Do not use the combined delta as a product estimate. The same cases and hidden rules appear in both runs.

## Jev decision consistency

Jev returned the same typed decisions in both runs:

| Case | Claim status | Decisive basis | Hidden judgment |
| --- | --- | --- | --- |
| JE-01 | `contradicted` | `B-eval` | Correct |
| JE-02 | `contradicted` | `B-paired` | Correct |
| JE-03 | `contradicted` | `B-host` | Correct |
| JE-04 | `contradicted` | `B-local` | Correct |

Raw typed status-and-basis accuracy was therefore 4/4 in each run and 8/8 across the two executions. This is component evidence only; the host still owns source verification and final composition.

## Host application failure

Run 1 converted each typed result into a scoped conclusion containing the required semantic content. Replication 2 emitted the generic sentence `The supplied packet contradicts the scoped claim` for every treatment case.

Those generic conclusions retained most selected anchors but omitted required case-specific meaning. The hidden grader therefore scored all four replication treatment answers incorrect. This is a host application failure, not a change in Jev's selected status or basis.

The benchmark measures the whole product path:

```text
fixed evidence packet
  → Jev status and basis
  → host verifies selected evidence
  → host composes final conclusion
  → hidden semantic grader
```

Run 1 passed the final two stages. Replication 2 passed the Jev stage but failed final composition.

## Jev usage

| Usage | Run 1 | Replication 2 | Total |
| --- | ---: | ---: | ---: |
| Remote calls | 4 | 4 | 8 |
| Input tokens | 3,473 | 3,473 | 6,946 |
| Output tokens | 402 | 402 | 804 |
| Summed call latency | 3,823 ms | 3,911 ms | 7,734 ms |
| Mean latency per call | 956 ms | 978 ms | 967 ms |

All eight calls used pinned `jev-1.13.0`, zero retries, and one successful local envelope check per response.

## Checks run

- Recomputed the sealed suite hashes before replication.
- Executed all four curator-only request dry runs with zero remote calls.
- Parsed both replication arm artifacts and all saved request/envelope JSON.
- Verified packet, request, and response hashes, plus byte counts.
- Verified four calls per treatment run, one per case, pinned model, exit zero, zero retries, and successful local checks.
- Applied hidden route labels, semantic atoms, anchor sets, and unsupported-claim traps through fresh graders.
- Independently inspected the treatment conclusions after the surprising replication result.
- Preserved both positive and negative runs without editing cases or graders.

## Measurement limitations

1. The suite has only four efficacy cases and focuses on Jev skill-policy precedence rather than broad repository investigations.
2. The replication reuses the same sealed cases, so it measures execution stability, not new-case generalization.
3. Exact Terra host prompt and completion tokens were unavailable or inconsistently represented. Run 2 also omitted auditable baseline host-model and sampling fields.
4. Process-level logs cannot independently prove the absence of unrecorded tools or calls; artifacts and envelopes support protocol compliance.
5. The grader combines Jev advice quality with host composition quality. That is appropriate for product behavior but obscures component attribution unless the report includes both scores.

## Verdict

**CONTINUE, but do not accept a product benefit claim.** Jev produced stable and correct typed judgments on this small suite, but the full Octocode flow did not apply those judgments reliably. The original positive lift did not replicate at the final-answer layer.

We recommend targeting the host application contract next, not the Jev question packet: require a case-specific conclusion that states the resolved claim, selected evidence basis, and verified implication. Grade typed advice and final composition as separate nodes while retaining final correctness as the product KPI.

## Next

1. Add an application-fidelity check between the Jev envelope and the final answer.
2. Require the host to restate the scoped claim outcome and explain the selected basis in one concise, source-verified sentence.
3. Record host model, sampling configuration, and token-telemetry availability consistently.
4. Seal and run a new suite with at least 30 disputed-inference pairs across repository behavior, architecture, conflicting documentation, and source-version disagreements.
5. Accept optional routing only if final correctness improves across new cases without increasing corruption or unsupported claims.
