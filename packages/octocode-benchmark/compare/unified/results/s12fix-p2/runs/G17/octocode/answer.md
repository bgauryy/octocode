**Summary:** requests 2.32.3 requires Python >=3.8 and has 4 runtime dependencies plus 3 extras. Its source is the `v2.32.3` tag of psf/requests. The HTTP adapter defaults to 10 pooled connections, a pool max size of 10, and no retries.

**Python versions**
- `python_requires=">=3.8"` is at `setup.py:97`. PyPI metadata also reports `>=3.8`.
- The classifiers list 3.8, 3.9, 3.10, 3.11 and 3.12 (`setup.py:110-114`), CPython and PyPy (`setup.py:116-117`).
- `setup.py:10-28` also exits at install time on anything below 3.8.

**Runtime dependencies** (`setup.py:61-66`)
- `charset_normalizer>=2,<4`
- `idna>=2.5,<4`
- `urllib3>=1.21.1,<3`
- `certifi>=2017.4.17`

**Optional extras** (`setup.py:123-127`)
- `security`: empty list.
- `socks`: `PySocks>=1.5.6, !=1.5.7`.
- `use_chardet_on_py3`: `chardet>=3.0.2,<6`.

**Source for this release**
- The repository is https://github.com/psf/requests (PyPI metadata, `setup.py:130`).
- The tag `v2.32.3` resolved to commit `0e322af87745eff34caffe4df68456ebc20d9068`. I read all files from that tag.
- PyPI reports a publish date of 2024-05-29.

**HTTPAdapter defaults** (`src/requests/adapters.py`)
- The module constants are `DEFAULT_POOLBLOCK = False`, `DEFAULT_POOLSIZE = 10`, `DEFAULT_RETRIES = 0` and `DEFAULT_POOL_TIMEOUT = None` (lines 71-74).
- `HTTPAdapter.__init__` (lines 202-208) takes `pool_connections=DEFAULT_POOLSIZE` (10), `pool_maxsize=DEFAULT_POOLSIZE` (10), `max_retries=DEFAULT_RETRIES` (0) and `pool_block=DEFAULT_POOLBLOCK` (False).
- When `max_retries` equals the default of 0, the adapter sets `Retry(0, read=False)` (lines 209-210). That means no retries, and read errors are not retried either. Any other value goes through `Retry.from_int` (line 212).

**Uncertainty**
- I did not inspect the published wheel's METADATA. The dependency and extras lists come from `setup.py` at the tag. The repo's `pyproject.toml` at that tag has only tool config.
- PyPI's `dependencies: 4` count matches the four runtime dependencies above.
- I did not look at how `DEFAULT_POOL_TIMEOUT` is used.