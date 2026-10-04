1. **Helped:** The first `localSearch` was the best step. One regex over `runtime/task` returned the whole call chain with line numbers: `store_output`, `take_output`, `wake_join`, `complete`, `set_join_waker`, `try_read_output`. The batched `localFetch` with multiple `ranges` then read harness.rs, core.rs and join.rs in one call, so I used only three calls in total.

2. **Did not help:**
   - The `localFetch` output elided large middle spans ("lines 201-274 omitted", "275-539 omitted"). I never saw `poll_inner`'s body or the start of `can_read_output`.
   - The second `localFetch` was meant to fill those gaps, but it still elided most of what I wanted and mostly returned the tail of the file.
   - I did not read `state.rs`, `raw.rs` wrappers or `task/mod.rs`, so the ordering guarantees rest on comments I saw inside harness.rs, not on the state transitions themselves.

3. **Next time:** I'd request narrower ranges, such as 200-224 and 395-425, so the tool does not elide them. I'd also add `state.rs` `transition_to_complete` and `set_join_waker` to the batch, and open the `task/mod.rs` safety rules.

4. **Confidence:** Medium-high. The store, wake and read path is directly quoted with line numbers. The synchronization details are less verified, and I flagged that in the answer.