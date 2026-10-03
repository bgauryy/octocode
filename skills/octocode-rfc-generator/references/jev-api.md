# `clasify` through Octocode (Jev provider)

Load when an admitted `clasify` review needs request mechanics. `clasify` is the public Octocode tool; Jev is the configured classification provider. The provider evaluates typed questions over supplied evidence; the host owns admission and every later action.

## Availability

Inspect the live catalog and query schema first. Inside this repository, substitute `node packages/octocode/out/octocode.js` for `octocode`.

```sh
octocode scheme clasify --view query --compact
octocode clasify --input /absolute/review/request.json
```

There is no public `jev` command or schema alias. MCP registers `clasify` only when `OCTOCODE_CLASSIFICATION_API` is set in the Octocode process environment. The CLI command stays discoverable; without the key it must fail with an actionable missing-`OCTOCODE_CLASSIFICATION_API` message. Keep credentials out of requests and receipts. A missing tool or key, denied access, timeout, or invalid output means no usable judgment: continue the unjudged audit and report the actual reason. Schema discovery alone is not provider validation.

## Request contract

- Submit one SemanticQuery, or batch independent complete queries as `{ "queries": [...] }`. Do not batch when one answer changes a later question.
- Shape: `{id?, goal, reasoning, resources:[{id?, context:{value}|{tool,query}, maxChars?}], questions:[{id?, type, instructions, criteria?}]}`. `goal` and `reasoning` are required, ≤500 characters each, and go to the provider with every question. Omitted IDs derive from position. The RFC debate uses native typed questions only, not research presets (`questionType`).
- Every question is assessed against every resource: `resources[] × questions[]`, at most 25 cells. IDs correlate; they carry no instructions. `{tool, query}` is one unexecuted read request; `{value}` is non-empty state the host already holds. The RFC preflight permits exactly one `{value}` resource.
- `maxChars` ≤ 80,000 (file reads count content only). A multi-page resource is judged per ~600-line page against every question; each page returns its line `scope`. No averaging or whole-resource conclusion; partial pages prove neither global absence nor support.

| Type | Use and exact boundary |
|---|---|
| `noul` | One binary proposition; returns P(yes), 0–1. `instructions` non-null and non-empty. Optional `criteria` contains both `true` and `false`; either may be `null`. |
| `choice` | One of 2–255 declared alternatives with a distribution. `instructions` non-null and non-empty. Labels non-empty; descriptions may be `null`. |
| `score` | One ordered dimension, 2–10 levels; returns the expected zero-based level, possibly fractional. `instructions` and every criterion non-null and non-empty. |

Instructions and criteria may be strings, objects, or arrays. Add an `insufficient` choice when missing evidence must stay distinct from rejection, and `conflicting` when evidence supports incompatible conclusions. Run exact lookups or executable checks directly instead of asking the provider.

## Project the worker packet

The host packet keeps the full frozen `review`, `admission`, and `evidence: {E1: {source, observation}}`. Copy `review.goal` and `review.questions` unchanged to outer `goal` and `questions` only. Put `review` without goal and questions into `resources[0].context.value.review`, beside the evidence array (same entries plus `id`), both argument rounds, and missing evidence. Never send admission to the provider. Keep cited sections whole; avoid clipped prefixes and unrelated full files. Run `node scripts/validate-debate.mjs request.json worker-packet.json` before submission.

Synthetic capability example, not a recommended research flow:

```json
{"id":"migration-review-1","goal":"Decide whether the frozen cutover proposal can advance under its criteria.","reasoning":"Resolve the frozen disagreement only if it changes the host action.","resources":[{"id":"debate","context":{"value":{"review":{"id":"migration-review-1","rfcRevision":"example-revision","criteria":["Preserve clients through cutover."],"subject":{"kind":"proposal","text":"Retain dual-read compatibility for one release and preserve a restore checkpoint."}},"evidence":[{"id":"E1","source":"example:sample@revision-1","observation":"No old-client requests appeared; offline clients were not covered."}],"arguments":{"A":{"opening":"E1 supports the prerequisite.","rebuttal":"Owner review can weigh the residual risk."},"B":{"opening":"E1 omits offline clients.","rebuttal":"Recovery does not close that gap."}},"missingEvidence":["Acceptable offline-client risk."]}}}],"questions":[{"id":"Q1","type":"choice","instructions":"Assess whether the resource review subject can advance under its criteria. Use its evidence, both arguments, and missing evidence. Do not infer owner approval.","criteria":{"support":"Safeguards satisfy the criteria.","reject":"A safeguard fails the criteria.","insufficient":null,"conflicting":"Evidence supports incompatible conclusions."}}]}
```

## Interpret

Inspect `queries[]` → `resources[]` → every `pages[]` entry and its `answers.<questionId>`. Correlate by `queryId`, `resourceId`, and `questionId`; check coverage, page scope, limitations, and errors. A successful command with an errored or partial page is not complete coverage. The agent response omits provider model and usage.

Noul near 0.5 means uncertainty. Choice `confidence` measures distribution concentration, not correctness. Score is an expected level, not a probability. Keep raw typed answers, including distributions; a host disposition is recorded separately and never rewrites them.

Save the intended next action before the call. Confidence or a favorable result never verifies evidence, closes a blocker, or records owner approval. Do not retry a valid unfavorable judgment. Count transport failures, retries, and one justified input repair against the budget. Changed sources, questions, criteria, or RFC revision invalidate affected applicability.

Next: apply closure rules in `references/rfc-completeness.md`.
