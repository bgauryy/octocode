**Summary:** requests 2.32.3 requires Python >=3.8 and has four runtime dependencies plus three extras. Its source is the `v2.32.3` tag of `psf/requests`. The HTTP adapter defaults to 10 pooled connections, a pool size of 10, no retries and non-blocking pools.

**Python versions**
- PyPI metadata says `requires_python >=3.8`, and `setup.py:97` has `python_requires=">=3.8"`.
- `setup.py:10` sets `REQUIRED_PYTHON = (3, 8)`.
- The classifiers list 3.8, 3.9, 3.10, 3.11 and 3.12 (`setup.py:110-114`). They also list CPython and PyPy (`setup.py:116-117`).

**Runtime dependencies** (`setup.py:61-66`)
- `charset_normalizer>=2,<4`
- `idna>=2.5,<4`
- `urllib3>=1.21.1,<3`
- `certifi>=2017.4.17`

PyPI reports 4 dependencies, which matches this list.

**Extras** (`setup.py:123-127`)
- `security`: empty list.
- `socks`: `PySocks>=1.5.6, !=1.5.7`.
- `use_chardet_on_py3`: `chardet>=3.0.2,<6`.

**Source for this release**
- The repository is https://github.com/psf/requests, taken from the PyPI metadata and `setup.py:130`.
- I read the files at tag `v2.32.3`, which resolved to commit `0e322af87745eff34caffe4df68456ebc20d9068`.
- Dependencies are declared in `setup.py`. `pyproject.toml` at that tag only holds isort and pytest settings.

**HTTP adapter defaults** (`src/requests/adapters.py`)
- Module constants (lines 71-74):
  - `DEFAULT_POOLBLOCK = False`
  - `DEFAULT_POOLSIZE = 10`
  - `DEFAULT_RETRIES = 0`
  - `DEFAULT_POOL_TIMEOUT = None`
- `HTTPAdapter.__init__` (lines 202-208) defaults `pool_connections=10`, `pool_maxsize=10`, `max_retries=0` and `pool_block=False`.
- When `max_retries == DEFAULT_RETRIES`, it sets `self.max_retries = Retry(0, read=False)` (lines 209-210).
  - That means no retries, and read errors are not retried either.
  - Any other value goes through `Retry.from_int` (line 212).

**Uncertainty:** I did not open the published wheel or sdist. The dependency and extras lists come from the git tag's `setup.py`, but the PyPI metadata agrees on the Python requirement and the dependency count.