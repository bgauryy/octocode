requests 2.32.3 requires Python 3.8 or newer. It has four runtime dependencies and three extras. Its HTTP adapter defaults to a pool of 10 and no retries. I read everything below from the `psf/requests` repo at tag `v2.32.3`, using the GitHub API. I did not download the PyPI sdist or wheel, so I haven't confirmed that the published metadata matches this source.

**Python versions**
- `python_requires=">=3.8"` (`setup.py:97`).
- Classifiers list CPython 3.8, 3.9, 3.10, 3.11 and 3.12, plus CPython and PyPy implementations (`setup.py:~100-117`).
- The version guard in `setup.py:16-23` tells users on older Pythons to pin requests below 2.32.0.

**Runtime dependencies** (`install_requires`, `setup.py:61-66`)
- `charset_normalizer>=2,<4`
- `idna>=2.5,<4`
- `urllib3>=1.21.1,<3`
- `certifi>=2017.4.17`

**Optional extras** (`setup.py:123-127`)
- `security`: empty list.
- `socks`: `PySocks>=1.5.6, !=1.5.7`
- `use_chardet_on_py3`: `chardet>=3.0.2,<6`

**Source for this release**
- Repo: https://github.com/psf/requests, tag `v2.32.3`. The tag object is `61e2240f283f15780ac2d0e2cfefb0fd6fdab627`. I didn't look up the commit SHA it points to.
- `src/requests/__version__.py:8` has `__version__ = "2.32.3"`.
- `src/requests/__version__.py:7` has `__url__ = "https://requests.readthedocs.io"`.

**HTTP adapter defaults** (`src/requests/adapters.py`)
- Module constants (`:71-74`):
  - `DEFAULT_POOLBLOCK = False`
  - `DEFAULT_POOLSIZE = 10`
  - `DEFAULT_RETRIES = 0`
  - `DEFAULT_POOL_TIMEOUT = None`
- `HTTPAdapter.__init__` (`:204-207`) uses `pool_connections=10`, `pool_maxsize=10`, `max_retries=0` and `pool_block=False`.
- When `max_retries` equals the default of 0, the adapter sets `Retry(0, read=False)` (`:209-210`). Otherwise it uses `Retry.from_int(max_retries)` (`:212`). So the default does no retries, and read errors are not retried.