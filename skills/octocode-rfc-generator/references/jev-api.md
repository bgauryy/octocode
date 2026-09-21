# `clasify` through Octocode (Jev provider)

Load when checking availability, constructing the judge request, or interpreting its result. `clasify` is the public Octocode tool; Jev is the configured classification provider. The provider evaluates typed questions over supplied evidence, while the host owns review admission and every subsequent action.

## Availability and setup

Use the native Octocode CLI or discovered MCP tool. Inspect the live catalog and query schema first (inside this repository substitute `node packages/octocode/out/octocode.js` for `octocode`):

```sh
octocode scheme clasify --view query --compact
octocode clasify --input /absolute/review/request.json --compact
```

There is no public `jev` command or schema alias. MCP registers `clasify` only when `OCTOCODE_CLASSIFICATION_API` is configured in the Octocode process environment. The CLI command remains discoverable, but an invocation without the key must fail with an actionable missing-`OCTOCODE_CLASSIFICATION_API` message. Keep credentials out of requests and receipts. A missing tool/key, denied access, timeout, or invalid output means no usable judgment: continue the unjudged audit and report the actual reason. Catalog or schema discovery alone is not provider validation.

## Public request contract

Submit one SemanticQuery directly or batch independent complete SemanticQueries as `{ "queries": [...] }`. Its shape is `{id?, reasoning?, resources:[{id, context:{value}|{tool,query}, maxChars?}], questions:[{id, question:{type,instructions,criteria?}}]}`. Do not use a batch when one answer changes a later question.

Every question is assessed against every resource: `resources[] × questions[]`. IDs correlate query, resource, question, and page; they do not carry instructions. Keep the matrix at or below 25 cells. A `{tool, query}` context is one unexecuted ordinary read request; `{value}` is non-empty state already held by the host. The RFC debate preflight deliberately permits exactly one `{value}` resource so unreviewed retrieval cannot enter the frozen worker snapshot.

`maxChars` is at most 80,000 characters. When a resource needs multiple pages, the runtime assesses the same resource/question cell page by page and returns every page explicitly. Treat each page as a local judgment. There is no hidden averaging, voting, or whole-resource conclusion; partial pages cannot prove global absence or support.

## Native question types

| Type | Use and exact boundary |
|---|---|
| `noul` | One binary proposition; returns P(yes), 0–1. `instructions` must be non-null and non-empty. Optional `criteria` must contain both `true` and `false`; either value may be `null`. |
| `choice` | One of 2–255 declared alternatives with a probability distribution. `instructions` must be non-null and non-empty. Option labels are non-empty; option descriptions may be `null`. |
| `score` | One ordered dimension with 2–10 levels; returns the expected zero-based level and may be fractional. `instructions` and every criterion must be non-null and non-empty. |

Non-empty instructions and criteria may be strings, objects, or arrays. Include an `insufficient` choice when missing evidence must remain distinguishable from rejection, and `conflicting` when supplied evidence can support incompatible conclusions. Run exact lookups or executable checks directly instead of asking the provider.

For a two-worker review, freeze typed question entries in `review.questions` and copy them unchanged to the SemanticQuery `questions`. Put the frozen `review`, host `admission`, exact evidence entries, both argument rounds, and missing evidence in one `context.value` resource. Read deciding sources before freezing the worker packet.

Illustrative request with synthetic evidence:

```json
{"id":"migration-review-1","reasoning":"Resolve the frozen disagreement only if it changes the host action.","resources":[{"id":"debate","context":{"value":{"review":{"id":"migration-review-1","rfcRevision":"example-revision","questions":[{"id":"Q1","question":{"type":"choice","instructions":"Assess whether the resource review subject can advance under its criteria. Use its evidence, both arguments, and missing evidence. Do not infer owner approval.","criteria":{"support":"Safeguards satisfy the criteria.","reject":"A safeguard fails the criteria.","insufficient":null,"conflicting":"Evidence supports incompatible conclusions."}}}],"criteria":["Preserve clients through cutover."],"subject":{"kind":"proposal","text":"Retain dual-read compatibility for one release and preserve a restore checkpoint."}},"admission":{"workersDisagree":true,"remainingDisagreement":"Whether safeguards bound residual risk.","evidenceDoesNotSettleBecause":"Observed traffic omits offline clients.","directCheckUnavailableBecause":"No exact check settles the risk tradeoff.","currentAction":"Hold for review.","ifJudgeSupports":"Advance to owner review.","ifJudgeRejects":"Add another safeguard.","workerPositions":{"A":"Safeguards are sufficient.","B":"Another safeguard is required."},"willChangeAction":true,"directCheck":{"available":false},"evidenceFresh":true,"clasifyCallsAtCrossroad":0},"evidence":[{"id":"E1","source":"example:sample@revision-1","observation":"No old-client requests appeared; offline clients were not covered."}],"arguments":{"A":{"opening":"E1 supports the prerequisite.","rebuttal":"Owner review can weigh the residual risk."},"B":{"opening":"E1 omits offline clients.","rebuttal":"Recovery does not close that gap."}},"missingEvidence":["Acceptable offline-client risk."]}}}],"questions":[{"id":"Q1","question":{"type":"choice","instructions":"Assess whether the resource review subject can advance under its criteria. Use its evidence, both arguments, and missing evidence. Do not infer owner approval.","criteria":{"support":"Safeguards satisfy the criteria.","reject":"A safeguard fails the criteria.","insufficient":null,"conflicting":"Evidence supports incompatible conclusions."}}}]}
```

The worker packet contains the exact resource `review` and `admission` objects, plus `evidence: {E1: {source, observation}}` copied from the resource evidence without `id`. Preserve optional evidence fields. Run `node scripts/validate-debate.mjs request.json worker-packet.json` before submission. The preflight binds questions, subject, admission, argument rounds, and evidence snapshot; it cannot establish source truth or prove worker coverage.

## Interpret and verify

Inspect `queries[]`, then every resource/question result and every `pages[]` entry. Correlate by `queryId`, `resourceId`, and `questionId`; inspect page coverage, limitations, errors, typed answer, usage, `requestedModel`, and provider `resolvedModel`. Never report the configured/requested model as the model that actually answered. A successful command with an errored or partial page is not complete coverage.

For answers, Noul near 0.5 means uncertainty. Choice `confidence` measures distribution concentration, not probability of correctness. Score is an expected ordered level, not a yes/no probability. Preserve raw typed provider answers, including distributions and provider fields; the host may add a separate disposition but must not rewrite the provider response into free-form reasoning.

Save the host's intended next action before the call. Record the raw answer separately from the host disposition. Neither confidence nor a favorable result verifies evidence, closes an RFC blocker, or records owner approval. Preserve both arguments. Do not retry a valid unfavorable judgment. Count transport failures, retries, and one clearly justified input repair against the review budget. Changed sources, questions, criteria, or RFC revision invalidate affected applicability. Apply the closure rules in `references/rfc-completeness.md`.
