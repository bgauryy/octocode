1. **Helped:** the second Bash call, a `grep -nE` over `core.rs`, `harness.rs`, `state.rs` and `join.rs`. It located every relevant function in one pass. The next call, a batch of `sed -n` ranges, gave the real code for `poll_future`, `store_output`, `take_output`, `complete`, `can_read_output` and `JoinHandle::poll`. That was nearly the whole answer.

2. **Did not help:**
   - `rg` is not installed, so the first call was wasted; I should have used `grep` from the start.
   - Every Bash call printed `/dev/null: Operation not permitted`, which is noise. It also made the `git log -1` check fail silently, so I never confirmed the checkout is at facc6fc47e.
   - I never opened `task/mod.rs` or `state.rs` bodies.

3. **Next time:** use `grep -n` first. Print line-numbered output with `cat -n` or `grep -n` on the exact lines I cite, instead of inferring numbers from `sed` offsets. Read the `state.rs` transitions and the `mod.rs` rules.

4. **Confidence:** high on the mechanism, since I saw the code for every step. Medium on the exact line numbers. Several were estimated from `sed` offsets, not printed: `join.rs:335-346` and `harness.rs:339-343` are approximate, and `drop_join_handle_slow` is probably at line 287, not the 289 I gave. The numbers I took directly from `grep -n` output are exact.