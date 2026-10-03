**Summary.** PR #8156 (merged 2026-05-20, merge commit `32312ae0`, author WhySoBad) removes the "no `socket` in Miri" exclusions from the TCP tests. Miri now supports TCP sockets, so most TCP tests, docs and test files run under Miri. Some tests stay ignored, each for a narrower reason. I read the patches for the test files and the changed source and CI files. I did not read the second page of the changed-file list (the PR has 36–37 files), so the list below could be missing a file.

## What it changes
- **CI:** bumps the pinned Miri nightly from `nightly-2026-04-27` to `nightly-2026-05-20` (`.github/workflows/ci.yml`).
- **Test files un-gated:** it drops `not(miri)` file-level gates and per-test `#[cfg_attr(miri, ignore)]` in these files:
  - `tcp_accept`, `tcp_connect`, `tcp_echo`, `tcp_into_split`, `tcp_into_std`, `tcp_peek`, `tcp_split`, `tcp_shutdown`, `tcp_socket`, `tcp_stream`
  - `io_driver`, `io_driver_drop`, `no_rt`, `net_bind_resource`, `net_lookup_host`
  - `rt_common`, `rt_threaded`, `rt_handle_block_on`
- **Doc examples:** it removes `if cfg!(miri) { return Ok(()); }` from the `TcpListener`, `TcpSocket` and `TcpStream` examples.
- **Tuned tests:** `tcp_echo` uses `ITER = 32` under Miri instead of 1024. Some slow tests are ignored with "Too slow on miri" instead of being excluded wholesale.
- **Reworded reasons:** UDP and Unix-domain-socket ignores now say "No UDP sockets" or "No Unix domain sockets in miri". Before, they said "No `socket`".

## Tests that still don't run under Miri, and why
**Missing Miri socket features (TCP):**
- `tcp_shutdown::shutdown_after_tcp_reset`, `tcp_socket::basic_linger`, the `tcp_socket` `linger` test and `tcp_stream::set_linger` need `SO_LINGER`.
- `tcp_socket::bind_before_connect`: Miri doesn't support binding before connecting.
- `tcp_socket` `keepalive`, `reuseaddr` (reading it), `reuseport`, `send_buffer_size` and `recv_buffer_size`: Miri doesn't support setting or reading those options.
- `tcp_socket` `nodelay`: Miri only supports `TCP_NODELAY` on connected sockets.
- `tcp_socket` `tclass_v6` is compiled out, and `tos_v4` is excluded under Miri (no TClass or TOS support).
- `net_quickack.rs` is still `cfg(not(miri))`: no TCP quickack option.

**Open Miri issues the PR body names:**
- `readv` can leak memory, per rust-lang/miri PR #5054. One "write buffer full" block in `tcp_stream::try_read_write` is `#[cfg(not(miri))]`.
- Miri only processes host I/O events when switching into the scheduler, per rust-lang/miri issue #5047. This affects:
  - `io_copy_bidirectional::blocking_one_side_does_not_block_other`
  - `rt_common::local_set_client_server_block_on`
  - `rt_common::client_server_local`

  The PR body says follow-up PRs will come once these are fixed and reach a nightly.

**Too slow under Miri:**
- `rt_common`: `spawn_many_from_block_on`, `spawn_many_from_task`, `ping_pong_saturation`, `shutdown_concurrent_spawn`.
- `rt_threaded`: `many_oneshot_futures`, `many_multishot_futures`, `drop_threadpool_drops_futures`, `test_tuning`.

**Leaked threads:** `rt_common` `shutdown_timeout`, `shutdown_timeout_0` and `shutdown_wakeup_time` are ignored because Miri detects leaked threads (#7010).

**Not TCP, and still unsupported:**
- UDP: `tokio-util/tests/udp.rs`, `net_panic::udp_socket_from_std_panic_caller`, `rt_handle_block_on::udp_socket_bind` and the `rt_common` UDP test.
- Unix domain sockets: the `net_panic` `unix_*` tests, `rt_handle_block_on` Unix test, and the `uds_*` tests.
- `net_unix_pipe::fifo_resilient_reader`: no `mkfifo`.

## Uncertainty
- The `tcp_connect`, `tcp_into_std` and `tcp_accept` patches only removed gates. I did not see the full `tcp_stream` diff, only the Miri-related hunks.
- I did not check whether Miri #5047 or #5054 have been fixed since the PR merged.