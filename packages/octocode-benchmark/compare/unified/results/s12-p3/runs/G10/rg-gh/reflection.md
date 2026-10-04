1. **Helped:** The first Bash call, `gh api pulls/8156` plus `pulls/8156/files` piped through `jq`, returned the PR body and every file patch at once. The body states the motivation and the two open Miri issues. My last Bash call filtered the saved patch with `awk '/^\+.*miri/'`. That listed the added Miri markers with their reasons.

2. **Did not help:**
   - The output was too large, so it was saved to a file.
   - My second call, a `grep` with a convoluted regex, mostly returned noisy removed "No `socket`" lines.
   - Every Bash call printed `/dev/null: Operation not permitted`, which was harmless noise.
   - I only saw diff hunks, not the final files at the merge commit. I never checked the `tcp_socket.rs` macro-wrapped ignores or what replaced the `net_lookup_host.rs` marker.
   - I never looked at the PR's comments or review discussion.

3. **Next time:** Filter the added lines per file right away, with `awk` on `^+` lines. Fetch `tcp_socket.rs` at `32312ae` through the contents API to confirm the multi-line attributes. Check the PR comments and the merge commit message.

4. **Confidence:** Medium-high. The list of ignore reasons comes straight from the diff. The mapping of reasons to individual test names is partly inferred, and the `net_lookup_host.rs` status is unverified.