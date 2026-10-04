**Answer:** requests 2.32.3 needs Python ≥3.8 and has four runtime dependencies. Its source is the `v2.32.3` tag of `psf/requests`. The default `HTTPAdapter` uses a pool size of 10 and does no retries. I read all of this from the GitHub tag. I did not check the PyPI metadata itself.

**Python versions** (`setup.py` at `v2.32.3`)
- `python_requires=">=3.8"` is at `setup.py:97`. `REQUIRED_PYTHON = (3, 8)` is at line 10, and the import-time guard is at lines 12–27.
- Classifiers list 3.8, 3.9, 3.10, 3.11 and 3.12, for CPython and PyPy (`setup.py:108-117`).

**Runtime dependencies** (`setup.py:61-66`)
- `charset_normalizer>=2,<4`
- `idna>=2.5,<4`
- `urllib3>=1.21.1,<3`
- `certifi>=2017.4.17`

**Optional extras** (`setup.py:123-127`)
- `security` is an empty list.
- `socks` installs `PySocks>=1.5.6, !=1.5.7`.
- `use_chardet_on_py3` installs `chardet>=3.0.2,<6`.

**Source for the release**
- The repo is https://github.com/psf/requests, at tag `v2.32.3`.
- The tag object SHA is `61e2240f283f15780ac2d0e2cfefb0fd6fdab627`. This is the annotated tag object, not the commit it points to.
- `src/requests/__version__.py:8` has `__version__ = "2.32.3"`.

**HTTPAdapter defaults** (`src/requests/adapters.py`)
- The module constants are `DEFAULT_POOLBLOCK = False`, `DEFAULT_POOLSIZE = 10`, `DEFAULT_RETRIES = 0` and `DEFAULT_POOL_TIMEOUT = None` (lines 71–74).
- The `__init__` signature (lines 204–207) defaults `pool_connections` and `pool_maxsize` to 10, `max_retries` to 0 and `pool_block` to `False`.
- When `max_retries` equals the default, the adapter sets `Retry(0, read=False)` (line 210). Otherwise it calls `Retry.from_int(max_retries)` (line 212). So there are no retries by default, and read errors are never retried even if retries are enabled with `Retry` defaults.

**Uncertainty:** the `setup.py` packaging data is the same data PyPI shows for the release. I didn't fetch the PyPI page to confirm, and I didn't check whether PyPI's metadata differs.