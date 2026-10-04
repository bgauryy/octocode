**1. Helped:**
- `artifactSearch` (pypi, requests 2.32.3) returned `requiresPython >=3.8`, the repository URL and a dependency count of 4 in one call. That gave me the PyPI-side facts to check against.
- One batched `ghGetFileContent` call on branch `v2.32.3` fetched `adapters.py` with `matchString` and the full `setup.py`. It gave the dependencies, extras, classifiers and adapter defaults with line numbers, and it returned `shared.commitSha`.

**2. Did not help:**
- The `pyproject.toml` fetch in that batch was wasted, because it holds only tool config.
- `artifactSearch` gave a dependency count but no names or extras.
- The `matchString` output omitted lines 78-136 and 144-198. I never saw how `DEFAULT_POOL_TIMEOUT` or the retry settings are used downstream.
- I had no way to read the wheel's METADATA, which is the authoritative record of what is declared.

**3. Next time:**
- I would skip `pyproject.toml` and read `setup.py` directly.
- I would ask for `__init__` in `HTTPAdapter` with more context lines so `init_poolmanager` is visible.
- I would say outright that the commit SHA is inferred from the batch's shared field, not from a dedicated tag lookup.

**4. Confidence:**
- High for the Python version, dependencies, extras and adapter defaults, since I read all of them at the tagged source.
- Medium for "declared in the published wheel", because I did not check the wheel's metadata. The dependency count of 4 matches.