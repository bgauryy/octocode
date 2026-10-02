**What it does.** PR #8156 is merged (merge commit `32312ae0`, 37 files changed). Miri now supports TCP sockets, so the PR stops skipping tokio's TCP tests under Miri. It removes the blanket `not(miri)` and `cfg_attr(miri, ignore)` gates, which were annotated "No `socket` in miri". It also bumps the pinned Miri nightly from `nightly-2026-04-27` to `nightly-2026-05-20` in `.github/workflows/ci.yml`.

- **Newly enabled:** the `tcp_*` integration tests (`tcp_accept`, `tcp_connect`, `tcp_echo`, `tcp_into_split`, `tcp_into_std`, `tcp_peek`, `tcp_split`, `tcp_shutdown`, `tcp_socket`, `tcp_stream`) now run under Miri, with some cases still ignored (listed below). `io_driver`, `io_driver_drop`, `no_rt`, `net_bind_resource` and most of `rt_common`, `rt_handle_block_on` and `net_panic` are newly enabled as well. The `Ok(())` early-return for Miri is removed from the `TcpListener`, `TcpSocket` and `TcpStream` doc tests (`tokio/src/net/tcp/{listener,socket,stream}.rs`).
- **Cost tuning:** `tcp_echo.rs` uses a lower iteration count under Miri. Some `rt_common` and `rt_threaded` tests are ignored with "Too slow on miri". `rt_threaded.rs` moves from a file-level `not(miri)` to per-test ignores.
- **Relabelled, not enabled:** the vague "No `socket`" comments are replaced with specific reasons for UDP (`udp.rs`, `tokio-util/tests/udp.rs`) and Unix domain sockets (`uds_*`, `net_panic`), which stay disabled.

**Networking tests still not run under Miri, and why** (taken from the diff comments, not from running Miri):

| Area | Reason given |
|---|---|
| UDP (`tokio/tests/udp.rs`, `tokio-util/tests/udp.rs`, UDP doc tests) | "No UDP sockets in miri" |
| Unix domain sockets (`uds_datagram`, `uds_socket`, `uds_split`, `uds_stream`, `net_panic` UDS tests, doc tests) | "No Unix domain sockets in miri" |
| `uds_cred` | "No `getsockopt` for Unix domain sockets" |
| `net_unix_pipe` | "No `mkfifo` in miri" |
| `net_lookup_host` | The ignore is removed from one test, but I did not check how the rest of that file is gated. |
| `net_quickack` | "Miri doesn't support TCP quickack socket option" |
| `tcp_shutdown`, `tcp_socket`, `tcp_stream` linger tests | "Miri doesn't support `SO_LINGER`" |
| `tcp_socket`: bind before connect | "Miri doesn't support binding before connecting" |
| `tcp_socket`: socket options | Miri lacks setting keepalive, reading reuseaddr, setting reuseport, setting the send buffer size and setting the receive buffer size. |
| `tcp_socket`: `TCP_NODELAY` | "Miri only supports `TCP_NODELAY` on connected sockets" |
| `tcp_socket`: TClass and TOS | "Miri doesn't support TClass" and "Miri doesn't support TOS" |
| `tcp_stream` readv-error test | Miri leaks memory when `readv` returns an error (rust-lang/miri#5054). |
| `io_copy_bidirectional` (one test) and two `rt_common` tests | Miri only processes host I/O events when switching into the scheduler (rust-lang/miri#5047). |
| `rt_common` (three tests) | "Miri detects leaked threads (see #7010)" |

The PR body says follow-up PRs will re-enable the `readv` and I/O-event tests once those two Miri issues are fixed and reach a nightly.

**Uncertainty:** I read only the PR diff and description, not the merged tree, so line numbers are not cited. The table covers the Miri-specific reasons visible in the diff. Tests that were already gated for other reasons, such as wasi, may exist outside it. I did not check whether Miri#5047 or #5054 have been fixed since.