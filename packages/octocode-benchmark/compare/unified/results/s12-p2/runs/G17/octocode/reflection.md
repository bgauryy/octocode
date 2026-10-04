**1. Helped**
- `artifactSearch` (pypi, requests 2.32.3) gave `requires_python >=3.8`, the dependency count of 4 and the repo URL in one call. The count let me check the dependency list I found later.
- The batched `ghGetFileContent` on `adapters.py` with `matchString` returned the constants and the `__init__` signature without reading the whole 719-line file. It also reported the tag's commit SHA.
- `ghGetFileContent` on `setup.py` with `matchString` gave the dependency list, extras and classifiers.

**2. Did not help**
- Reading `pyproject.toml` with `fullContent` was a wasted guess. I assumed it held the dependencies, but it only has isort and pytest settings.
- The `setup.py` call used `contextLines: 12` and returned essentially the whole file, so it was noisy.
- `artifactSearch` gave only a dependency count, not the names or extras. Its repo link was marked "unverified".
- I never read the published wheel or sdist metadata, so that comparison is untested.

**3. Next time**
- Go straight from `artifactSearch` to `setup.py` with a tight line range.
- Batch the `adapters.py` and `setup.py` reads into one call.

**4. Confidence**
- High for the dependencies, extras, adapter defaults and commit SHA, since I read them at tag `v2.32.3`.
- Medium-high that they match the published PyPI metadata, since I did not check that directly.