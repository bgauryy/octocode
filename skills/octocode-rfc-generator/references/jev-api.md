# `clasify` through Octocode (Jev provider)

Load when the `octocode-research` clasify gate admits an explicit classification request or experiment and request mechanics are needed. `clasify` is the public Octocode tool; Jev is the configured classification provider. The provider evaluates typed questions over supplied evidence, while the host owns review admission and every subsequent action.

## Availability and setup

Use the native Octocode CLI or discovered MCP tool. Inspect the live catalog and query schema first (inside this repository substitute `node packages/octocode/out/octocode.js` for `octocode`):

```sh
octocode scheme clasify --view query --compact
octocode clasify --input /absolute/review/request.json
```

There is no public `jev` command or schema alias. MCP registers `clasify` only when `OCTOCODE_CLASSIFICATION_API` is configured in the Octocode process environment. The CLI command remains discoverable, but an invocation without the key must fail with an actionable missing-`OCTOCODE_CLASSIFICATION_API` message. Keep credentials out of requests and receipts. A missing tool/key, denied access, timeout, or invalid output means no usable judgment: continue the unjudged audit and report the actual reason. Catalog or schema discovery alone is not provider validation.

## Public request contract

Submit one SemanticQuery directly or batch independent complete SemanticQueries as `{ "queries": [...] }`. Its shape is `{id?, goal, reasoning, resources:[{id?, context:{value}|{tool,query}, maxChars?}], questions:[{id?, type, instructions, criteria?}]}`; `goal` and `reasoning` are required (≤500 characters each) and are sent to the provider with every question. Questions are flat typed objects; omitted IDs are derived by position. The RFC debate uses native typed questions only, not research presets (`questionType`). Do not use a batch when one answer changes a later question.

Every question is assessed against every resource: `resources[] × questions[]`. IDs correlate query, resource, question, and page; they do not carry instructions. Keep the matrix at or below 25 cells. A `{tool, query}` context is one unexecuted ordinary read request; `{value}` is non-empty state already held by the host. The RFC debate preflight deliberately permits exactly one `{value}` resource so unreviewed retrieval cannot enter the frozen worker snapshot.

`maxChars` is at most 80,000 characters (file reads count content only). When a resource needs multiple pages, the runtime judges each ~600-line page against every question and returns every page explicitly with its line `scope`. Treat each page as a local judgment. There is no hidden averaging, voting, or whole-resource conclusion; partial pages cannot prove global absence or support.

## Native question types

| Type | Use and exact boundary |
|---|---|
| `noul` | One binary proposition; returns P(yes), 0–1. `instructions` must be non-null and non-empty. Optional `criteria` must contain both `true` and `false`; either value may be `null`. |
| `choice` | One of 2–255 declared alternatives with a probability distribution. `instructions` must be non-null and non-empty. Option labels are non-empty; option descriptions may be `null`. |
| `score` | One ordered dimension with 2–10 levels; returns the expected zero-based level and may be fractional. `instructions` and every criterion must be non-null and non-empty. |

Non-empty instructions and criteria may be strings, objects, or arrays. Include an `insufficient` choice when missing evidence must remain distinguishable from rejection, and `conflicting` when supplied evidence can support incompatible conclusions. Run exact lookups or executable checks directly instead of asking the provider.

For the explicit two-worker protocol, retain the full frozen `review` and `admission` in the host worker packet. Copy `review.goal` and `review.questions` unchanged only to outer SemanticQuery `goal` and `questions`. Project `review` without its goal and question array into `context.value.review`, alongside exact evidence entries, both argument rounds, and missing evidence; omit host admission policy from provider state. Preserve the meaning of each cited source section; avoid clipped prefixes and unrelated full files. Read deciding sources before freezing the worker packet. Skip the judge if the held claim is settled, facts are missing, or the result cannot change the next action.

Explicit API capability example with synthetic evidence; not a recommended research flow:

```json
{"id":"migration-review-1","goal":"Decide whether the frozen cutover proposal can advance under its criteria.","reasoning":"Resolve the frozen disagreement only if it changes the host action.","resources":[{"id":"debate","context":{"value":{"review":{"id":"migration-review-1","rfcRevision":"example-revision","criteria":["Preserve clients through cutover."],"subject":{"kind":"proposal","text":"Retain dual-read compatibility for one release and preserve a restore checkpoint."}},"evidence":[{"id":"E1","source":"example:sample@revision-1","observation":"No old-client requests appeared; offline clients were not covered."}],"arguments":{"A":{"opening":"E1 supports the prerequisite.","rebuttal":"Owner review can weigh the residual risk."},"B":{"opening":"E1 omits offline clients.","rebuttal":"Recovery does not close that gap."}},"missingEvidence":["Acceptable offline-client risk."]}}}],"questions":[{"id":"Q1","type":"choice","instructions":"Assess whether the resource review subject can advance under its criteria. Use its evidence, both arguments, and missing evidence. Do not infer owner approval.","criteria":{"support":"Safeguards satisfy the criteria.","reject":"A safeguard fails the criteria.","insufficient":null,"conflicting":"Evidence supports incompatible conclusions."}}]}
```

The host worker packet retains the full `review` including `goal` and `questions`, its `admission` gate, plus `evidence: {E1: {source, observation}}` copied from the resource evidence without `id`. Preserve optional evidence fields. Run `node scripts/validate-debate.mjs request.json worker-packet.json` before submission. The preflight compares outer goal, questions, and projected review with that packet, validates host admission without sending it, and checks argument rounds and evidence snapshot; it cannot establish source truth or prove worker coverage.

## Interpret and verify

Inspect `queries[]` → `resources[]` → every `pages[]` entry and its `answers.<questionId>`. Correlate by `queryId`, `resourceId`, and `questionId`; inspect resource coverage, page scope, limitations, errors, and typed answers. The agent response omits provider model and usage. A successful command with an errored or partial page is not complete coverage.

For answers, Noul near 0.5 means uncertainty. Choice `confidence` measures distribution concentration, not probability of correctness. Score is an expected ordered level, not a yes/no probability. Preserve raw typed provider answers, including distributions and provider fields; the host may add a separate disposition but must not rewrite the provider response into free-form reasoning.

Save the host's intended next action before the call. Record the raw answer separately from the host disposition. Neither confidence nor a favorable result verifies evidence, closes an RFC blocker, or records owner approval. Preserve both arguments. Do not retry a valid unfavorable judgment. Count transport failures, retries, and one clearly justified input repair against the review budget. Changed sources, questions, criteria, or RFC revision invalidate affected applicability. Apply the closure rules in `references/rfc-completeness.md`.
