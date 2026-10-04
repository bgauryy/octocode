PR #8156 (merged 2026-05-26 as `32312ae0`, 37 files, +165/−155) turns the Miri tests back on for TCP sockets. Several TCP tests and all UDP, Unix-socket and FIFO tests still don't run under Miri. I read the full patches for most files but not all of them, so the "still ignored" list below may be incomplete.

**What it changes**
- **Why:** The PR body says Miri has gained TCP socket support over the past months, so tests no longer need the blanket `// No \`socket\` in miri` ignore.
- **CI:** it bumps the pinned Miri nightly from `nightly-2026-04-27` to `nightly-2026-05-20` (`.github/workflows/ci.yml`).
- **Test gates removed:**
  - It drops `#![cfg(not(miri))]` and the `#[cfg_attr(miri, ignore)]` attributes from the TCP-related tests.
  - Files covered include `tcp_echo`, `tcp_socket`, `tcp_stream`, `tcp_shutdown`, `io_driver`, `io_driver_drop`, `net_bind_resource`, `net_panic` (TCP cases), `rt_common`, `rt_threaded` and `io_copy_bidirectional`.
  - It removes the `if cfg!(miri) { return Ok(()); }` guards from the `TcpListener`, `TcpSocket` and `TcpStream` doctests.
- **Slow tests made smaller:**
  - In `tcp_echo.rs`, `ITER` drops from 1024 to 32 under Miri only, via `#[cfg(miri)]`. A reviewer (Darksonn) asked for the `cfg` version rather than lowering it everywhere.
  - In `tcp_stream.rs`, `try_read_write` uses `DATA = &[2u8; 4000]` instead of a short string.
- **Slow tests ignored:** some `rt_common` and `rt_threaded` tests now carry `#[cfg_attr(miri, ignore)] // Too slow on miri`, including `many_oneshot_futures`, `ping_pong_saturation` and `shutdown_concurrent_spawn`.
- **Comments:** it rewords the old "No `socket`" comments to say what is actually missing, such as UDP sockets, Unix domain sockets or `mkfifo`. Examples are `net_unix_pipe.rs` and `udp.rs`.
- **Other:** `net_lookup_host.rs` also loses its ignore on `resolve_dns`.

**Still not run under Miri, and why**

TCP tests, ignored in this PR:
- **Miri bugs, with upstream fixes pending (named in the PR body):**
  - The vectored read/write part of `try_read_write` in `tcp_stream.rs` is compiled out. Miri leaks memory when `readv` returns an error (rust-lang/miri#5054).
  - `blocking_one_side_does_not_block_other` in `io_copy_bidirectional.rs`, `local_set_client_server_block_on` and `client_server_local` in `rt_common.rs` are ignored. Miri only processes host I/O events when switching into the scheduler (rust-lang/miri#5047).
- **Socket options Miri lacks (the `ignore` messages in `tcp_socket.rs`):**
  - `SO_LINGER`: `basic_linger`, the `linger` test, `set_linger` in `tcp_stream.rs`, and `shutdown_after_tcp_reset` in `tcp_shutdown.rs`.
  - Keepalive, reading `reuseaddr`, `reuseport`, send and receive buffer sizes.
  - `nodelay` is only supported on connected sockets.
  - `bind_before_connect`: Miri can't bind before connecting.
  - `tclass_v6` and `tos_v4`.
- **Leaked threads:** `shutdown_timeout`, `shutdown_timeout_0` and `shutdown_wakeup_time` are ignored with "Miri detects leaked threads (see #7010)".
- **Other TCP gates:**
  - `net_quickack.rs` stays `cfg(not(miri))` because Miri doesn't support the TCP quickack option.
  - `tcp_accept_ready.rs` still has `not(miri)` at line 2. I saw this in the post-merge file listing but did not check its reason.
  - `buffered.rs` still has `#[cfg_attr(miri, ignore)] // No \`socket\` on miri.` at line 12. This is probably a leftover, but I didn't confirm what it tests.
  - I did not read the patches for `tcp_accept`, `tcp_connect`, `tcp_peek`, `tcp_split`, `tcp_into_split`, `tcp_into_std`, `no_rt` or `rt_handle_block_on`.

Non-TCP networking, unchanged by this PR:
- **UDP:** `tokio-util/tests/udp.rs`, `udp_socket_from_std_panic_caller` in `net_panic.rs`, and `io_notify_while_shutting_down` in `rt_common.rs`.
- **Unix sockets:** `uds_socket`, `uds_datagram` and the Unix cases in `net_panic.rs`. These are still ignored, and the PR only updated the comments to "No Unix domain sockets in miri".
- **FIFOs:** `net_unix_pipe.rs` stays ignored with "No `mkfifo` in miri".

**Uncertainty**
- The `ignore` reasons come from the post-merge code search (commit `5d5cd8b5`) and the PR diffs. I didn't read the last ~1.2k characters of the `tcp_stream.rs` patch or the 7 changed files on page 2 of the file list.
- The PR body says follow-up PRs will come once the Miri fixes land in a nightly, so the ignored-test list may have shrunk since.