**1. Helped:**
- The first `localSearch` for `keep_stack` in `include/` went straight to `json_sax.hpp` and gave line anchors for the whole callback state machine.
- Two parallel `localFetch` line-range reads, at 515–720 and 990–1110, covered the start, key and end handlers and `handle_value`.
- Follow-up reads of `remove_discarded_value` and the `is_discarded` hit in `parser.hpp:130` closed the loop.

**2. Did not help:**
- I never read the array-side handlers (`end_array`, about lines 720–810). I only saw their `remove_discarded_value` call at line 807.
- I did not read `resolve_duplicate_key_stash` or `duplicate_key_stash`, so my description of duplicate-key restoration rests on comments and call sites.
- The first search returned only 10 of its 20 matches, and I did not fetch the second page.
- My summary was worded more confidently than that partial reading supports.

**3. Next time:**
- Fetch the 720–830 range and the `resolve_duplicate_key_stash` definition in the same parallel batch as the first reads.
- Check the `parser.hpp` path that selects the callback parser.
- Check whether this checkout's duplicate-key stash logic differs from upstream. It looks like an unusual addition.

**4. Confidence:** Medium-high on the overall mechanism: placeholder, removal, and null root. I read those lines directly. Medium on the array and duplicate-key details, since I did not read that code.