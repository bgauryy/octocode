**Answer:** requests 2.32.3 requires Python >=3.8 and has four runtime dependencies plus three extras. Its HTTP adapter defaults to a pool of 10 connections, 10 per pool, and no retries. I read all of this from the `v2.32.3` tag of `psf/requests` on GitHub, not from the PyPI metadata itself.

**Source for the release**
- The repo is `github.com/psf/requests`, tag `v2.32.3`. It is an annotated tag, and it points to commit `0e322af87745eff34caffe4df68456ebc20d9068`.
- `src/requests/__version__.py:8` sets `__version__ = "2.32.3"`.

**Python versions** (`setup.py`)
- `python_requires=">=3.8"` is at `setup.py:97`.
- The classifiers list 3.8, 3.9, 3.10, 3.11 and 3.12, plus CPython and PyPy. They are at `setup.py:~108-116`; I did not capture exact line numbers for them.

**Runtime dependencies** (`setup.py:61-66`)
- `charset_normalizer>=2,<4`
- `idna>=2.5,<4`
- `urllib3>=1.21.1,<3`
- `certifi>=2017.4.17`

**Optional extras** (`setup.py:123-127`)
- `security` is an empty list.
- `socks` installs `PySocks>=1.5.6, !=1.5.7`.
- `use_chardet_on_py3` installs `chardet>=3.0.2,<6`.

**HTTP adapter defaults** (`src/requests/adapters.py`)
- The module constants are `DEFAULT_POOLBLOCK = False`, `DEFAULT_POOLSIZE = 10`, `DEFAULT_RETRIES = 0` and `DEFAULT_POOL_TIMEOUT = None` (lines 71-74).
- The `HTTPAdapter.__init__` signature (lines 204-207) uses `pool_connections=DEFAULT_POOLSIZE` (10), `pool_maxsize=DEFAULT_POOLSIZE` (10), `max_retries=DEFAULT_RETRIES` (0) and `pool_block=DEFAULT_POOLBLOCK` (False).
- At lines 209-212, if `max_retries` equals the default, the adapter sets `self.max_retries = Retry(0, read=False)`. Any other value goes through `Retry.from_int(max_retries)`.
- In practice, there are no retries by default. Read errors are also not retried.

**Uncertainty**
- I did not open the PyPI page, so the `Requires-Python` and `Requires-Dist` fields in the published metadata are unchecked.
- The upstream `setup.py` also has a `pyproject.toml` alongside it. I did not check what `pyproject.toml` declares.