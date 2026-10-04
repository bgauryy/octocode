**Answer:** requests 2.32.3 declares Python `>=3.8` and four runtime dependencies. Its source is the `v2.32.3` tag of `psf/requests`. The default `HTTPAdapter` uses 10 pooled connections, a max pool size of 10, no blocking, and no retries.

**Python versions**
- `requires_python` is `>=3.8`. The PyPI metadata shows this, and `setup.py:97` has `python_requires=">=3.8"`.

**Runtime dependencies** (`setup.py:61-65`, the `requires` list passed as `install_requires` at `setup.py:98`)
- `charset_normalizer>=2,<4`
- `idna>=2.5,<4`
- `urllib3>=1.21.1,<3`
- `certifi>=2017.4.17`

**Optional extras** (`setup.py:123-127`)
- `security` is an empty list.
- `socks` installs `PySocks>=1.5.6, !=1.5.7`.
- `use_chardet_on_py3` installs `chardet>=3.0.2,<6`.

**Source for this release**
- The PyPI metadata gives the repository as https://github.com/psf/requests, and the release was published 2024-05-29.
- I read the files at the `v2.32.3` tag, which resolved to commit `0e322af87745eff34caffe4df68456ebc20d9068`.
- The files are `setup.py` and `src/requests/adapters.py`.

**HTTPAdapter defaults** (`src/requests/adapters.py`)
- The module constants are at lines 71-74: `DEFAULT_POOLBLOCK = False`, `DEFAULT_POOLSIZE = 10`, `DEFAULT_RETRIES = 0`, and `DEFAULT_POOL_TIMEOUT = None`.
- `HTTPAdapter.__init__` (lines 202-208) defaults `pool_connections` and `pool_maxsize` to `DEFAULT_POOLSIZE` (10), `max_retries` to `DEFAULT_RETRIES` (0), and `pool_block` to `DEFAULT_POOLBLOCK` (False).
- Lines 209-212 turn the default of 0 retries into `Retry(0, read=False)`, so read errors are not retried either. Any other value goes through `Retry.from_int(max_retries)`.
- I did not trace where `DEFAULT_POOL_TIMEOUT` is used.

**Uncertainty:** The tool omitted `setup.py` lines 66-67. I inferred nothing from them: the PyPI metadata reports 4 dependencies, which matches the four listed at lines 62-65.