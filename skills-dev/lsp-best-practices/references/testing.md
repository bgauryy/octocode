# Testing: prove the client without real servers

Load when adding or fixing LSP client code and deciding which tests prove it, or when a bug needs a reproducing fixture. Why: real language servers are slow, nondeterministic, and missing on CI. Nearly every client property (framing, cancellation, timeouts, leaks, readiness) can be proven faster and more reliably with in-memory fakes and paused time.

## Test tiers
| Tier | Tool | Proves | Speed |
|---|---|---|---|
| Codec unit | a byte buffer or `tokio::io::duplex(64)` | header parsing, caps, partial chunks, resync, string ids | ms |
| Connection | duplex + a scripted server task | out-of-order responses, server→client requests, cancel on drop, timeouts, fail-all-pending | ms |
| Time-dependent | `#[tokio::test(start_paused = true)]` + `time::advance` | timeouts, retry backoff, idle eviction, graceful-exit windows, readiness quiet windows, all deterministic | ms |
| Process | a tiny fake server binary or script (Node/Python) | spawn, group kill, reap, stderr drain, memory cap, crash mid-request | 100s of ms |
| Surface | rebuilt `$OCTO lspSearch` against real rust-analyzer and typescript-language-server | end-to-end results, line numbers, cold vs warm timing | seconds |

## Fake server shape (from Zed's `FakeLanguageServer`)
- Wire two pipes between the **real client** and a scripted server.
- Pre-register replies for `initialize` (configurable capabilities) and `shutdown`.
- Offer `on_request::<M>(handler)`, which returns a receiver that fires each time the handler responds, so a test can wait for "the handler was hit".
- Offer `expect_notification::<M>()`, which skips notifications of other types.
- Offer `notify(..)` and `request(..)` so the server can send `$/progress`, `publishDiagnostics`, or `workspace/configuration`.
- Add fault knobs: delay, never reply, reply out of order, send a garbage or oversized frame, close stdout, flood with server requests, stop reading stdin.

## Must-have cases
| Area | Case |
|---|---|
| Framing | chunks split at every byte; lowercase or extra headers; missing `Content-Length`; length > cap is rejected **before** allocating; garbage prefix; CRLF vs LF header ends |
| Cancel | drop a `request` future before the write, during the write, and while awaiting the reply → a `$/cancelRequest` is sent, the pending entry is gone, and the next request succeeds (or the connection is failed if a frame was cut) |
| Timeout | one request times out → per policy, the others are failed or still served; no entry leaks |
| Server requests | `configuration` answered per `section`; `registerCapability` → `null`; unknown → `-32601`; a reply while the server isn't reading stdin doesn't deadlock the reader |
| Readiness | begin/end before `didOpen` is seen; a second wave restarts the wait; silent server → `settledWithoutProgress`; the deadline is honored |
| Coordinates | non-ASCII (é, emoji surrogate pairs), CRLF, lone CR, first and last line, a single-char identifier |
| Lifecycle | cancel at every `await` of start; leak loop (N× acquire/stop, no PIDs left, grandchildren included); a crash mid-request fails pending requests fast |
| Walks | cycle, diamond, fan-out above the cap → `truncated`; failed node → partial result plus retry |

## Property and fuzz
- Round-trip property (proptest): `encode(msgs)`, split at arbitrary offsets, `decode` → the same `msgs`. With a garbage prefix → it resyncs, or the connection is failed per policy.
- `cargo-fuzz` target on the header and frame decoder: it never panics, never allocates above the cap, and never loops forever. No surveyed LSP client fuzzes its codec, so this is cheap, differentiating coverage.
- Snapshot the outbound `initialize` params and first `didOpen` (insta, with ids and paths redacted). This catches capability drift, such as declaring `didSave` without ever sending it.

## Rules
- Every bug fix lands with the fixture that reproduces it. Add it to the smallest tier that shows the bug.
- Paused-time tests replace real sleeps. A test that sleeps for real is flaky and slow.
- Surface checks (cold and warm `$OCTO` runs) come last and don't replace the tiers above.

Next: for octocode's existing tests and missing fixtures load `references/octocode-engine-map.md`.
