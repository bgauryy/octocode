**Summary:** PR #8156 (merged, author WhySoBad, merge commit `32312ae0d6f0b1c6457f1323e3e7f568f448d0db`) removes the old "no `socket` in Miri" blanket exclusions for TCP. Miri now supports TCP sockets, so the PR turns those tests back on. It touches 37 files (+165/−155) and bumps the pinned Miri nightly from `nightly-2026-04-27` to `nightly-2026-05-20` in `.github/workflows/ci.yml`. I read the patches filtered to lines containing "miri", so I may have missed changes that don't mention it.

**What it changes**
- **Enabled TCP test files:** the `not(miri)` gates are dropped from the file-level `#![cfg]` in `tcp_accept`, `tcp_connect`, `tcp_echo`, `tcp_into_split`, `tcp_into_std`, `tcp_peek`, `tcp_shutdown`, `tcp_socket`, `tcp_split`, `tcp_stream`, `net_bind_resource`, `rt_common`, and `rt_threaded`. The same gates are also dropped from `rt_handle_block_on`.
- **Per-test ignores:** many `#[cfg_attr(miri, ignore)] // No socket` attributes are removed. Examples are `io_driver`, `io_driver_drop`, `no_rt`, `net_panic`, `io_copy_bidirectional`, and most of `tcp_stream.rs`.
- **Doctests:** the `if cfg!(miri) { return Ok(()); }` guards are removed from the TCP doctests in `tokio/src/net/tcp/{listener,socket,stream}.rs`.
- **Comment rewording:** the generic "No `socket`" comments are replaced with specific reasons. UDP, Unix domain socket and `mkfifo` tests stay ignored. `udp.rs`, `net_panic`, `uds_*`, and the `UnixDatagram` and `UnixStream` doctests still skip Miri.
- **Slow tests:** `tcp_echo` lowers `ITER` from 1024 to 32 under Miri. Several runtime tests are newly ignored because they are too slow under Miri: `spawn_many_*`, `ping_pong_saturation`, `shutdown_concurrent_spawn`, `many_*_futures`, `drop_threadpool_drops_futures`, `test_tuning`.

**Networking tests still not run under Miri, and why**
- **Miri issue #5047 (host I/O events are only processed when switching into the scheduler):**
  - `blocking_one_side_does_not_block_other` in `io_copy_bidirectional.rs`.
  - `local_set_client_server_block_on` and `client_server_local` in `rt_common.rs`.
- **Miri PR #5054 (`readv` can leak memory when it returns an error):** the `readv` error block in `tcp_stream.rs::try_read_write` is compiled out with `#[cfg(not(miri))]`. The rest of that test still runs.
- **`tcp_socket.rs` unsupported socket options:**
  - Binding before connecting (`bind_before_connect`).
  - `SO_LINGER` (`basic_linger` and the linger option test).
  - Setting keepalive.
  - Reading reuseaddr.
  - Setting reuseport.
  - Setting the send and receive buffer sizes.
  - `TCP_NODELAY` on unconnected sockets. Miri only supports it on connected sockets.
  - TClass (`tclass_v6`) and TOS are excluded.
- **`SO_LINGER` elsewhere:** `tcp_shutdown::shutdown_after_tcp_reset` and `tcp_stream::set_linger`.
- **Other excluded files and tests:**
  - `net_quickack.rs` is still `cfg(not(miri))` because Miri doesn't support the TCP quickack option.
  - `udp.rs` in `tokio-util` and `udp_socket_bind` are excluded because there are no UDP sockets in Miri.
  - Unix domain socket tests are excluded because there are no Unix domain sockets in Miri.
  - `fifo_resilient_reader` in `net_unix_pipe` is excluded because there is no `mkfifo`.
- **`net_lookup_host::resolve_dns`:** the PR removes its ignore. I did not check why the `getaddrinfo` ignore could be dropped.
- **Miri issue #7010 (leaked threads):** `shutdown_timeout`, `shutdown_timeout_0` and `shutdown_wakeup_time` are ignored. These are runtime tests, not networking tests.

**Uncertainty**
- The PR body says follow-up PRs will re-enable the #5047 and #5054 cases once Miri fixes land in a nightly. I did not check whether those fixes have landed.
- I didn't read the final file contents at the merge commit, only the patches. A few `tcp_socket.rs` hunks were truncated in the output (for example the `tclass_v6` and TOS changes). Exact line numbers in the merged files are therefore not given.