# Jev API through Octocode

Load when checking availability, constructing the judge request, or interpreting its result. Jev evaluates typed questions over supplied evidence; the host owns review admission and subsequent action.

## Available transport and setup

Use the native Octocode CLI or equivalent discovered MCP tool. Inspect `octocode tools --json`, then the query schema (in this repository substitute `node packages/octocode/out/octocode.js` for `octocode`):

```sh
octocode tools jev --scheme --scheme-view query --json --compact
octocode tools jev --input /absolute/review/request.json --json --compact
```

`--input` takes a JSON file. Jev queries require `state` and `questions` and optionally accept `sources`; the CLI also accepts its standard `queries` envelope. This RFC review uses a source-free query so every judgment sees the same inspected evidence as both workers. The native adapter owns transport and typed response validation. Use the configured Octocode environment for `OCTOCODE_JEV_KEY`; keep credentials out of requests and receipts. Internal configuration selects the model; queries do not supply it. Missing tool/key, denied access, timeout, or invalid output means no usable judgment: continue the unjudged audit and report the actual reason. A catalog or schema read alone never counts as provider validation.

## Construct the judgment

| Unresolved object | Host preparation and typed question |
|---|---|
| Exact fact or cheap executable discriminator | Run the lookup or test directly. |
| One independent condition | Put its evidence and coverage in `state`; ask a complete `noul` question naming the relevant fields. |
| Disputed factual or causal claim | Supply the claim and supporting/contrary observations; use `choice` with supported, contradicted, insufficient, and conflicting alternatives. |
| Proposed design, migration, rollout, or readiness | Supply the exact proposal, assumptions, risks, costs, reversibility, and owner criteria; ask bounded `choice` questions or a `score` question about one ordered dimension. |
| Owner preference or absent evidence | Ask the owner or collect evidence before judging. |

Every question requires `type` and `instructions`. `noul` returns P(yes), 0–1; optional `criteria` defines `true` and `false`. `choice` requires a map of 1–255 labeled alternatives. `score` requires 2–10 ordered levels and returns the expected zero-based level, which can be fractional. Instructions and individual criteria entries can be strings, objects, arrays, or null; preserve useful structure. Question IDs only identify answers. Put the complete question and relevant state fields in `instructions`; include an insufficient-evidence alternative when needed. Batch only independent questions; a question that needs another answer belongs in a later call.

For a two-worker review, freeze typed questions in `review.questions` and copy them unchanged to the query's `questions`. Put the frozen `review`, host `admission`, exact evidence entries, both argument rounds, and missing evidence in `state`. Strings and paths inside `state` remain literal evidence. Optional top-level `sources` can hydrate content in other Jev workflows; this review preflight excludes them so unreviewed content cannot enter the judgment. Read deciding sources before freezing the worker packet.

Illustrative request with synthetic evidence; replace the review, evidence, and arguments with the actual frozen worker packet before a real submission:

```json
{"state":{
  "review":{"id":"migration-review-1","rfcRevision":"example-revision","questions":{"Q1":{"type":"choice","instructions":"Assess whether state.review.subject.text can advance to owner review under state.review.criteria. Use state.evidence, both sides of state.arguments, and state.missingEvidence. Do not infer owner approval from either argument.","criteria":{"support":"The supplied safeguards satisfy the review criteria well enough to advance to owner review.","reject":"The supplied evidence shows a safeguard fails the review criteria.","insufficient":"Missing evidence or owner criteria prevent a conclusion.","conflicting":"Relevant evidence supports incompatible conclusions that the packet cannot resolve."}}},"criteria":["Preserve existing clients through cutover."],"subject":{"kind":"proposal","text":"Retain dual-read compatibility for one release, require zero observed old-client traffic before cutover, and preserve a restore checkpoint."}},
  "admission":{"workersDisagree":true,"remainingDisagreement":"Whether the proposed safeguards sufficiently bound residual compatibility risk.","evidenceDoesNotSettleBecause":"Observed traffic does not establish the risk from unobserved clients.","directCheckUnavailableBecause":"No available exact check establishes the remaining risk tradeoff.","currentAction":"Hold for review.","ifJudgeSupports":"Advance to owner review with residual risk recorded.","ifJudgeRejects":"Add another compatibility safeguard before owner review.","workerPositions":{"A":"The existing safeguards are sufficient for owner review.","B":"Residual compatibility risk requires another safeguard."},"willChangeAction":true,"directCheck":{"available":false},"evidenceFresh":true,"jevCallsAtCrossroad":0},
  "evidence":[{"id":"E1","source":"example:traffic-sample@revision-1","observation":"The sampled traffic contains no old-client requests; the sample does not cover offline clients."}],
  "arguments":{"A":{"opening":"E1 supports the traffic prerequisite; the restore checkpoint bounds recovery cost.","rebuttal":"E1 omits offline clients, but owner review can weigh that residual risk."},"B":{"opening":"E1 cannot establish that offline clients will remain compatible.","rebuttal":"The restore checkpoint helps recovery but does not resolve the coverage gap in E1."}},"missingEvidence":["Acceptable residual risk for offline clients has not been established by the owner."]},
"questions":{"Q1":{"type":"choice","instructions":"Assess whether state.review.subject.text can advance to owner review under state.review.criteria. Use state.evidence, both sides of state.arguments, and state.missingEvidence. Do not infer owner approval from either argument.","criteria":{"support":"The supplied safeguards satisfy the review criteria well enough to advance to owner review.","reject":"The supplied evidence shows a safeguard fails the review criteria.","insufficient":"Missing evidence or owner criteria prevent a conclusion.","conflicting":"Relevant evidence supports incompatible conclusions that the packet cannot resolve."}}}}
```

The matching worker packet contains the exact `state.review` and `state.admission` objects, plus `evidence: {E1: {source, observation}}` copied from `state.evidence` without the `id` field. Preserve any optional evidence fields too. Run `node scripts/validate-debate.mjs request.json worker-packet.json` from this skill folder before submission. The preflight binds the typed questions, subject, admission, argument rounds, and evidence snapshot; it cannot establish source truth or prove the workers reviewed the material.

`state.admission` records host policy, not Jev execution controls. Admit a call only when workers still disagree, evidence is fresh, no direct check settles the question, no call has judged the unchanged crossroad, and support versus rejection changes the frozen next action. Converged workers, settled evidence, or exhausted budget stop submission in the host. An uncertain judgment leaves the question open.

## Interpret and verify

Inspect each result for errors and its typed answers, model, and usage; CLI success alone is insufficient. Use the live output schema for exact response fields. `noul` near 0.5 means uncertainty; a `score` is an expected ordered level, not a yes/no probability. Choice labels retain the distinctions supplied in their criteria. The source-free review request returns judgments without additional retrieval or actions; the host decides what to check or do next.

Record the raw answer separately from the host disposition. Neither confidence nor a favorable result verifies evidence, resolves an RFC blocker, or records owner approval. Preserve both arguments. Do not retry a valid unfavorable judgment. Count transport failures, retries, and one clearly justified input repair against the review budget; an input repair is not new evidence. Changed sources, questions, criteria, or RFC revision invalidate affected applicability. Apply the host closure rules in `references/rfc-completeness.md`.
