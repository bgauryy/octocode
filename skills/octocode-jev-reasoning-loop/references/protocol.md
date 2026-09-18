# Jev protocol

Load when building a request or handling a response. Why: Jev uses typed evaluation, not chat completions, generated tool calls or computer-control commands.

## Check the live contract

Before writing or updating an integration, use the [documentation index](https://docs.typesafe.ai/llms.txt) to find the current API or chosen SDK page, the selected primitive, confidence guidance and the closest cookbook. Read only the pages needed for the design. Treat this reference as a checked snapshot, not a substitute for current provider documentation. If live docs are unavailable, inspect installed SDK types or this snapshot, state the limitation and avoid inventing version-dependent fields, models or limits.

## Wire contract

Authenticated POST to `https://api.typesafe.ai/v1/systemone` with `Content-Type: application/json` and `Authorization: Bearer <key>`. Send `{ "model": "jev-latest", "state": ..., "questions": { "id": ... } }`. GET `/v1/models` uses the same auth and returns a `models` array with `name`, `description`, and `release_date` strings.

`state` is text, a JSON object/array, or null. Current official SDK types also permit object/array/null descriptions, optional instructions and null Noul criteria; the HTTP narrative shows a narrower text-centric form. The client supports the SDK's entry types. Supply explicit, meaningful instructions in agent-authored questions. Numbers and booleans can appear inside structured entries, but are not supported as entire entries.

| Question | Criteria | Answer fields, alongside matching `type` |
| --- | --- | --- |
| `choice` | 1–255 named labels mapped to descriptions or null | `choice`, `probabilities` keyed by every label, `confidence` |
| `score` | Ordered array with at least two descriptions | `score`, `legend` keyed by zero-based index strings, `probabilities`, `confidence` |
| `noul` | Optional object with `true` and/or `false` descriptions, or null | `noul`, a yes-probability in [0,1] |

Response shape: `{ "model": "<resolved-id>", "answers": { "id": ... }, "usage": { "input_tokens": 0, "output_tokens": 0 } }`. Question IDs route answers but are excluded from inference. Each answer is independent; there is no cross-question reasoning within a request. Preserve needed history explicitly inside state. See the `examples` array inside `assets/hypothesis-triage.schema.json` for a mixed Choice + Score request template.

## Meaning and validation

- Choice picks a highest-probability label from its criteria. Probability mass covers all options. Add a real unknown/not-applicable candidate when forcing a winner would misrepresent the task.
- Score is an expectation over ordered level indices, so it can be fractional. It is not exact numeric extraction. Keep arithmetic and date comparisons in code.
- Choice/Score confidence summarizes distribution shape; it is not necessarily the chosen probability. Noul has no separate confidence. Retain probabilities when applying a statistical decision rule.
- The CLI validates answer IDs, types, numbers, candidate membership, complete distributions, score legends and expected score before emitting success. It allows 0.02 probability-sum error and 2% of score range for rounded responses. A type-correct answer can still be semantically wrong.

## Limits and versions

Jev consumes text/JSON; no image/audio/video input or documented streaming/session protocol. The model-specific guidance, reviewed September 16 and rechecked September 18, 2026, states 64k tokens across state plus all questions, and 32k for state plus the longest question. A nominal “34k window” is not a verified safe request size. Some general primitive docs still describe a ~32k total budget; use the stricter applicable budget when uncertain and confirm provider acceptance. The client's 4 MiB input/output cap is a separate memory bound, not a token estimator. For packet compaction and headroom, load `references/context.md`.

At inspection on September 18, 2026, `jev-latest` and `jev-preview` resolve to `jev-1.13.0`. Aliases move; record returned `model`, and pin a version when calibrating a workflow. Listing models may show only aliases even though versioned IDs are accepted.

## Recovery

| Exit | Meaning | Next action |
| --- | --- | --- |
| 0 | Validated JSON emitted | Inspect decisions and apply caller-owned policy |
| 2 | Input/configuration failure | Repair JSON, options, key or configuration |
| 3 | HTTP/transport failure | Use the reported status; no decision is available |
| 4 | Invalid, unreadable or oversized response | Keep downstream actions stopped; inspect provider/version behavior |
| 5 | Output write failed | Repair the pipe/destination before reuse |

HTTP 401/403: repair key/account access. HTTP 400/422: inspect request, model and context limits. HTTP 429/503/529: the native client defaults to two retries; the launcher uses Octocode's shared configuration (three by default). Backoff starts at 500ms and doubles; `Retry-After` seconds/date and `retry-after-ms` override the delay. If the requested wait would exceed the total deadline, it exits instead of retrying early. Other statuses, redirects and transport failures do not retry automatically. The total deadline covers connection, response reading, attempts and waits. Server error bodies are omitted to prevent private request data or secrets being echoed. Tune `--retries` and `--timeout-ms` explicitly for a different budget.

For composition and browser-specific integration, load `references/patterns.md`; for source disagreements or updates, load `references/references.md`.
