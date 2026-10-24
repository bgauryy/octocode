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

A source names an allowed reader and its typed source query, or a supported HTTPS URL. Text questions may use supplied JSON content instead. Captured source bodies are not repeated in the answer; the emitted read continuation inspects the exact evidence.

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

A nonblank `OCTOCODE_CLASSIFICATION_API` or `classification.api` enables inline classification. Public clasify additionally requires beta (`OCTOCODE_BETA` or `local.beta`). Reader availability and storage policy still apply. Inspect live schema/config before using a capability.

## Limits

The generated contract and native constants own limits. The runtime bounds source bytes, question/input sizes, images, candidates, inference time and retained state. A limit must disclose unassessed work, a continuation or a terminal reason. See [configuration](CONFIGURATION.md#fixed-internal-limits) and [data contract](TOOL_DATA_CONTRACT.md#evidence-boundaries).

## Results and rating

Judge quality by agreement with source evidence and typed answers, complete coverage accounting, provider request counts, cache invalidation and executable inspection flows. Shorter output alone is not better. Preserve exact captured bytes and source identity when testing replay, mutation or access-policy changes.
