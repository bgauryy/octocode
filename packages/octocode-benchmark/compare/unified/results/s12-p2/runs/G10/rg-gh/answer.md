**Summary:** PR #8156 is merged (merge commit `32312ae`, head `c5d51ae`). It removes the blanket "no `socket` in Miri" gating from tests and docs that use TCP sockets, so they now run under Miri. It touches 37 files. Some TCP tests are still skipped for specific reasons. I read the PR diff and description but did not check out the repo. Where I name a file below, I inferred it from the diff's file order, so treat those as likely rather than certain.

**What it changes**
- **CI:** it bumps the pinned Miri nightly from `nightly-2026-04-27` to `nightly-2026-05-20` (`.github/workflows/ci.yml`).
- **Gates removed:** it drops `#![cfg(not(miri))]` and `#[cfg_attr(miri, ignore)] // No socket` from the TCP tests. These include `tcp_accept`, `tcp_connect`, `tcp_echo`, `tcp_into_split`, `tcp_into_std`, `tcp_peek`, `tcp_shutdown`, `tcp_split`, `tcp_stream`, `tcp_socket`, `io_driver`, `io_driver_drop`, `net_bind_resource`, `no_rt`, `rt_common`, `rt_threaded` and `rt_handle_block_on`.
- **Doc-tests:** the `if cfg!(miri) { return Ok(()); }` guards are removed from the TCP doc-tests in `tcp/{listener,socket,stream}.rs`.
- **Comment cleanup:** the remaining UDP and Unix-socket guards now say what is actually unsupported, for example "No UDP sockets in miri" and "No Unix domain sockets in miri".
- **Slow tests:** the author kept some important tests by shrinking buffers or iteration counts. `tcp_echo` gets a `#[cfg(miri)]` lower iteration count. Other tests are marked `#[cfg_attr(miri, ignore)] // Too slow on miri`, including some in `rt_common` and `rt_threaded`.

**Networking tests still skipped under Miri, and why**
- **Open Miri issues**, which the PR description says will be follow-ups:
  - **`readv` leak:** `readv` leaks memory when it returns an error (rust-lang/miri#5054). A `tcp_stream` test is `#[cfg(not(miri))]` because of this.
  - **Host I/O events:** Miri only processes host I/O events when switching into the scheduler (rust-lang/miri#5047). Tests ignored for this include one in `io_copy_bidirectional` and a couple in `rt_common`.
- **Unsupported socket options or operations in Miri:**
  - `SO_LINGER` is unsupported, which affects `tcp_socket`, `tcp_stream` and `tcp_shutdown`.
  - Binding before connecting is unsupported.
  - `TCP_NODELAY` is only supported on connected sockets.
  - TClass and TOS are unsupported (`tcp_socket`).
  - TCP quickack is unsupported, so all of `net_quickack.rs` stays `cfg(not(miri))`.
- **Other host features missing in Miri:**
  - `getaddrinfo` is not supported, so `net_lookup_host` stays ignored. I did not see a diff hunk for it, so this is from the original comment.
  - `mkfifo` is not supported, which affects `net_unix_pipe`.
- **UDP and Unix domain sockets:** these are still unsupported in Miri, so `udp`, `uds_*`, `tokio-util/tests/udp.rs` and the UDP/UDS doc-tests are still skipped.
- **Leaked threads:** three `rt_common` tests are ignored with "Miri detects leaked threads (see #7010)".
- **Too slow:** several tests are skipped purely for speed, as noted above.

**Uncertainty:** I did not read every hunk in full. In particular, I did not check which individual `tcp_socket` and `tcp_stream` tests carry each `SO_LINGER` or `TCP_NODELAY` ignore. The full list of remaining skips is whatever still carries a `miri` ignore or `cfg(not(miri))` at `32312ae`.