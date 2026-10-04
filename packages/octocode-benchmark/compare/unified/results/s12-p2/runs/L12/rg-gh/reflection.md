1. **Helped:** The second Bash call, a `grep -rn -i "lifo"` over `multi_thread/*.rs` and `config.rs`, mapped every relevant site in one pass. The third call, a batch of `sed -n` reads, gave the key code with line numbers. Together they were enough to answer.

2. **Did not help:**
   - The first Bash call failed because `rg` isn't installed, though the prompt suggested it. I fell back to grep.
   - Every Bash call printed `/dev/null: Operation not permitted`. It was noise, but it blocked `git log`, so I couldn't confirm the checkout was at the pinned commit.
   - I never read the poll site around `worker.rs:770-797` or the builder docs.
   - I also didn't use the dedicated file-read and search tools the harness prefers. I used shell `sed` and `grep` throughout.

3. **Next time:** I'd check tool availability first. I'd use the dedicated read and grep tools instead of shell commands. I'd also read the unread range at `worker.rs:770-797` and the builder docs on the LIFO option, and cross-check the pinned commit with `gh api` instead of local git.

4. **Confidence:** Medium-high. The core mechanism and limits come straight from code I read, with line numbers. The unverified commit pin and the unread lines are the gaps. I flagged both in the answer.