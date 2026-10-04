**Answer:** requests 2.32.3 requires Python >=3.8. It has four runtime dependencies and three extras. Its source is the `v2.32.3` tag of github.com/psf/requests. The `HTTPAdapter` defaults to 10 pooled connections, 10 connections per pool and no retries. I read the files from the tag via the GitHub API, not from the PyPI sdist.

**Python versions** (`setup.py` at `v2.32.3`)
- `python_requires=">=3.8"` (`setup.py:97`). `REQUIRED_PYTHON = (3, 8)` at `setup.py:10` is a second check inside the script.
- The classifiers list 3.8, 3.9, 3.10, 3.11 and 3.12, plus CPython and PyPy (`setup.py:110-117`).

**Runtime dependencies** (`setup.py:62-65`, passed as `install_requires` at line 98)
- `charset_normalizer>=2,<4`
- `idna>=2.5,<4`
- `urllib3>=1.21.1,<3`
- `certifi>=2017.4.17`

**Optional extras** (`setup.py:123-126`)
- `security` is an empty list, kept only for backward compatibility.
- `socks` installs `PySocks>=1.5.6, !=1.5.7`.
- `use_chardet_on_py3` installs `chardet>=3.0.2,<6`.

The `pytest*` entries and a second `PySocks` entry at `setup.py:68-73` are test dependencies, not runtime ones.

**Source for this release**
- The repo is https://github.com/psf/requests, tag `v2.32.3`. `src/requests/__version__.py:8` has `__version__ = "2.32.3"`.
- The tag ref points to an annotated tag object, `61e2240f283f15780ac2d0e2cfefb0fd6fdab627`. I did not resolve it to the underlying commit SHA.
- The project URLs in `setup.py:129-130` give "Source" as https://github.com/psf/requests and "Documentation" as https://requests.readthedocs.io.

**HTTP adapter defaults** (`src/requests/adapters.py`)
- Module constants: `DEFAULT_POOLBLOCK = False`, `DEFAULT_POOLSIZE = 10`, `DEFAULT_RETRIES = 0`, `DEFAULT_POOL_TIMEOUT = None` (lines 71-74).
- The `HTTPAdapter.__init__` signature uses `pool_connections=DEFAULT_POOLSIZE` (10), `pool_maxsize=DEFAULT_POOLSIZE` (10), `max_retries=DEFAULT_RETRIES` (0) and `pool_block=DEFAULT_POOLBLOCK` (False) (lines 204-207).
- When `max_retries` equals the default, the adapter sets `Retry(0, read=False)` (lines 209-210). That means no retries, and read errors are not retried. Any other value goes through `Retry.from_int(max_retries)` (line 212).
- `DEFAULT_POOL_TIMEOUT` is defined but I did not check where, or whether, it is used.

**Uncertainty:** the PyPI sdist and wheel metadata could differ slightly from the tag's `setup.py`. I did not check them. I also did not look at `pyproject.toml`, whose fetch returned nothing.