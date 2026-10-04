**Summary:** requests 2.32.3 requires Python >=3.8 and has four runtime dependencies plus three extras. Its source is the `v2.32.3` tag of psf/requests. `HTTPAdapter` defaults to a pool of 10 connections, a pool size of 10, no retries and non-blocking pools.

**Python versions**
- `python_requires=">=3.8"` at `setup.py:97`. PyPI metadata also reports `requiresPython >=3.8`.
- The classifiers I saw list 3.11 and 3.12, plus "3 :: Only", CPython and PyPy (`setup.py:113-117`). Lines 108-112 were omitted from my read, so I haven't seen the 3.8–3.10 classifiers.

**Runtime dependencies** (`install_requires`, `setup.py:61-66`)
- `charset_normalizer>=2,<4`
- `idna>=2.5,<4`
- `urllib3>=1.21.1,<3`
- `certifi>=2017.4.17`

**Extras** (`setup.py:123-127`)
- `security`: empty list.
- `socks`: `PySocks>=1.5.6, !=1.5.7`.
- `use_chardet_on_py3`: `chardet>=3.0.2,<6`.

**Source for this release**
- Repository: https://github.com/psf/requests (from PyPI metadata). The release was published 2024-05-29.
- Tag `v2.32.3` resolves to commit `0e322af87745eff34caffe4df68456ebc20d9068`. All file reads above came from that tag.
- The package code is in `src/requests`.

**HTTPAdapter defaults** (`src/requests/adapters.py`)
- Constants at lines 71-74:
  - `DEFAULT_POOLBLOCK = False`
  - `DEFAULT_POOLSIZE = 10`
  - `DEFAULT_RETRIES = 0`
  - `DEFAULT_POOL_TIMEOUT = None`
- `__init__` (lines 202-208) uses `pool_connections=DEFAULT_POOLSIZE` (10), `pool_maxsize=DEFAULT_POOLSIZE` (10), `max_retries=DEFAULT_RETRIES` (0) and `pool_block=DEFAULT_POOLBLOCK` (False).
- Retry behaviour (lines 209-212): when `max_retries` equals the default 0, it sets `Retry(0, read=False)`. Otherwise it uses `Retry.from_int(max_retries)`. So by default there are no retries, and read errors are not retried.

**Uncertainty:** `DEFAULT_POOL_TIMEOUT` is defined, but I didn't check where it's used.