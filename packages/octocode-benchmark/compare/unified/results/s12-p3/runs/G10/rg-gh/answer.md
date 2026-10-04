**Summary.** PR #8156 is merged (merge commit `32312ae`, head `c5d51ae`) and touches 37 files. It lets the TCP-socket tests run under Miri, because Miri has gained TCP support. The tests had been ignored with the comment "No `socket` in miri". I read the PR's file patches. I did not run the tests.

**What it changes**
- **CI pin:** `.github/workflows/ci.yml` bumps `rust_miri_nightly` from `nightly-2026-04-27` to `nightly-2026-05-20`.
- **Ignores removed:** the blanket `cfg(not(miri))` and `cfg_attr(miri, ignore)` "No `socket`" markers come off these:
  - TCP tests: `tcp_accept`, `tcp_connect`, `tcp_echo`, `tcp_into_split`, `tcp_into_std`, `tcp_peek`, `tcp_split`, `tcp_stream`, `tcp_socket`, `tcp_shutdown`.
  - Runtime and driver tests: `io_driver`, `io_driver_drop`, `net_bind_resource`, `no_rt`, `rt_common`, `rt_handle_block_on`, `rt_threaded`.
  - Doctests in `tokio/src/net/tcp/{listener,socket,stream}.rs`.
- **Tests made cheaper:** `tcp_echo` gets a lower iteration count under Miri. The PR body says some tests used tiny buffers or many iterations, so it changed buffer size or iteration count for the ones it considered important.
- **Ignores that stay but get accurate reasons:** the UDP, Unix-domain-socket and `mkfifo` markers are reworded, for example "No UDP sockets in miri", "No Unix domain sockets in miri" and "No `mkfifo` in miri". Those tests still don't run.

**Still not run under Miri, and why**

*TCP-specific:*
- **`SO_LINGER`:** `tcp_shutdown.rs`, `tcp_stream.rs` and `tcp_socket.rs` ignore with "Miri doesn't support `SO_LINGER`".
- **Bind before connect:** a `tcp_socket.rs` test is ignored with "Miri doesn't support binding before connecting".
- **`TCP_NODELAY` on unconnected sockets:** a `tcp_socket.rs` test is ignored because "Miri only supports `TCP_NODELAY` on connected sockets".
- **`TClass` and `TOS`:** the `tcp_socket.rs` tests for these are excluded. The comments read "Miri doesn't support TClass" and "Miri doesn't support TOS".
- **`TCP_QUICKACK`:** `net_quickack.rs` stays `cfg(not(miri))` with "Miri doesn't support TCP quickack socket option".
- **`readv` error leak:** a `tcp_stream.rs` test is excluded because "Miri currently has a memory leak when `readv` returns an error" (rust-lang/miri#5054).
- **Host I/O events:** `io_copy_bidirectional.rs` and two `rt_common.rs` tests are ignored because "Miri currently only processes host I/O events when switching into the scheduler" (rust-lang/miri#5047).

*Other networking tests, unchanged by this PR in substance:*
- **UDP and Unix domain sockets:** still unsupported in Miri.
- **`mkfifo`:** `net_unix_pipe.rs` is ignored because Miri has no `mkfifo`.
- **DNS lookup:** `net_lookup_host.rs` was ignored for "No `getaddrinfo` in miri". I only saw that line removed in the diff. I didn't check what replaced it, so I can't say whether it now runs.

*Ignored for speed or thread leaks, not missing Miri features:*
- **Too slow:** some `rt_common.rs` and `rt_threaded.rs` tests carry "Too slow on miri".
- **Leaked threads:** three `rt_common.rs` tests are ignored with "Miri detects leaked threads (see #7010)".

**Follow-up:** the PR body says the `readv` and host-I/O-event cases will get new PRs once the Miri fixes land in a nightly.

**Uncertainty:** I read the diff patches, not the final files. I didn't map every ignore to its individual test name. Several `tcp_socket.rs` ignores sit inside macro or multi-line attributes (the bare `miri,` lines), so my reasons for those come from nearby comments.