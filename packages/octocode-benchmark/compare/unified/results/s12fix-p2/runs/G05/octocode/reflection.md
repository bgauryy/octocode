1. **Helped:** The first `ghGetFileContent` call (`matchString: "def resolve_redirects"`) found the function at once. The short SHA resolved, and the response returned the full commit SHA, which I pinned for later calls. The second call (`ranges: ["286-420"]`) was the most efficient: it returned the rest of the loop plus `rebuild_auth`, `rebuild_proxies` and `rebuild_method` in one read.

2. **Did not help:** `contextLines: 120` was silently clamped to 100, so the first read ended at line 286 and I needed a second call. The `matchString: "allow_redirects"` call with `contextLines: 3` was noisy. It matched 11 lines and omitted large blocks, including the middle of `send()`, so I never read `send()` lines 777-798 or 806-814. I also didn't open `rewind_body` or `resolve_proxies`, and the answer says so.

3. **Next time:** I would skip the matchString probe and read `send()` directly with `ranges: ["770-825"]`. I would request explicit ranges from the start instead of relying on context padding.

4. **Confidence:** High for the `resolve_redirects` and `rebuild_*` behavior, because I read those lines in full. Medium-high for the `send()` wiring, which comes from partial reads.