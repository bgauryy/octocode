# Protocol Basics: framing, messages, errors, cancel

Load when touching the JSON-RPC transport: reading or writing frames, matching responses, error handling, or cancellation. Why: a mis-framed message or a lost id doesn't raise an error. It just leaves the connection hanging until something times out.

## Framing (LSP 3.17 base protocol)
- A frame is a header, a blank line, then the body: `Content-Length: N\r\n` (with an optional `Content-Type`), `\r\n`, then exactly N **bytes** of UTF-8 JSON. N counts bytes, not characters.
- Parse header names case-insensitively and ignore headers you don't recognize. Read the body with `read_exact(N)`.
- **Check every size limit before allocating.** That covers the header line, the whole header block, and the body. If a limit is exceeded, mark the connection failed and fail every pending request. Never just `return` from the reader.
- Skip a body that isn't valid JSON, but count it or log it. A spike of skipped bodies means the stream has lost its framing.

## Message kinds

| Kind | Shape | Client duty |
|---|---|---|
| Request | `id` + `method` | Match the response **by id**. Servers may answer out of order, because pipelining is legal. |
| Response | `id` + `result` or `error` | Resolve the pending slot. Accept both integer and string ids. |
| Notification | `method`, no `id` | No reply. A `$/…` notification may be ignored. |
| Server→client request | `id` + `method` | **Always reply** with a result or an error, and echo the id unchanged. Reply to an unknown `$/…` request with MethodNotFound. |

## Error codes that change behavior

| Code | Name | Client action |
|---|---|---|
| -32801 | ContentModified | Retry with a small backoff |
| -32802 | ServerCancelled | Retry only if `data.retriggerRequest` is set (pull diagnostics) |
| -32800 | RequestCancelled | Expected after our own `$/cancelRequest`. Not a failure. |
| -32803 | RequestFailed | Report it. Don't retry blindly. |
| -32002 | ServerNotInitialized | Client bug: something was sent before the `initialize` response |
| -32601 | MethodNotFound | Send this for unknown server requests. Receiving it means our capability gating failed. |

## Cancellation
- `$/cancelRequest {id}` is a notification. The server **still sends a response**, usually -32800. Keep the pending slot until that response arrives or a short grace period expires. Ignore late responses whose ids we already dropped.
- **Cancel when the future is dropped, not only on timeout.** Zed arms a guard at request creation that sends `$/cancelRequest` and removes the pending entry, and disarms it when the response arrives. It holds only a `Weak` reference to the writer, so a guard that outlives the server does nothing. Helix and async-lsp don't do this, and their pending entries leak until the server answers.
- **Writes must be atomic under cancellation.** If the future is dropped halfway through `write_all`, a partial frame is left on the pipe and every later message is corrupted. Either hand whole frames to a single writer task over a channel, or mark the connection failed when a write future is dropped before it finishes.
- **Timeout policy is a trade-off.** Poisoning the whole connection is safe against a wedged server, but one slow `workspace/symbol` then costs a warm server. Cancelling only that request keeps the server, but it trusts the server to recover. Document which one you chose.

## Backpressure
- Bound the inbound queue: Zed uses 128 messages. When the queue is full, stop reading stdout and let the OS pipe push back on the server. rust-analyzer's `lsp-server` goes further and uses rendezvous (`bounded(0)`) channels.
- The reader must never block on a write. A reply to a server→client request goes through the writer queue with a deadline. If it is written inline from the read loop, a server that stops reading stdin while it writes stdout deadlocks both sides.

Next: for initialize, shutdown, and the replies the client owes the server, load `references/lifecycle-and-server-requests.md`.
