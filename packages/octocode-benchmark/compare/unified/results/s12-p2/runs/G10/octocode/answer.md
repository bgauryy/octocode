**Short answer:** PR #8156 was merged on 2026-05-26 (head `c5d51ae`). It turns Miri on for the tests that use TCP sockets, because Miri now supports TCP. The TCP tests that still don't run under Miri are skipped for one of four reasons: Miri lacks a socket option, Miri has an open I/O-event bug, a test is too slow, or a test leaks threads.

I couldn't see the second page of changed files (37 in total, 30 per page) or the rest of the `rt_common.rs` patch. The list below may therefore miss a few ignores.

## What it changes

- **Removes the old Miri guards.** The PR drops the `// No socket in miri` guards:
  - File-level `#![cfg(not(miri))]` or `not(miri)` clauses come out of `tcp_accept`, `tcp_connect`, `tcp_echo`, `tcp_into_split`, `tcp_into_std`, `tcp_peek`, `tcp_shutdown`, `tcp_socket`, `tcp_split`, `net_bind_resource`, `rt_common`, `rt_handle_block_on` and `rt_threaded`.
  - Per-test `#[cfg_attr(miri, ignore)]` lines come out of `io_copy_bidirectional`, `io_driver`, `io_driver_drop`, `net_panic`, `no_rt` and `net_lookup_host`.
  - Three doc-test `if cfg!(miri) { return Ok(()); }` lines come out of the `TcpListener`, `TcpSocket` and `TcpStream` docs.
- **Makes slow tests cheaper.** `tcp_echo.rs` now uses a lower iteration count under Miri, with `#[cfg(not(miri))]` and `#[cfg(miri)]` variants. Some tests are ignored as "Too slow on miri".
- **Bumps the CI Miri nightly** from `nightly-2026-04-27` to `nightly-2026-05-20` in `.github/workflows/ci.yml`.
- **Updates comments.** Blanket "No `socket`" comments become specific ones, such as "No UDP sockets in miri" and "No Unix domain sockets in miri". The `DEFAULT_BUF_SIZE` link in `tokio/src/io/util/mod.rs` is refreshed too.

## Still ignored under Miri

The PR description names two open Miri issues. I only saw the second one tied to specific tests. The `readv` leak fix, rust-lang/miri#5054, isn't tied to a test in what I read.

1. **Host I/O events are only processed when the scheduler is entered** (rust-lang/miri#5047). This ignores `blocking_one_side_does_not_block_other` in `io_copy_bidirectional.rs`. It also ignores two `rt_common.rs` tests, whose names I didn't see.
2. **Unsupported TCP socket options.** These are the ignored tests in `tcp_socket.rs` and `tcp_shutdown.rs`:
   - `SO_LINGER`: `tcp_shutdown::shutdown`, plus the linger tests in `tcp_socket.rs`.
   - Binding before connecting: `bind_before_connect`.
   - Setting keepalive, setting or reading reuseaddr, setting reuseport, and setting the send and receive buffer sizes.
   - `TCP_NODELAY` is supported only on connected sockets. That test is ignored.
   - TClass and TOS: the TClass test is `#[cfg(not(miri))]`, and the TOS test is excluded with a `miri` cfg clause.
3. **Too slow.** These are `spawn_many_from_block_on`, `spawn_many_from_task`, two more `rt_common` tests, and several `rt_threaded` tests.
4. **Leaked threads.** Three `rt_common` tests are ignored with "Miri detects leaked threads (see #7010)".

## Networking tests excluded for other reasons

- **UDP** isn't supported in Miri: `tokio-util/tests/udp.rs`, `udp_socket_from_std_panic_caller`, and one `rt_common` and one `rt_handle_block_on` test.
- **Unix domain sockets** aren't supported in Miri: the Unix panic tests in `net_panic.rs` and one `rt_handle_block_on` test.
- **`mkfifo`** isn't supported in Miri: `fifo_resilient_reader` in `net_unix_pipe.rs`.
- **TCP quickack:** `net_quickack.rs` stays `cfg(not(miri))` because Miri doesn't support the option.