**Short answer:** PR #8156 is merged (merge commit `32312ae0d6f0`, head `c5d51ae8`). It turns on Miri for the tests that use TCP sockets (`TcpListener`, `TcpStream`, `TcpSocket`) and keeps a per-test ignore only where Miri still can't run them. I read the PR metadata and the full diff. I did not check out the code or run Miri.

## What it changes

- **CI pin:** `.github/workflows/ci.yml` moves `rust_miri_nightly` from `nightly-2026-04-27` to `nightly-2026-05-20`, presumably to pick up Miri's TCP support.
- **Blanket gates removed:** The old `not(miri)` / "No `socket` in miri" gates come off whole test files and doc examples. This covers:
  - `tcp_accept`, `tcp_connect`, `tcp_echo`, `tcp_into_split`, `tcp_into_std`, `tcp_peek`, `tcp_shutdown`, `tcp_socket`, `tcp_split`, `tcp_stream`
  - `io_driver`, `io_driver_drop`, `net_bind_resource`, `no_rt`, `rt_common`, `rt_handle_block_on`, `rt_threaded`, `io_copy_bidirectional`
  - the `TcpListener`, `TcpSocket` and `TcpStream` doc examples
- **Slow tests:** The PR description says tests with tiny buffers or many iterations were too slow under Miri.
  - Ones it judged important got a smaller buffer or iteration count. For example, `tcp_echo.rs` uses `#[cfg(miri)]` for a lower iteration count.
  - The rest got `#[cfg_attr(miri, ignore)] // Too slow on miri`, mainly in `rt_common.rs` and `rt_threaded.rs`.
- **Comment rewrites:** The vague "No `socket` in miri" comments are rewritten to state the real reason, such as "No UDP sockets" or "No Unix domain sockets".
- **Incidental:** The `DEFAULT_BUF_SIZE` URL comment in `tokio/src/io/util/mod.rs` is updated.

## Networking tests still not run under Miri, and why

**TCP tests, open Miri issues** (named in the PR description):
- `readv` can leak memory when it returns an error (rust-lang/miri#5054). In `tcp_stream.rs`, a test is gated with `#[cfg(not(miri))]` and this comment.
- Host I/O events are only processed when switching into the scheduler (rust-lang/miri#5047). `io_copy_bidirectional.rs::blocking_one_side_does_not_block_other` is ignored for this, and so are two tests in `rt_common.rs`.
- The PR author says they will open follow-up PRs once those Miri fixes land in a nightly.

**TCP socket options Miri doesn't support:**
- `SO_LINGER`: ignored in `tcp_shutdown.rs`, `tcp_stream.rs`, and three places in `tcp_socket.rs`.
- Binding a `TcpSocket` before connecting: ignored in `tcp_socket.rs`.
- Setting keepalive, setting reuseport, and setting the send and receive buffer sizes: each ignored in `tcp_socket.rs`.
- Reading reuseaddr: ignored in `tcp_socket.rs`.
- `TCP_NODELAY` on unconnected sockets: Miri only supports it on connected sockets.
- TClass and TOS: gated in `tcp_socket.rs`.
- TCP quickack: `net_quickack.rs` stays `cfg(not(miri))`.

**Other reasons:**
- Leaked threads: three `rt_common.rs` tests are ignored with "Miri detects leaked threads (see #7010)".
- Too slow: several `rt_common.rs` and `rt_threaded.rs` tests.

**Non-TCP networking tests, unchanged by this PR:**
- UDP: `tokio/tests/udp.rs`, `tokio-util/tests/udp.rs`, the UDP test in `net_panic.rs`, and some `rt_handle_block_on.rs` tests. The comment is "No UDP sockets in miri".
- Unix domain sockets: the `uds_datagram`, `uds_socket`, `uds_split` and `uds_stream` tests, plus the Unix cases in `net_panic.rs`.
- `uds_cred.rs`: no `getsockopt` for Unix domain sockets.
- `net_unix_pipe.rs::fifo_resilient_reader`: no `mkfifo`.

## Uncertainty

- I read the exact ignore reasons from the diff. I did not open the linked Miri issues to check whether #5047 or #5054 has been fixed since.
- I did not count every ignored test, so the lists above are not exhaustive.
- The wording "Miri doesn't support X" is the PR's own comments, not something I confirmed in Miri.