**1. Helped:** Two parallel calls did nearly all the work. `ghStructure` on `httpx/_transports` with the pinned SHA as `branch` listed the modules. `ghSearchCode` for `BaseTransport` scoped to that path returned every class definition, with line numbers and base classes, in one response.

**2. Did not help:**
- I never called `ghGetFileContent`. The client-compatibility column rests on base classes alone. I did not read how `Client` and `AsyncClient` check the transport type, so I never confirmed that `MockTransport` really works with both.
- The search output did not include the `ASGITransport` line number, and I did not follow up to get it.
- I did not pass `branch` to `ghSearchCode`. The server's instructions say it searches the default branch. The returned `commitSha` matched the pinned commit, so the results are probably valid, but I did not confirm that on purpose.
- I did not check how `__init__.py` re-exports the transports.

**3. Next time:** I would add one batched `ghGetFileContent` call for the client transport checks and the `asgi.py` class line. I would also pass `branch` explicitly to the search.

**4. Confidence:** Medium-high. The module and class locations are directly evidenced. The sync/async split is a reasonable inference from the base classes but is not directly verified. The `ASGITransport` line number is missing.