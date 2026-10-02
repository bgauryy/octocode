Good enough; I'll skip the unix-socket test files (uds_*) that I didn't read.

**Summary:** PR #8156 (merged 2026-05-26, merge commit `32312ae`, 37 files, +165/−155) removes the blanket "No `socket` in miri" exclusions from tokio's TCP tests and docs. Miri gained TCP socket support in recent months. The PR also bumps the pinned Miri nightly in `.github/workflows/ci.yml` from `nightly-2026-04-27` to `nightly-2026-05-20`.

**What it changes**
- **Test files re-enabled:** it drops the `not(miri)` gate or `cfg_attr(miri, ignore)` from `tcp_accept`, `tcp_connect`, `tcp_echo`, `tcp_into_split`, `tcp_into_std`, `tcp_peek`, `tcp_shutdown`, `tcp_split`, `tcp_stream`, `tcp_socket`, `io_driver`, `io_driver_drop`, `net_bind_resource`, `no_rt`, `rt_handle_block_on` and `rt_threaded`. `rt_common.rs` loses its file-level `#![cfg(not(miri))]`. These files are re-enabled in whole or in part, with new targeted ignores (see below).
- **`net_lookup_host`:** the `getaddrinfo` ignore on `resolve_dns` is removed.
- **Doctests:** the `if cfg!(miri) { return Ok(()); }` guards are removed from the `TcpListener`, `TcpSocket` and `TcpStream` docs in `tokio/src/net/tcp/`.
- **Slow tests:** `tcp_echo` uses a lower iteration count under Miri (`#[cfg(miri)]`). Several `rt_common` and `rt_threaded` tests get `ignore` with the comment "Too slow on miri".
- **Comment-only edits:** the generic "No `socket`" comments are replaced with specific reasons in the UDP, Unix-socket and `net_panic` tests and docs. There is also a doc-link fix in `tokio/src/io/util/mod.rs`.

**Networking tests still not run under Miri, and why** (from the patches I read)
- **UDP:** `tokio-util/tests/udp.rs`, `tokio/tests/udp.rs`, `udp_socket_from_std_panic_caller` and a UDP test in `rt_common` and `rt_handle_block_on` have no UDP support in Miri.
- **Unix domain sockets:** the `net_panic` unix tests, `uds_cred` and the unix tests in `rt_handle_block_on` still carry ignore or cfg gates. `uds_cred` has no `getsockopt` for Unix domain sockets. I did not read the `uds_*` patches (`uds_datagram`, `uds_socket`, `uds_split`, `uds_stream`), which are a few lines each.
- **FIFOs:** `net_unix_pipe::fifo_resilient_reader` has no `mkfifo` in Miri.
- **TCP socket options and features** (`tcp_socket.rs`, `tcp_stream.rs`, `tcp_shutdown.rs`, `net_quickack.rs`):
  - `SO_LINGER` is unsupported (`set_linger`, `shutdown`, the linger tests in `tcp_socket`).
  - Binding before connecting is unsupported (`bind_before_connect`).
  - Unsupported options: keepalive set, reuseaddr read, reuseport set, send/recv buffer size set, TClass and TOS. `nodelay` is supported only on connected sockets.
  - TCP quickack is unsupported.
- **Miri `readv` leak:** a part of `try_read_write` in `tcp_stream.rs` is `#[cfg(not(miri))]`. Miri leaks memory when `readv` returns an error (rust-lang/miri PR #5054).
- **Host I/O events:** `blocking_one_side_does_not_block_other` in `io_copy_bidirectional.rs` and two `rt_common` tests are ignored. Miri only processes host I/O events when switching into the scheduler (rust-lang/miri issue #5047).
- **Leaked threads:** three `rt_common` tests are ignored because Miri detects leaked threads (tokio #7010). These are not strictly networking tests.

**Uncertainty:** the patch output was truncated and I did not read the `uds_*` file diffs. The PR body says follow-up PRs will re-enable the `readv` and host-I/O tests once the Miri fixes land in a nightly.