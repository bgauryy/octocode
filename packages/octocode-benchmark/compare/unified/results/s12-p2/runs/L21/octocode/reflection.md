**Helped:**
- The first `localSearch` (regex `fn begin_shutdown|\.begin_shutdown\(` over `runtime/`) found every definition and call site in one call. Its `hints.read` pointed at the right files.
- The batched `localFetch` with line ranges across `pool.rs` and `sharded.rs` returned the doc comments and bodies I needed. Two calls were enough.

**Did not help:**
- `localFetch` elided the middle ranges (191-304, 361-569, 586-734), which was fine here. But I never saw `drain_and_seal`, and I never checked how `InnerImpl` is chosen at runtime. I flagged both as gaps.
- I did not use `lspSearch` to confirm the call graph. I relied on text matches.

**A mistake in my answer:** I said there were five `begin_shutdown` functions. My own search shows four definitions: `pool.rs:156`, `pool.rs:576`, `pool.rs:740` and `sharded.rs:336`. The order and behavior I described are unaffected, but the count was wrong and should be corrected to four.

**Next time:** I would count the definitions explicitly before writing the number. I would also run one more `localSearch` or `lspSearch` for `drain_and_seal` and the `InnerImpl` selection, so the stated uncertainty is smaller.

**Confidence:** High on the order and per-function behavior, since I read the bodies directly. Medium-high overall, because of the miscount and the two unread pieces.