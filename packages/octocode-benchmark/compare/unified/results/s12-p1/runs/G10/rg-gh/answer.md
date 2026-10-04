**Short answer:** PR #8156 is merged (merge commit `32312ae0d6f0b1c6457f1323e3e7f568f448d0db`). It removes the blanket "no `socket` in Miri" skips from the TCP tests and doctests, so they now run under Miri. It also bumps the Miri nightly in CI. Tests that Miri still can't run stay skipped, each with a more specific reason. I read this from the PR metadata and file patches via the API, not from a checkout, so I give file names rather than line numbers.

**What it changes** (36 files)
- **CI:** `.github/workflows/ci.yml` bumps `rust_miri_nightly` from `nightly-2026-04-27` to `nightly-2026-05-20`.
- **TCP tests:** it drops the `not(miri)` gates and `cfg_attr(miri, ignore)` attributes. This covers `tcp_accept`, `tcp_connect`, `tcp_echo`, `tcp_into_split`, `tcp_into_std`, `tcp_peek`, `tcp_split`, `tcp_stream`, `tcp_socket`, `tcp_shutdown`, `io_driver`, `io_driver_drop`, `net_bind_resource`, `net_lookup_host`, `no_rt`, `rt_common` and `rt_handle_block_on`.
- **Doctests:** it removes the `if cfg!(miri) { return Ok(()); }` guards from the TCP doctests in `tcp/listener.rs`, `tcp/socket.rs` and `tcp/stream.rs`.
- **Slow tests:** the PR description says some tests used very small buffers or many iterations. Important ones had their sizes or iteration counts reduced, for example `tcp_echo` has a lower iteration count under Miri. The rest got `#[cfg_attr(miri, ignore)] // Too slow on miri`, mainly in `rt_common` and `rt_threaded`.
- **Comment rewording:** the generic "No `socket`" comments on UDP and Unix-socket skips now name the real reason ("No UDP sockets", "No Unix domain sockets").

**Networking tests that still don't run under Miri, and why**
- **UDP:** `udp.rs` in both `tokio` and `tokio-util`, the UDP doctests, and the UDP cases in `net_panic`, `rt_common` and `rt_handle_block_on` are skipped because Miri has no UDP sockets.
- **Unix domain sockets:** `uds_datagram`, `uds_socket`, `uds_split`, `uds_stream`, the UDS doctests, and the UDS cases in `net_panic`, `rt_common` and `rt_handle_block_on` are skipped because Miri has no Unix domain sockets.
- **Unix socket credentials:** `uds_cred` is skipped because Miri has no `getsockopt` for Unix domain sockets.
- **Named pipes:** `net_unix_pipe` is skipped because Miri has no `mkfifo`.
- **TCP quickack:** `net_quickack` is skipped because Miri doesn't support that socket option.
- **`SO_LINGER`:** tests in `tcp_shutdown`, `tcp_stream` and `tcp_socket` are skipped because Miri doesn't support it.
- **Other TCP socket options in `tcp_socket`:**
  - binding before connecting
  - setting keepalive
  - reading `SO_REUSEADDR`
  - setting `SO_REUSEPORT`
  - setting the send and receive buffer sizes
  - `TCP_NODELAY` on unconnected sockets (Miri only supports it on connected ones)
  - TClass
  - TOS
- **Miri bug, `readv` leak (tcp_stream):** a `tcp_stream` test is excluded because Miri leaks memory when `readv` returns an error (rust-lang/miri#5054).
- **Miri bug, host I/O events:** `io_copy_bidirectional` and two `rt_common` tests are ignored because Miri only processes host I/O events when it enters the scheduler (rust-lang/miri#5047).
- **Leaked threads:** three `rt_common` tests are ignored because Miri detects leaked threads (tokio issue #7010).
- **Too slow:** several `rt_common` and `rt_threaded` tests are ignored for speed.

The PR author says they will open follow-up PRs for the two Miri-bug cases once the fixes land in a nightly.

**Uncertainty:** I filtered the diffs for lines containing "miri" or "ignore" and truncated the output. I did not open the files at the merge commit, so I can't confirm which skips remain at the current head of master. The exact split of cases between UDP, UDS and TCP in `net_panic`, `rt_common` and `rt_handle_block_on` is inferred from the comment text, not checked test by test.