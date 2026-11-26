Load when deciding whether to stream or return a full result.

# I/O: streaming vs full results

## Streaming vs full results

| Property | Full (buffered) result | Streamed or paged result |
|---|---|---|
| Memory | Grows with the result on both sides | Bounded by the chunk or page |
| First useful byte | After all work finishes | After the first chunk |
| Errors | Status reflects the whole result | Status is sent before later failures: report errors in band (final record, trailer, error event) and mark the stream complete or incomplete |
| Consistency | One snapshot | Each chunk may see newer data unless a snapshot, version or cursor pins it |
| Retry | Repeat the request | Resume from a cursor, offset, `Range` or last event id; restarting duplicates work |
| Consumer | Parses one document | Must handle partial lines, chunk boundaries and early termination |
| Backpressure | Not needed | Required end to end, or the fastest stage buffers everything |
| Caching, compression | Straightforward | Proxies and compression can buffer the whole stream unless configured not to |

Rules:

1. Stream when size is unbounded, a person or agent acts on early results, or the producer is slower than the consumer. Return whole when results are small, must be atomic, or every consumer needs all of them before acting.
2. A stream or page sequence must be lossless: prove no gaps and no duplicates through the last item, and give an explicit end marker so truncation is detectable. A silently short result is worse than an error.
3. Paginate with opaque cursors, not offsets ([API contracts](systems-api-contracts.md#api-and-interface-contracts)).
4. Keep the total or `hasMore` honest: say when a count is a floor.
5. Use line-delimited framing (NDJSON, SSE, length-prefixed records) so a consumer can process each record without the rest.
6. Honour the stream's backpressure: Node `write()` returning `false` means wait for `drain`; prefer `stream.pipeline()`, which also propagates errors and cleanup.
7. Cancel the producer when the consumer stops (client disconnect, `AbortSignal`, dropped receiver); otherwise abandoned streams keep consuming downstream capacity.
