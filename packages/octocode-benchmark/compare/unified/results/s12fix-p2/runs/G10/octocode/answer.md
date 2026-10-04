I read the PR body and the first page of the diff (37 files, merged 2026-05-26, head `c5d51ae`). I did not read the second page of changed files or the rest of the `rt_common.rs` patch. I checked the remaining ignores against the default branch (`5d5cd8b`), which is later than the PR, so the "still ignored" list below may include changes made after the PR.

## What the PR changes

Miri now supports TCP sockets, so tokio no longer needs to skip TCP tests under it. The PR body says this (merged PR #8156).

- **Ignores removed.** The `// No `socket` in miri` markers are gone:
  - `#[cfg_attr(miri, ignore)]` in `io_copy_bidirectional.rs`, `io_driver.rs`, `io_driver_drop.rs`, `net_bind_resource.rs`, `net_panic.rs` (TCP tests), `no_rt.rs` and `net_lookup_host.rs`.
  - The crate-level `#![cfg(not(miri))]` in `net_bind_resource.rs` and `rt_common.rs`.
  - The `if cfg!(miri) { return Ok(()); }` guards in the doctests for `TcpListener`, `TcpSocket` and `TcpStream`.
- **Slow tests.** Tests with tiny buffers or many iterations were slow under Miri. The body says important ones got smaller buffer sizes or iteration counts. The rest got `#[cfg_attr(miri, ignore)] // Too slow on miri`, for example `spawn_many_from_block_on` and `spawn_many_from_task` in `rt_common.rs`.
- **Pinned Miri nightly.** Bumped from `nightly-2026-04-27` to `nightly-2026-05-20` in `.github/workflows/ci.yml`.
- **Comment fixes.** The vague "No `socket` in miri" comments now say what is actually missing: "No UDP sockets", "No Unix domain sockets", "No `mkfifo`" for `net_unix_pipe.rs`, and "doesn't support TCP quickack socket option" for `net_quickack.rs`. One stale `std` source link in `tokio/src/io/util/mod.rs` was also updated.
- **Other files.** I did not read the diffs for `tcp_*.rs`, `rt_threaded.rs` and `rt_handle_block_on.rs`.

## Networking tests that still don't run under Miri

**TCP tests the PR body says are blocked by open Miri issues:**
- `readv` can leak memory (rust-lang/miri PR 5054). I couldn't tell which tests this affects.
- `blocking_one_side_does_not_block_other` in `io_copy_bidirectional.rs` is still ignored. Its comment says "Miri currently only processes host I/O events when switching into the scheduler" (rust-lang/miri issue 5047).
- The body says follow-up PRs will come once these are fixed in Miri and land in a nightly.

**Other TCP cases:**
- `net_quickack.rs` is `cfg(not(miri))` because Miri doesn't support the TCP quickack option.
- In `tcp_socket.rs`, the `tclass_v6` test is excluded because Miri doesn't support TClass.
- Tests marked "Too slow on miri" are skipped on purpose, not because Miri lacks a feature.

**Non-TCP networking tests, still skipped:**
- UDP, e.g. the UDP `net_panic.rs` test, `coop_budget.rs`, `udp_stream_bind_after_shutdown` in `rt_handle_block_on.rs`, and `tokio-util/tests/udp.rs`.
- Unix domain sockets, including `uds_*`, `unix_shutdown.rs` and the `UnixDatagram` doctests. `uds_datagram.rs` also has a test ignored for "No SOCK_DGRAM for `socketpair`".
- `net_unix_pipe.rs`: `fifo_resilient_reader` (no `mkfifo`) and `anon_pipe_spawn_echo` (no `pidfd_spawnp`).

## Uncertainty

- A few default-branch files still carry the old "No `socket`" comment, for example `buffered.rs`, `coop_budget.rs`, `io_async_fd.rs` and `rt_shutdown_err.rs`. I did not check whether they involve TCP, or whether they predate or postdate the PR.
- I did not find the Miri issue or PR links in the test sources through code search. Those two links come from the PR body and the `io_copy_bidirectional.rs` diff only.