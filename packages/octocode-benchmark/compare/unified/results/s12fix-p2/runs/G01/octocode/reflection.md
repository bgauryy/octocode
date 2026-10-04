**Helped:** The first `ghGetHistoryItem` call on PR 16403 gave the body, merge date, file list and docs patches in one shot, and the docs patch described the intended behavior. The second `ghGetHistoryItem` call (with `files`) returned the full `routing.py` diff, which was the best evidence for hot-path changes. `ghGetFileContent` on `_runtime.py` at the merge SHA gave numbered lines I could cite.

**Did not help:** The patches were paginated and truncated, so `_api.py`, `background.py` and `_asgi.py` were only partly visible. The `ghGetFileContent` call with `fullContent` on three files paginated and returned only `_runtime.py`. The `_asgi.py` matchString read skipped lines, including the exception-logging code. I never read the tests, and I didn't follow `next.continuePatch`.

**Next time:** Fetch `_asgi.py` and `_api.py` with line ranges or `matchString` right away and follow `continuePatch`. Use `ghGetFileContent` calls with one file each to avoid pagination, and read `test_integrations.py`.

**Confidence:** Medium. The runtime-behavior summary is well supported. The claims about how contrib's middleware interacts with the new code, and the test coverage, are inferred from partial reads. I said so in my answer.

One correction to my answer: I numbered the reviewer list 1 to 7 but used "6" twice, so the real count is eight items.