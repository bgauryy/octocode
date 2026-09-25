# Rust Implementation: tokio idioms for an LSP client

Load when writing or reviewing the Rust inside an LSP client: the frame reader and writer, response parsing, error types, locks, spawned tasks, drop guards, or process spawning. Why: most LSP transport bugs are Rust/tokio bugs. A future that isn't cancel-safe, a detached task, a stringly-typed error, or a huge `Value` clone all look correct in review and fail under load. The tokio quotes below come from docs.rs summaries, so re-check exact wording before citing.

## Cancel-safety (tokio docs)
| Call | Cancel-safe? | Consequence if a `select!` or timeout drops it |
|---|---|---|
| `read` / `read_buf` / `fill_buf` | yes | nothing is lost |
| `Lines::next_line`, `FramedRead::next` | yes | partial bytes stay in the reader or codec buffer |
| `read_line` | **no** | partially read data is **lost**, so the stream desyncs |
| `read_exact` | **no** | part of the body has already been consumed |
| `read_until` | resumable only if the `Vec` outlives the future | |
| `write_all` | **no** | a **partial frame** reaches the server |
| `CancellationToken::cancelled`, `oneshot` recv | yes | |

**Rules:**
- The **read loop owns its reader alone** and never races `read_line`/`read_exact` against another branch.
- If reads must be cancellable, use a `Decoder` over `BytesMut` with `FramedRead`, keeping state in the codec. tower-lsp `src/codec.rs` is the model: it returns `Ok(None)` on a partial header or body, and after garbage it resyncs by scanning for `Content-Length`. Add the max-length cap that tower-lsp lacks.
- **Writes go through one writer task** (fed by an mpsc channel of whole frames). A caller's cancellation can then never cut a frame in half. The alternative is a lock-and-write where any dropped or failed write marks the connection failed.
- Use a **drop guard to poison the connection on every exit path**, including panic and early `return`: `struct FailOnExit(Arc<AtomicBool>, PendingMap)` whose `Drop` sets `failed` and drains pending. This replaces the scattered `failed.store(true)` calls before each `break`/`return`.

## Frames and parsing
- Check the `Content-Length` cap **before** allocating. Helix and async-lsp allocate `vec![0; len]` from the server's claim, which turns a bad header into an out-of-memory. Reuse one body buffer between frames.
- `serde_json::from_slice` on the frame bytes. Never `from_reader` on the stream (byte-at-a-time, unbounded), and never `from_utf8` followed by `from_str`.
- **Lazy results:** use one envelope `{id, method, params: Option<&RawValue>, result: Option<&RawValue>, error: Option<ResponseError>}` with `#[serde(borrow)]` (Zed `AnyResponse`). Route by `id`/`method`, and deserialize the typed result in the waiting caller's task.
- If you keep `Value`, **move, don't clone**: `obj.remove("result")` rather than `.get("result").cloned()`. A cloned 50 MB references result is a second 50 MB. Serialize each outbound message **once** and reuse the bytes, both for size checks and for the write.
- Write the header and body as one buffer (or `write_vectored`) under a single lock, then flush.
- SIMD parsers (sonic-rs in Helix, simd-json) are worth adopting only after a benchmark on large payloads.

## Errors
- Use a `thiserror` enum at the library boundary: `Rpc(ResponseError{code, message, data})`, `Timeout{id}`, `ConnectionClosed`, `Protocol(String)`, `Io(#[from] io::Error)`, `Deserialize(#[from] serde_json::Error)`.
- Put codes in an enum with a fallback: `ErrorCode::{ContentModified, ServerCancelled, RequestCancelled, MethodNotFound, …, Other(i64)}` (Zed, lsp-server). Match on the enum, **never on the rendered message text**. Add helpers: `is_retryable()` (ContentModified or ServerCancelled) and `should_retrigger()`.
- Keep the transport distinction (timeout vs closed vs RPC error) all the way up, so callers can choose retry, restart, or report.
- `anyhow` is only for binaries and tests. Convert to the napi or public error type once, at the binding boundary.

## Locks, atomics, tasks
- **`std::sync::Mutex`** for short critical sections that never cross an `.await`: pending map, stderr ring, capabilities, open documents. **`tokio::sync::Mutex`** only when the guard must be held across an `.await`.
- **Clone the `Arc`, drop the guard, then await** (octocode's `connection_handle`). Never hold a lock across a request.
- Ids are `AtomicU64::fetch_add(1, Relaxed)`, because uniqueness needs no ordering. Use Acquire/Release for flags such as `failed`, where readers act on other state.
- **No detached tasks.** Reader, writer, and stderr tasks are owned as `AbortOnDropHandle` (tokio-util) or aborted in `Drop`. Use `CancellationToken::child_token()` per server plus `drop_guard()` for a session-wide stop. Use `watch<State>` for Starting/Ready/Dead, where many waiters read the current value.
- **Drop can't await:** spawn async cleanup from `Drop` only through a runtime handle you **stored at construction**. `Handle::try_current()` inside `Drop` fails silently on non-runtime threads (napi finalizers), and the cleanup is then skipped.

## Process spawning
- `tokio::process::Command` with `.kill_on_drop(true)` and, on Unix, `.process_group(0)` (stable in tokio 1.40). `kill_on_drop` signals only the direct child and reaps best-effort, so do an explicit `shutdown` → `exit` → `timeout(wait)` → `killpg` → `wait`.
- On Windows, use a Job Object with kill-on-close, assigned while the process is suspended so children can't escape. `process-wrap` implements this; `command-group` is deprecated in its favour.
- Avoid `unsafe pre_exec` unless you need it (`setrlimit`, `setsid`). Inside it, only async-signal-safe calls are allowed: no allocation, no locks.
- stderr: read with a **bounded** `read_until(b'\n')` into a capped buffer, then `String::from_utf8_lossy`. Keep draining after errors.

Next: for how to prove these properties load `references/testing.md`.
