**Helped:** The first `localSearch` (regex `fn begin_shutdown|\.begin_shutdown\(`) found every definition and call site across `tokio/src` in one call. The next step was one `localFetch` with several line ranges on `pool.rs`, plus a second on `sharded.rs`, run in parallel. Together they gave the exact bodies and doc comments with line numbers. Two rounds were enough.

**Did not help:** `localFetch` with multiple ranges elided the gaps (e.g. 186-304), so I could not see the surrounding structure. Nothing errored. I never ran `lspSearch` or `astSearch` to confirm the call graph. I also never opened `drain_and_seal`.

**Next time:** I would count the search hits before writing the summary.

**Correction to my answer:** I miscounted. The search shows four `begin_shutdown` definitions, not five: `pool.rs:156`, `pool.rs:576`, `pool.rs:740` and `sharded.rs:336`. A single shutdown runs three of them, not four: the `InnerImpl` dispatcher, one of Locked or Sharded, and `ThreadManagementState`. `BlockingPool::shutdown` is the caller and not a `begin_shutdown`. The per-function descriptions and the call order are otherwise accurate.

**Confidence:** High on the behaviors and line citations, because I read the bytes. The count in my first summary was wrong, as stated above.