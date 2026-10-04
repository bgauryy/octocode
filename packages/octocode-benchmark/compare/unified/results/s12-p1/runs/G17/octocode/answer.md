**Answer:** requests 2.32.3 declares Python >=3.8 and four runtime dependencies. It has three extras, one of them empty. The source is the `v2.32.3` tag of github.com/psf/requests. The adapter defaults to 10 pools, a pool size of 10, and 0 retries with no blocking.

**Python versions** (`setup.py` at tag v2.32.3)
- `python_requires=">=3.8"` is at `setup.py:97`. `REQUIRED_PYTHON = (3, 8)` is at `setup.py:10`.
- The classifiers list CPython 3.8 to 3.12 (`setup.py:110-114`) and also PyPy (`setup.py:117`).
- PyPI metadata also reports `requiresPython >=3.8`, published 2024-05-29, license Apache-2.0.

**Runtime dependencies** (`setup.py:61-66`)
- `charset_normalizer>=2,<4`
- `idna>=2.5,<4`
- `urllib3>=1.21.1,<3`
- `certifi>=2017.4.17`
- PyPI reports 4 dependencies, which matches.

**Extras** (`setup.py:123-127`)
- `security`: empty list.
- `socks`: `PySocks>=1.5.6, !=1.5.7`.
- `use_chardet_on_py3`: `chardet>=3.0.2,<6`.

**Source for this release**
- The repository is https://github.com/psf/requests, tag `v2.32.3`. The tag resolved to commit `0e322af87745eff34caffe4df68456ebc20d9068`.
- At this tag `pyproject.toml` holds only isort and pytest config, so the metadata above comes from `setup.py`.

**HTTPAdapter defaults** (`src/requests/adapters.py`)
- Module constants at lines 71-74:
  - `DEFAULT_POOLBLOCK = False`
  - `DEFAULT_POOLSIZE = 10`
  - `DEFAULT_RETRIES = 0`
  - `DEFAULT_POOL_TIMEOUT = None`
- `HTTPAdapter.__init__` (lines 202-208) defaults `pool_connections` and `pool_maxsize` to `DEFAULT_POOLSIZE` (10), `max_retries` to `DEFAULT_RETRIES` (0), and `pool_block` to `DEFAULT_POOLBLOCK` (False).
- Lines 209-212: when `max_retries == DEFAULT_RETRIES`, the adapter sets `Retry(0, read=False)`. Otherwise it uses `Retry.from_int(max_retries)`. So by default there are no retries, and read errors are not retried either.

**Uncertainty:** I did not read lines 213-237 of `adapters.py`, which should be the rest of `__init__`. I also did not check how `DEFAULT_POOL_TIMEOUT` is used.