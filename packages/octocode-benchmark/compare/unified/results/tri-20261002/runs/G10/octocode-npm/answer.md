**Summary:** PR #8156 was merged on 2026-05-26 (+165/−155, 37 files, author WhySoBad). Miri now supports TCP sockets, so the PR removes the blanket "No `socket` in miri" skips from tokio's TCP tests and doctests. Tests that Miri still can't run are ignored individually, each with its own reason. I read the diff only; I did not check the merged files or run the tests.

## What it changes

- **CI:** `.github/workflows/ci.yml` bumps `rust_miri_nightly` from `nightly-2026-04-27` to `nightly-2026-05-20`.
- **Whole-file skips removed:** `not(miri)` or `cfg_attr(miri, ignore)` is dropped from these files:
  - `tcp_accept`, `tcp_connect`, `tcp_echo`, `tcp_into_split`, `tcp_into_std`, `tcp_peek`, `tcp_shutdown`, `tcp_socket`, `tcp_split` and `tcp_stream`
  - `net_bind_resource`, `io_driver`, `io_driver_drop`, `no_rt`, `rt_handle_block_on`, `rt_threaded` and `rt_common`
  - Several of these gate a whole file, so their other tests also start running under Miri.
- **Slow tests shrunk:**
  - `tcp_echo` drops `ITER` from 1024 to 32 under Miri only, via `#[cfg(miri)]`. A reviewer (Darksonn) asked for the `cfg` approach rather than lowering it everywhere.
  - `tcp_stream` changes the `try_read_write` and `try_read_buf` data from a 40-byte string to `[2u8; 4000]`.
- **Slow tests ignored:** `rt_common` and `rt_threaded` get `#[cfg_attr(miri, ignore)] // Too slow on miri`. These include `spawn_many_*`, `ping_pong_saturation`, `shutdown_concurrent_spawn`, `many_oneshot_futures`, `many_multishot_futures`, `drop_threadpool_drops_futures`, `blocking` and `test_tuning`.
- **Doctests:** the `if cfg!(miri) { return Ok(()); }` guards are removed from the `TcpListener`, `TcpSocket` and `TcpStream` docs.
- **Comments:** the generic "No `socket` in miri" comments are replaced with specific reasons ("No UDP sockets", "No Unix domain sockets", "No `mkfifo`").
- **Cleanup:** an outdated std link comment in `io/util/mod.rs` is fixed.

## Networking tests still not run under Miri, and why

**Unsupported Miri features (as stated in the diff comments):**
- **`SO_LINGER`:** `shutdown_after_tcp_reset`, `basic_linger`, the `linger` test in `tcp_socket`, and `set_linger` in `tcp_stream`.
- **Binding before connect:** `bind_before_connect`.
- **Socket options in `tcp_socket.rs`:**
  - Setting keepalive.
  - Reading reuseaddr.
  - Setting reuseport.
  - Setting the send and receive buffer sizes.
  - `nodelay`, which Miri supports only on connected sockets.
  - `tclass_v6` and `tos_v4`, which are compiled out under Miri.
- **Quickack:** `net_quickack.rs` is excluded because Miri doesn't support the TCP quickack option.
- **UDP:** `udp.rs`, `tokio-util/tests/udp.rs`, `udp_socket_from_std_panic_caller`, `udp_socket_bind` and `io_notify_while_shutting_down` ("No UDP sockets").
- **Unix domain sockets:** `uds_*` tests and the `net_panic` and `rt_handle_block_on` unix cases. `uds_cred` is excluded because Miri has no `getsockopt` for Unix domain sockets.
- **FIFOs:** `fifo_resilient_reader` in `net_unix_pipe` ("No `mkfifo`").

**Miri bugs, with open upstream issues or PRs (the PR body says follow-up PRs will come once these land in a nightly):**
- **Vectored I/O leak:** the vectored-I/O section of `tcp_stream::try_read_write` is wrapped in `#[cfg(not(miri))]`, so the rest of the test still runs. `readv` can leak memory when it returns an error (rust-lang/miri#5054).
- **Host I/O events:** Miri processes them only when switching into the scheduler (rust-lang/miri#5047). That ignores `blocking_one_side_does_not_block_other` in `io_copy_bidirectional` and `local_set_client_server_block_on` in `rt_common`. The `#[cfg_attr(miri, ignore)]` is also on `client_server_local`, which is a helper fn, not a test.

**Other reasons:**
- **Too slow:** the `rt_common` and `rt_threaded` tests listed above.
- **Leaked threads:** `shutdown_timeout`, `shutdown_timeout_0` and `shutdown_wakeup_time` ("Miri detects leaked threads (see #7010)").

**Uncertainty:** `resolve_dns` in `net_lookup_host` had its Miri ignore removed ("No `getaddrinfo`"). I didn't check whether it runs under Miri or why the PR enabled it.