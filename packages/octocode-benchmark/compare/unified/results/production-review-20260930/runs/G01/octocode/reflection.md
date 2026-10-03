1. **Helped:**
- The first `ghGetHistoryItem` call (body plus `changedFiles`) gave the PR's scope, merge SHA and file list in one step.
- The second `ghGetHistoryItem` call, with selected patches for `applications.py`, `routing.py`, `background.py` and `pyproject.toml`, exposed the behavior changes directly.
- `ghGetFileContent` on `_runtime.py` at the merge SHA showed the env auto-configure logic in full.
- A targeted `ghGetFileContent` line range on `_asgi.py` (lines 72–200) gave `enabled()` and `_legacy_otel`.

2. **Did not help:**
- The `_asgi.py` `minify: "symbols"` outline showed only signatures and truncated bodies, so it needed the follow-up line-range read.
- The `pyproject.toml` patch was cut off, and I never fetched the rest.
- I never read `_api.py`, most of `_asgi.py`, the tests or the docs. So I couldn't confirm span or metric names, what happens when `_legacy_otel` is true, or test coverage.
- I made no `clasify` calls and no errors occurred.

3. **Next time:** I would batch patch reads for `_api.py` and the remaining `_asgi.py`, and I would read the docs page and a few tests to verify the claims I left as open questions.

4. **Confidence:** medium. The changes I described come from diffs and source I actually read. I said in the answer that the telemetry data model and the contrib-coexistence behavior were not fully checked. I did not run anything, so "unchanged with no provider" follows from the code but is untested.