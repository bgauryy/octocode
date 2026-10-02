**Helped:** The first `ghGetFileContent` batch did most of the work. Three files in one call (`applications.py` with `matchString`, `exceptions.py`, `_exception_handler.py`), pinned to `63c5760d8a`, gave exact line numbers for stack assembly and handler dispatch. Reading whole small files avoided follow-up calls.

**Did not help:** The `ghSearchCode` call was a weaker step.
- It searches the default branch. It returned `commitSha` 4e7fc04, not the pinned commit, so the `routing.py:65/84` hits aren't verified at 63c5760d8a. I flagged that in the answer, but the tool can't search at a pinned ref.
- The `errors.py` query returned empty because I passed keywords with a path filter. The hint told me to broaden or check with `ghStructure`, and I didn't. As a result I never read `ServerErrorMiddleware`, and the answer says so.

**Next time:** I'd read `starlette/middleware/errors.py` and the `routing.py` lines directly with `ghGetFileContent` at the pinned commit, instead of using indexed search. Those two reads would have closed both gaps in one batch.

**Confidence:** High for stack order and the `ExceptionMiddleware`/`wrap_app_handling_exceptions` dispatch, because I read those lines at the pinned commit. Medium for the routing-level handling, because its line numbers come from a different commit. Low for what `ServerErrorMiddleware` does with the error, because I didn't read it.