**Answer:** PR #8156 is merged (merge commit `32312ae0`, base `c6d58ce7`). It stops blanket-disabling TCP-socket tests under Miri, because Miri now supports TCP sockets. It touches 37 files. Tests that still don't run under Miri are skipped for a specific reason, listed below.

**What it changes** (from the PR diff, so line numbers are in the patch, not in the repo):
- **Un-ignores TCP tests.** It removes the old "No `socket` in miri" ignores and `not(miri)` cfgs from the TCP tests: `tcp_accept`, `tcp_connect`, `tcp_echo`, `tcp_into_split`, `tcp_into_std`, `tcp_peek`, `tcp_split`, `tcp_shutdown`, `tcp_socket` and `tcp_stream`. It does the same for `io_driver`, `io_driver_drop`, `net_bind_resource`, `no_rt`, `io_copy_bidirectional` and `net_lookup_host`, and for much of `rt_common`, `rt_handle_block_on` and `rt_threaded`.
- **Doc tests.** It removes the `if cfg!(miri) { return Ok(()); }` guards from the TCP doc examples in `net/tcp/listener.rs`, `socket.rs` and `stream.rs`.
- **Slow tests.** `tcp_echo` now uses a lower iteration count under Miri. Several `rt_common` and `rt_threaded` tests get `#[cfg_attr(miri, ignore)] // Too slow on miri`. The PR body says small buffers and many iterations made some tests very slow, so it shrank buffers or iterations for tests it considered important and ignored the rest.
- **Comments and CI.** It reworded the remaining ignore comments for accuracy, for example "No UDP sockets" and "No Unix domain sockets". It also bumped `rust_miri_nightly` from `nightly-2026-04-27` to `nightly-2026-05-20` in `.github/workflows/ci.yml`.

**Networking tests that still don't run under Miri, and why:**
- **Missing Miri support for TCP socket options** (`tcp_socket.rs`, `tcp_shutdown.rs`, `tcp_stream.rs`):
  - `SO_LINGER`
  - binding before connecting
  - setting keepalive
  - reading reuseaddr
  - setting reuseport
  - setting the send and receive buffer sizes
  - `TCP_NODELAY` on unconnected sockets (Miri only supports it on connected sockets)
  - TClass
  - TOS
  - `net_quickack.rs`, because of the quickack option
- **Open Miri issues**, which the PR body says it will follow up once fixed and landed in a nightly:
  - Host I/O events are only processed when switching into the scheduler (rust-lang/miri#5047). This ignores one test in `io_copy_bidirectional.rs` and two in `rt_common.rs`.
  - `readv` can leak memory when it returns an error (rust-lang/miri#5054). A `tcp_stream.rs` test is `#[cfg(not(miri))]` for this.
- **Still unsupported by Miri:**
  - UDP: `udp.rs`, `tokio-util/tests/udp.rs`, and UDP cases in `net_panic` and `rt_common`.
  - Unix domain sockets: `uds_*`, plus the related cases in `net_panic` and `rt_handle_block_on`.
  - `mkfifo` for `net_unix_pipe`.
  - `getsockopt` for UDS credentials in `uds_cred`.
- **Unrelated to sockets:** the `rt_common` tests that leak threads (Miri detects leaked threads, tokio issue #7010).

**Uncertainty:** I only read the PR's file patches, not the repo at the merge commit. The list of remaining ignores comes from the diff's added lines, so I may have missed pre-existing ignores in files the PR didn't touch. I didn't check whether the two Miri issues have since been fixed.