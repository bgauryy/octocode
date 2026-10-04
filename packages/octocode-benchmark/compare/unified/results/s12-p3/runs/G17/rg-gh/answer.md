**Answer:** requests 2.32.3 requires Python >=3.8. It has four runtime dependencies and two optional extras. Its source is the `v2.32.3` tag of `psf/requests`. The HTTP adapter defaults to 10 pooled connections and no retries. All line numbers below are from `setup.py` and `src/requests/adapters.py` at that tag.

**Python versions** (`setup.py`)
- `python_requires=">=3.8"` (`setup.py:97`).
- Classifiers list 3.8, 3.9, 3.10, 3.11 and 3.12, plus CPython and PyPy (`setup.py:108-117`).
- The error message for unsupported versions says to pin `<2.32.0` on older Pythons (`setup.py:23`).

**Runtime dependencies** (`install_requires`, `setup.py:61-65`)
- `charset_normalizer>=2,<4`
- `idna>=2.5,<4`
- `urllib3>=1.21.1,<3`
- `certifi>=2017.4.17`

**Optional extras** (`extras_require`, `setup.py:125-126`)
- `socks`: `PySocks>=1.5.6, !=1.5.7`
- `use_chardet_on_py3`: `chardet>=3.0.2,<6`

**Source for the release**
- Repo `psf/requests`, tag `v2.32.3`. The tag object SHA is `61e2240f283f15780ac2d0e2cfefb0fd6fdab627`.
- `src/requests/__version__.py:8` has `__version__ = "2.32.3"`.

**HTTPAdapter defaults** (`src/requests/adapters.py`)
- Module constants are `DEFAULT_POOLBLOCK = False`, `DEFAULT_POOLSIZE = 10`, `DEFAULT_RETRIES = 0` and `DEFAULT_POOL_TIMEOUT = None` (lines 71-74).
- The constructor defaults are `pool_connections=10`, `pool_maxsize=10`, `max_retries=0` and `pool_block=False` (lines 204-207).
- With the default `max_retries`, the adapter uses `Retry(0, read=False)`. Any other value goes through `Retry.from_int(max_retries)` (lines 209-212). So there are no retries by default, and read errors are not retried.

**Uncertainty**
- I read only `setup.py`, `adapters.py` and `__version__.py` at the tag. I did not check the PyPI metadata directly. The `pyproject.toml` hit in my output was only pytest config (`minversion`), and I did not inspect it further.