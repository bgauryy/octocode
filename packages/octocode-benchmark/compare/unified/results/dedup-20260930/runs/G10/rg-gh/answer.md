**What PR #8156 does (merged as 32312ae0, head c5d51ae8)**

It removes the blanket "No `socket` in miri" exclusions from the TCP tests, because Miri now supports TCP sockets. The PR description says this.

- **CI:** it bumps the pinned Miri nightly from `nightly-2026-04-27` to `nightly-2026-05-20` (`.github/workflows/ci.yml`).
- **Un-ignored tests:** it drops the `cfg_attr(miri, ignore)` and `cfg(not(miri))` guards from the TCP and runtime tests.
  - Affected files: `tcp_accept`, `tcp_connect`, `tcp_echo`, `tcp_into_split`, `tcp_into_std`, `tcp_peek`, `tcp_split`, `tcp_stream`, `tcp_socket`, `tcp_shutdown`, `io_driver`, `io_driver_drop`, `net_bind_resource`, `no_rt`, `io_copy_bidirectional` and most of `net_panic`.
  - It also un-ignores `net_lookup_host::resolve_dns` (removes the "No `getaddrinfo`" ignore).
- **Doctests:** it removes the `if cfg!(miri) { return Ok(()); }` early returns from the `TcpListener`, `TcpSocket` and `TcpStream` doc examples.
- **Slow tests:** some tests were made cheaper for Miri. `tcp_echo` uses a lower iteration count under `cfg(miri)`. Others are ignored with "Too slow on miri" in `rt_common.rs` and `rt_threaded.rs`. The description says buffer sizes and iteration counts were reduced for tests it considered important and the rest were ignored. I only saw the `tcp_echo` change in the diff output.
- **Comment cleanups:** the remaining ignore comments are reworded to name the specific missing feature, for example "No UDP sockets in miri" and "No Unix domain sockets in miri". There is also an unrelated link fix for `DEFAULT_BUF_SIZE` in `io/util/mod.rs`.

**Networking tests still not run under Miri, and why** (from the diff's `+` lines)

| Area | Reason |
|---|---|
| `tcp_stream.rs`, a test guarded by `cfg(not(miri))` | Miri leaks memory when `readv` returns an error (rust-lang/miri#5054) |
| `io_copy_bidirectional::blocking_one_side_does_not_block_other`, two `rt_common.rs` tests | Miri only processes host I/O events when switching into the scheduler (rust-lang/miri#5047) |
| `SO_LINGER` tests in `tcp_shutdown`, `tcp_socket` and `tcp_stream` | Unsupported |
| `tcp_socket.rs`: binding before connecting | Unsupported |
| `tcp_socket.rs`: setting keepalive, reading reuseaddr, setting reuseport, setting send and receive buffer sizes | Unsupported socket options |
| `tcp_socket.rs`: `TCP_NODELAY` | Miri supports it only on connected sockets |
| `tcp_socket.rs`: TClass and TOS tests | Unsupported |
| `net_quickack.rs` | TCP quickack option unsupported |
| UDP: `tokio/tests/udp.rs`, `tokio-util/tests/udp.rs`, `net_panic` UDP test, UDP docs and `rt_handle_block_on` UDP test | No UDP sockets in Miri |
| Unix domain sockets: `uds_datagram`, `uds_socket`, `uds_split`, `uds_stream`, `net_panic` and `rt_handle_block_on` UDS tests, UDS doctests | No Unix domain sockets in Miri |
| `uds_cred.rs` | No `getsockopt` for Unix domain sockets |
| `net_unix_pipe.rs` | No `mkfifo` |
| Three `rt_common.rs` tests | Miri detects leaked threads (tokio#7010) |
| Slow tests in `rt_common.rs` and `rt_threaded.rs` | Too slow on Miri |

**Uncertainty**
- I read only the `+` lines of the diff and did not open the tests at the merge commit, so I have no `path:line` citations. I also did not check whether Miri issues #5047 and #5054 are still open.
- The PR description says follow-up PRs will re-enable the #5047 and #5054 tests once those land in a nightly. I did not check for such follow-ups.
- The table is based on a filtered grep of the diff for `ignore` and `miri`, so a few minor ignores may be missing.