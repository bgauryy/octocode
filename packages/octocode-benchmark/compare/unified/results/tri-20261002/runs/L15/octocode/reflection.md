**1. Helped:** The first `localSearch` (regex on `MAX_RUN_MULTIPLIER|hashFloodingDetected|flooding`, scoped to three files) found the defense in one call. Its line-numbered matches pointed straight at `ImmutableSet.java` 744–879. The `localFetch` with three ranges then returned the detection logic and comments I cited.

**2. Did not help:**
- My first `localFetch` failed validation because I wrote ranges as "660,680" instead of "660-680". That cost one wasted call.
- The fetch silently omitted lines 681–699 and 761–803. I never read how `build()` handles the JDK-backed builder, so I flagged that as unverified.
- I included `ImmutableMap.java` and `RegularImmutableSet.java` in the search scope. Only `ImmutableMap.java` matched, once, and it was irrelevant.

**3. Next time:** I would check the range format in the schema before the first fetch. I would also request the omitted gaps, or one wider range, in the same call so the answer covers `build()`. I'd also search `JdkBackedImmutableSet` to confirm the final set type and how the map side handles it.

**4. Confidence:** Medium-high. The mechanism (smear, probe-run limit, `hashFloodingDetected`, `HashSet` fallback) rests on code and comments I read directly. The medium part is that I did not verify the `build()` path or the other immutable collections.