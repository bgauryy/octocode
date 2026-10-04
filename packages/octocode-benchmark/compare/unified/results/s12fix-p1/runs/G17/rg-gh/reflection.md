1. **Helped:** The first Bash call fetched `setup.py`, `adapters.py`, `__version__.py` and the tag ref in one go. It used `gh api -X GET repos/psf/requests/contents/... -f ref=v2.32.3` with the raw Accept header. That pinned everything to the tag and showed line numbers via `cat -n`. It gave me `python_requires`, the extras, the `DEFAULT_*` constants and the tag SHA.

2. **Did not help:** My grep filter in that call cut off the `requires` list, so I needed a second call (`sed -n 59,76p`) to read lines 61–66. Every call also printed `/dev/null: Operation not permitted`. It was harmless, but noisy. I never fetched PyPI metadata, so "matches PyPI" is unverified.

3. **Next time:** I'd print the relevant `setup.py` ranges directly instead of grepping them. I'd also resolve the annotated tag to its commit SHA with one more `gh api` call.

4. **Confidence:** High on the dependencies, extras, Python range and adapter defaults, since I read them at the tag. Two statements in my answer were wrong or overstated:
   - I said the `setup.py` data is "the same data PyPI shows", then said I hadn't checked. That should have been phrased as an expectation only.
   - My line on `Retry(0, read=False)` was misleading. It applies only when `max_retries` is the default 0. With `max_retries=3`, `Retry.from_int(3)` is used and read errors can be retried.