**1. Helped:**
- `ghSearchCode` for `_transport_for_url` was the fastest step. It returned the method at line 760, the call site at 1005, and the commit SHA b5addb64f0 in one call. Its `next.readTopMatch` hint gave a ready-made read query.
- The third `ghGetFileContent` call (ranges 236-258 and 697-725) got the `_get_proxy_map` and `_mounts` construction I needed.

**2. Did not help:**
- The first `ghGetFileContent` (ranges 660-700 and 755-785) elided 701-754, so I needed another read. I had also requested the range-based reads without knowing where the `_mounts` code ended.
- The parallel `matchString` call for `get_environment_proxies` returned only the import line and a fragment (243-249). It was noisy and partly redundant with the third read.
- Output with "lines omitted" markers makes it easy to under-read.

**3. Next time:**
- Read one wider range, roughly 685-770, in a single call.
- Read `get_environment_proxies` and `URLPattern` in `_utils.py`. I skipped these, so env-var and NO_PROXY handling and the ordering of `URLPattern` are unconfirmed.

**4. Confidence:** High for the core mechanism: the first-match loop in `_transport_for_url`, a `None` mount meaning direct, and `proxy_map` feeding `_mounts`. Medium for the claim that sorting puts more specific patterns first. That is inferred, and I flagged it as such. I also did not read `_init_proxy_transport`.