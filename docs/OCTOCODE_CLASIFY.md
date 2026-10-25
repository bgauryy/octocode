# JEV in Octocode

Classification produces typed judgments about supplied or internally captured evidence. It can select search/read results before the agent receives their bodies, or answer explicit questions through clasify.

## Choose a route once

Use an exact read when the target is small. Use inline `classification` on localSearch, localFetch, ghSearchCode or ghGetFileContent when selecting a large retrieved set helps. Use clasify for typed questions over content, an unread source, image assets or sections of several resources.

## Shared mechanism

The runtime captures and sanitizes source, applies bounded inference, caches eligible judgments and returns provenance and usage. A cache hit does not repeat inference. Changing evidence or question context changes the judgment identity. Provider failures preserve source inspection when captured evidence exists.

## JEV API support

Question types include `noul`, `choice`, `score` and `box`. Their typed request/answer shapes come from live schema discovery. `operation:"models"` discovers provider models. `model`, `context` and `longContext` are explicit request fields where advertised; the adapter owns provider wire translation.

## clasify: scout unread resources

All operations use the shared queries envelope. Evaluate either `content` or `source`:

```json
{"queries":[{"operation":"evaluate","source":{"tool":"localFetch","query":{"path":"src/cache.ts","ranges":[{"startLine":1,"endLine":80}]}},"questions":{"restart":{"type":"noul","instructions":"Does this source require restarting after credential rotation?"}}}]}
```

A source names an allowed reader and its typed source query, or a supported HTTPS URL. Use `source` to scout an unread resource; paths or URLs inside `content` are literal data and are not fetched. Several keyed questions share the captured resource. Text questions may also use supplied JSON content directly, without a source or an agent-specific wrapper.

For GitHub, use `source:{tool:"ghGetFileContent",query:{owner,repo,path,ref}}` with discovered values. Native uses the same authorized GitHub reader and cache as an ordinary file read, pins the capture to its resolved commit and sends the selected source and questions to JEV internally. The agent receives typed answers, usage and an executable inspection handle, not an automatic copy of the resource body. Source limits fail explicitly instead of judging a silently clipped prefix.

A label-only task may finish with the answers. Follow the captured-source read only when you need to inspect the evidence or quote it; it preserves the judged selection and rejects a changed local source instead of silently reading its new contents. Complete any remaining answer or assessment pages needed for your task. An uncertain judgment is not proof that the source lacks something, and a classifier's answer does not establish symbol identity.

### Research text sections across resources

```json
{"queries":[{"operation":"sections","resources":{"cache":{"tool":"localFetch","query":{"path":"src/cache.ts"}}},"questions":{"restart":{"type":"noul","instructions":"Does the target require restarting after credential rotation?"}}}]}
```

Sections are planned from captured text. Each resource reports its status, source identity, section evidence and coverage. Follow assessment continuations until the requested scope is complete. Frozen plans do not repeat completed inference on replay. A resource that cannot be read or assessed reports that fact without inventing answers for it.

### Unread images

An image asset supplies a source reference, not host-provided binary data. Reference the asset key from content or questions. The provider receives admitted PNG/JPEG bytes; the result carries dimensions, format, a snapshot and an inspection path for the captured bytes. Local path policy, size bounds and HTTPS network policy apply before inference. Persistent storage is required for retained image evidence.

## Classifiers on search and fetch

```json
{"queries":[{"path":"src","matchString":["cache"],"classification":{"predicates":["Does this implement credential invalidation?"],"context":"Trace credential rotation through the cache."}}]}
```

The same `classification` object works on the four supported tools. Predicates are independent questions about admitted candidates. The selection receipt counts selected, related, excluded and unknown units in its scope. Unknown units remain unknown; a partial selected view does not prove corpus absence.

Complete selected-view pages for selected evidence. Use emitted unfiltered or inspection continuations when the claim needs omitted bodies. Transient provider unavailability can return unfiltered evidence with a disclosure; refused credentials and invalid provider answers remain typed failures.

## Questions and recovery

A question must use its declared type and shape. Input errors identify the rejected field. Output errors and partial receipts distinguish source, admission, provider and assessment failures. `next.*` carries saved work; `related[]` starts a fresh source or discovery request. Execute each input unchanged. Prose hints contain no encoded tool calls.

## Availability

A nonblank `OCTOCODE_CLASSIFICATION_API` or `classification.api` enables inline classification. Public clasify also requires a successful authenticated `GET /v1/models` ping at host startup, with one attempt and a two-second deadline. It does not depend on `OCTOCODE_BETA` or `local.beta`. A failed ping omits clasify from the enabled CLI tool listing and MCP discovery and blocks execution until a new runtime starts. Its named CLI schema remains inspectable and reports disabled availability. Explicit tool allowlists and exclusions still apply. CLI `--help` uses configuration only and does not contact the provider. Reader availability and storage policy still apply. Inspect live schema/config before using a capability.

## Limits

The generated contract and native constants own limits. The runtime bounds source bytes, question/input sizes, images, candidates, inference time and retained state. A limit must disclose unassessed work, a continuation or a terminal reason. See [configuration](CONFIGURATION.md#fixed-internal-limits) and [data contract](TOOL_DATA_CONTRACT.md#evidence-boundaries).

## Results and rating

Judge quality by agreement with source evidence and typed answers, complete coverage accounting, provider request counts, cache invalidation and executable inspection flows. Shorter output alone is not better. Preserve exact captured bytes and source identity when testing replay, mutation or access-policy changes.

### Measured scorecard

Scores are out of 10, from evaluation runs against known answers.

| Area | Score | Evidence |
|---|---|---|
| Image classification | 9 | 73/77 correct. Every miss came with low confidence (0.51 or less) |
| Text classification | 8.5 | 83/90 correct. Its few confident wrong answers came from vague or broad questions |
| Scanning many files (sections) | 8 | 100/100 correct per file (25 files × 4 questions). Single sections next to a real hit can be false hits |
| Scouting (cutting what the agent reads) | 9.5 | 84–86% less output, with no relevant hits lost in the tests |
| Large sources | 9.5 | A 200k-character log came back as about 1 KB of answers, 5/5 correct |
| Confidence you can trust | 8 | Reliable for images; for text, only when the question names a specific thing |
| Caching | 7 | Repeats cost nothing, but a file edit can get the old answer back |
| Ease of use | 7 | Strict limits, no image resizing, confusing errors when you pass unexpected arguments |

Best at:

- Answering specific questions about large sources you haven't read
- Narrowing searches down to the hits that matter
- Reading screenshots, charts and error popups
- Spotting obvious design flaws
- Triaging incidents from logs

Weak at:

- Counting, totals and finding the maximum: use a script
- Pixel-level layout: use DOM measurements
- Broad questions like "any bug?": ask one exact, named fact per question
- Re-checking a file right after editing it: an identical query can replay the saved answer, so rephrase the question to re-judge

Treat answers between 0.3 and 0.7 as unsure, and read the lines or image region behind a finding before acting on it. A sections hit is a candidate: confirm it at its target lines.
