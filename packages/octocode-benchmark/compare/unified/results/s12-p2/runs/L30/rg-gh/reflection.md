1. **Helped:** The `gh api -X GET repos/tokio-rs/tokio/pulls/8546/files` call returned the full patch, so I had every hunk in one call. The `grep -nE` on `block.rs` and `list.rs` then gave exact line numbers for the unpatched code. Reading `.git/refs/heads/master` confirmed the checkout was at `facc6fc47e`.

2. **Did not help:** Every Bash call printed `/dev/null: Operation not permitted`, which is sandbox noise. My first `git log` failed for the same reason. `rg` wasn't installed, so I had to redo the search with `grep`. I never confirmed that `facc6fc47e` predates the merge `8513291fda`. I inferred it from the unpatched code. I also didn't verify `block::start_index` or `Rx::reclaim_blocks`'s signature in the checkout.

3. **Next time:** Start with `grep` instead of `rg`. I'd run `gh api repos/tokio-rs/tokio/compare/facc6fc47e...8513291fda` to check the ordering of the two commits directly.

4. **Confidence:** High that the bug is present, because the lines match the PR's "before" hunks. Medium on the 32-bit claim, which comes only from the PR description, and on the "predates the merge" statement, which I didn't check directly.