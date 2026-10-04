**Summary:** PR #8156 was merged on 2026-05-26 into `master`. It removes most of the `#[cfg_attr(miri, ignore)] // No socket in miri` markers from TCP-related tests, because Miri now supports TCP sockets. I read the patches for about 25 of the 37 changed files. I did not read the `tcp_*` test patches, which are on page 2 of the file list. Treat those as unreviewed.

**What it changes** (PR body and patches at head `c5d51ae`)
- **Un-ignores TCP tests.** It removes the "No `socket` in miri" ignores or `cfg(not(miri))` gates from these files:
  - `io_copy_bidirectional.rs`, `io_driver.rs`, `io_driver_drop.rs`, `net_bind_resource.rs`, `no_rt.rs`
  - the TCP cases in `net_panic.rs`
  - the socket tests in `rt_common.rs`, `rt_handle_block_on.rs` and `rt_threaded.rs`
  - `rt_common.rs` also loses its file-level `#![cfg(not(miri))]`.
- **Un-ignores `resolve_dns`.** It also drops the "No `getaddrinfo`" ignore on `resolve_dns` in `net_lookup_host.rs`.
- **Doctests.** The `if cfg!(miri) { return Ok(()); }` guards are removed from the `TcpListener`, `TcpSocket` and `TcpStream` doctests.
- **Slow tests.** The PR body says tests that were too slow under Miri got smaller buffers or fewer iterations if they mattered. The rest were ignored with `// Too slow on miri`. Examples are `spawn_many_from_block_on` and `spawn_many_from_task` in `rt_common.rs`, and several tests in `rt_threaded.rs`.
- **Housekeeping.** It bumps the pinned `rust_miri_nightly` from `nightly-2026-04-27` to `nightly-2026-05-20` in `ci.yml`. It also rewrites the remaining ignore comments to give accurate reasons, and fixes a stale link comment in `io/util/mod.rs`.

**Networking tests that still don't run under Miri, and why**
- **Host I/O events are only processed when switching into the scheduler** (https://github.com/rust-lang/miri/issues/5047). This affects `blocking_one_side_does_not_block_other` in `io_copy_bidirectional.rs` and two tests in `rt_common.rs` (around lines 1158 and 1172).
- **`readv` can leak memory** (https://github.com/rust-lang/miri/pull/5054). The PR body names this as a blocker. I did not see which test it affects, because that is probably in the `tcp_*` files I didn't read.
- **UDP sockets are unsupported.** `tokio-util/tests/udp.rs`, `udp_socket_from_std_panic_caller` in `net_panic.rs`, and UDP tests in `rt_common.rs` and `rt_handle_block_on.rs` stay ignored, as do the UDP doctests.
- **Unix domain sockets are unsupported.** This covers the `UnixListener`, `UnixStream` and `UnixDatagram` tests and doctests, including the Unix cases in `net_panic.rs` and `rt_handle_block_on.rs`.
- **TCP quickack option unsupported.** `net_quickack.rs` stays `cfg(not(miri))`.
- **FIFOs.** `fifo_resilient_reader` in `net_unix_pipe.rs` stays ignored because Miri has no `mkfifo`.
- **Leaked threads.** Three `rt_common.rs` tests are ignored because "Miri detects leaked threads (see #7010)". They are not strictly networking tests.

The PR body says follow-up PRs will re-enable the I/O-event and `readv` tests once those Miri fixes land in a nightly.