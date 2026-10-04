**Helped:** The `ghSearchCode` call for `_transport_for_url` was the fastest step. It returned the pinned commit SHA and the sync (760, 1005) and async (1474, 1721) locations in one shot. The parallel `ghGetFileContent` call with `matchString: "proxy_map"` located `_get_proxy_map` and the mount-building code. The follow-up `ranges` read (239-262, 685-722) gave the deciding lines.

**Did not help:**
- The `matchString` read elided large blocks ("lines 243-682 omitted"), so I needed a second read for the actual code.
- The `ghSearchCode` call for `get_environment_proxies` scoped to `httpx/_utils.py` came back empty. I guessed the path and never checked with `ghStructure`, so I don't know where it's defined.
- I never read `URLPattern`, `get_environment_proxies`, or `_init_proxy_transport`.

**Next time:** Drop the guessed-path search and do an unscoped one, or use `ghStructure`. Then read `URLPattern` (matching and ordering) to verify the "more specific patterns first" claim rather than inferring it from the `sorted` call. I'd also read `_init_proxy_transport` to confirm what a proxy transport is.

**Confidence:** High for the core mechanism (proxy map, mounts, first-match in `_transport_for_url`, direct fallback), since I read those lines. Medium for the details of environment-variable handling and the pattern-specificity ordering, which I didn't read and flagged as uncertain in the answer.