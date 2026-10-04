1. **Helped:** The `grep -n -i "lifo" worker.rs` call gave a full map of the relevant lines in one pass. The next call, several `sed -n` ranges plus a `grep` of `builder.rs`, printed the key code in one go. That covered the slot definition, the 3-poll cap, the budget check, `schedule_local` and the config flag, so two substantive calls were enough.

2. **Did not help:**
   - The first call failed because `rg` is not installed, so I had to fall back to `grep`.
   - Every Bash call printed `/dev/null: Operation not permitted`. That noise made `git rev-parse HEAD` fail with a fatal error.
   - **I never verified that the checkout is at facc6fc47e.** My answer cites line numbers as if they were at that commit, but I only saw the working tree. The answer's uncertainty section did not mention this, and it should have.
   - I did not read `Context::run`, the park path or `steal.rs`. I also skipped the tests and git history, such as the PR that introduced the LIFO cap.

3. **Next time:**
   - Use `grep` from the start.
   - Confirm the commit another way, for example by reading `.git/HEAD` or using `gh api` against the pinned SHA.
   - Spot-check the cap and `lifo_enabled` handling against history.

4. **Confidence:** Medium-high on the mechanism, because I read the code directly. Medium on the line numbers, since the commit pin is unconfirmed. The claim that wake-ups are notified only when the slot was already occupied is the weakest part, since I did not trace the park paths.