**PR #8156 (merged; merge commit `32312ae`) removes the blanket "No `socket` in Miri" exclusions from the TCP tests, so they now run under Miri.** The PR body says Miri has gained TCP socket support over the past months. It adds narrower ignores where Miri still can't handle a test. I read the PR metadata and per-file patches through the GitHub API. I did not check out the code or run the tests.

**What it changes**
- **CI:** `.github/workflows/ci.yml` bumps `rust_miri_nightly` from `nightly-2026-04-27` to `nightly-2026-05-20`.
- **TCP test files:** `tcp_accept`, `tcp_connect`, `tcp_echo`, `tcp_into_split`, `tcp_into_std`, `tcp_peek`, `tcp_shutdown`, `tcp_socket`, `tcp_split` and `tcp_stream` drop their file-level `not(miri)` gate. The same happens for the TCP-based tests in `io_driver`, `io_driver_drop`, `net_bind_resource`, `no_rt`, `io_copy_bidirectional` and `rt_handle_block_on`.
- **`rt_common` and `rt_threaded`:** the file-level `not(miri)` gates are removed. Individual tests are ignored instead, mostly as "Too slow on miri".
- **Slow tests:** the PR body says some tests used tiny buffers or many iterations. Where it considered the test important, it changed the buffer size or iteration count (for example `tcp_echo.rs` uses a lower iteration count under `cfg(miri)`). Otherwise it added `#[cfg_attr(miri, ignore)]`.
- **Doc comments:** the `if cfg!(miri) { return Ok(()); }` guards are removed from the TCP doctests in `listener.rs`, `socket.rs` and `stream.rs`.
- **Relabelled comments:** UDP and Unix-domain-socket tests stay excluded, but their reason now reads "No UDP sockets" or "No Unix domain sockets" instead of "No `socket`".

**Networking tests that still don't run under Miri, and why**

| Group | Reason (from the added comments) |
|---|---|
| UDP (`tokio-util/tests/udp.rs`, `tokio/tests/udp.rs`, UDP tests in `net_panic`, `rt_common` and `rt_handle_block_on`, UDP doctests) | No UDP sockets in Miri |
| Unix domain sockets (`uds_datagram`, `uds_socket`, `uds_split`, `uds_stream`, UDS tests in `net_panic`, `rt_handle_block_on` and others) | No Unix domain sockets in Miri |
| `uds_cred` | No `getsockopt` for Unix domain sockets |
| `net_unix_pipe` | No `mkfifo` |
| `net_lookup_host` | No `getaddrinfo` |
| `net_quickack` | Miri doesn't support the TCP quickack option |
| `tcp_socket`: bind before connect | Miri doesn't support binding before connecting |
| `tcp_socket`: keepalive, reuseaddr (read), reuseport, send and receive buffer size | Miri doesn't support these socket options |
| `tcp_socket`: TCP_NODELAY | Miri supports it only on connected sockets |
| `tcp_socket`: TClass and TOS | Miri doesn't support them |
| Linger tests in `tcp_shutdown`, `tcp_socket` and `tcp_stream` | Miri doesn't support `SO_LINGER` |
| Part of `tcp_stream` (`#[cfg(not(miri))]`) | Memory leak when `readv` returns an error (rust-lang/miri PR #5054) |
| `io_copy_bidirectional` (one test), two `rt_common` tests | Miri only processes host I/O events when switching into the scheduler (rust-lang/miri issue #5047) |
| Three `rt_common` tests | Miri detects leaked threads (tokio #7010) |
| Several `rt_common` and `rt_threaded` tests | Too slow on Miri |

The PR body says the author will open follow-up PRs in tokio once the two Miri issues (#5054 and #5047) are fixed and reach a nightly.

**Uncertainty**
- My list comes from the diff's added and removed lines. Tests that were already ignored before the PR and not touched by it may not appear.
- I didn't match each ignore to its exact test name, so the per-group counts are approximate.