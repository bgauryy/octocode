**PR #8156 (merged 2026-05-26, `master`) un-ignores the TCP-socket tests under Miri. Miri now has TCP support, so the old "No `socket` in miri" ignores are no longer needed.** Some networking tests are still skipped, and each skip now has a specific reason. I read the patches for the test and doc files. Page 2 of the 37-file list wasn't fully paged through, and the `rt_common.rs` patch was only partly visible, so the list below may miss a few skips.

**What it changes**
- **CI:** it bumps `rust_miri_nightly` from `nightly-2026-04-27` to `nightly-2026-05-20` (`.github/workflows/ci.yml`).
- **Tests enabled under Miri:** it removes `#[cfg_attr(miri, ignore)] // No socket in miri` and `#![cfg(not(miri))]` from many TCP tests. These include `io_copy_bidirectional`, `io_driver`, `io_driver_drop`, `net_bind_resource`, `no_rt`, `net_panic` (TCP cases), `rt_common`, `rt_handle_block_on`, `rt_threaded` and `tcp_stream`.
- **`net_lookup_host`:** it also drops the "No `getaddrinfo`" ignore.
- **Doctests:** it removes the `if cfg!(miri) { return Ok(()); }` guards from the `TcpListener`, `TcpSocket` and `TcpStream` doc examples.
- **Slow tests:** the PR body says it shrank buffers or iteration counts for tests it considered important. The patches I saw show `#[cfg_attr(miri, ignore)] // Too slow on miri` added to several `rt_common` and `rt_threaded` tests.
- **Comment-only edits:** the remaining ignores keep their skips but get accurate reasons. The wording is now "No UDP sockets", "No Unix domain sockets" or "No `mkfifo`" (`net_unix_pipe.rs` `fifo_resilient_reader`), plus a small URL fix in `io/util/mod.rs`.

**Networking tests still skipped under Miri, and why**
- **UDP:** `tokio-util/tests/udp.rs`, `net_panic` `udp_socket_from_std_panic_caller`, a UDP test in `rt_common`, and one in `rt_handle_block_on`. Miri has no UDP sockets.
- **Unix domain sockets:** `uds_datagram.rs`, `uds_socket.rs`, the `net_panic` unix cases, an `rt_handle_block_on` case, and the `UnixDatagram` and `UnixStream` doctests. Miri has no Unix domain sockets.
- **FIFOs:** `net_unix_pipe::fifo_resilient_reader` is skipped because Miri has no `mkfifo`.
- **`net_quickack.rs`:** it stays `cfg(not(miri))` because Miri doesn't support the TCP quickack option.
- **`SO_LINGER`:** `tcp_shutdown::shutdown`, `tcp_stream::set_linger`, and two `tcp_socket.rs` tests are skipped because Miri doesn't support `SO_LINGER`. I only saw the ignore attributes for the two `tcp_socket.rs` tests, not which test each one sits on.
- **`tcp_socket.rs`, other limits:** `bind_before_connect` is skipped because Miri doesn't support binding before connecting. One more test is skipped because Miri only supports `TCP_NODELAY` on connected sockets.
- **Host I/O events:** `io_copy_bidirectional::blocking_one_side_does_not_block_other` and two `rt_common` tests are skipped. Miri only processes host I/O events when switching into the scheduler (rust-lang/miri#5047).
- **`readv` memory leak:** the PR body says some tests are affected by a `readv` leak (rust-lang/miri#5054). I didn't see which tests those are.
- **Leaked threads:** three `rt_common` tests carry the reason "Miri detects leaked threads (see #7010)". The patch doesn't show that they use sockets.
- **Too slow:** several `rt_common` and `rt_threaded` tests, including `spawn_many_from_block_on`, `spawn_many_from_task` and `rt_threaded::single_thread`. This is a speed problem, not a missing feature.

The author says follow-up PRs will re-enable the tests blocked by the two open Miri issues once the fixes land in a nightly.