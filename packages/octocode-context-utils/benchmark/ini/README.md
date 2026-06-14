# INI (.ini)

Source sample: `ini/pytest-tox.ini`

Strategy: `conservative`

| Tool | Bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| input | 7518 | - | - |
| content-view | 5746 | 23.6% | 0.43 ms |
| applyMinification | 5754 | 23.5% | 0.427 ms |
| sync minify | 5754 | 23.5% | 0.438 ms |
| async minify | 5754 | 23.5% | 0.431 ms |
| symbols | n/a | n/a | 0.001 ms |

## Notes

- conservative text strategy.
- symbols are not implemented for this extension.

## Before Excerpt

```ini
[tox]
requires =
    tox >= 4
envlist =
    linting
    py310
    py311
    py312
    py313
    py314
    py315
    pypy3
    py310-{pexpect,xdist,twisted24,twisted25,asynctest,numpy,pluggymain,pylib}
    doctesting
    doctesting-coverage
    plugins
    py310-freeze
    docs
    docs-checklinks

    # checks that 3.11 native ExceptionGroup works with exceptiongroup
    # not included in CI.
    py311-exceptiongroup



[pkgenv]
# NOTE: This section tweaks how Tox manages the PEP 517 build
# NOTE: environment where it assembles wheels (editable and regular)
# NOTE: for further installing them into regular testenvs.
#
# NOTE: `[testenv:.pkg]` does not work due to a regression in tox v4.14.1
# NOTE: so `[pkgenv]` is being used in place of it.
# Refs:
# * https://github.com/tox-dev/tox/pull/3237
# * https://github.com/tox-dev/tox/issues/3238
# * https://github.com/tox-dev/tox/issues/3292
# * https://hynek.me/articles/turbo-charge-tox/
#
# NOTE: The `SETUPTOOLS_SCM_PRETEND_VERSION_FOR_PYTEST` environment
# NOTE: variable allows enforcing a pre-determined version for use in
# NOTE: the wheel being installed into usual testenvs.
pass_env =
  SETUPTOOLS_SCM_PRETEND_VERSION_FOR_PYTEST


[testenv]
description =


... [truncated 5718 chars] ...

lease]passenv}
deps = {[testenv:release]deps}
commands = python scripts/prepare-release-pr.py {posargs}

[testenv:generate-gh-release-notes]
description = generate release notes that can be published as GitHub Release
usedevelop = True
deps =
    pypandoc_binary
commands = python scripts/generate-gh-release-notes.py {posargs}

[testenv:update-plugin-list]
description = update the plugin list
skip_install = True
deps =
    packaging
    requests
    tabulate[widechars]
    tqdm
    requests-cache
    platformdirs
commands = python scripts/update-plugin-list.py {posargs}

```

## Content-View Excerpt

```ini
[tox]
requires =
    tox >= 4
envlist =
    linting
    py310
    py311
    py312
    py313
    py314
    py315
    pypy3
    py310-{pexpect,xdist,twisted24,twisted25,asynctest,numpy,pluggymain,pylib}
    doctesting
    doctesting-coverage
    plugins
    py310-freeze
    docs
    docs-checklinks

    py311-exceptiongroup

[pkgenv]

pass_env =
  SETUPTOOLS_SCM_PRETEND_VERSION_FOR_PYTEST

[testenv]
description =
    run the tests
    coverage: collecting coverage
    exceptiongroup: against `exceptiongroup`
    nobyte: in no-bytecode mode
    lsof: with `--lsof` pytest CLI option
    numpy: against `numpy`
    pexpect: against `pexpect`
    pluggymain: against the bleeding edge `pluggy` from Git
    pylib: against `py` lib
    twisted24: against the unit test extras with twisted prior to 24.0
    twisted25: against the unit test extras with twisted 25.0 or later
    asynctest: against the unit test extras with asynctest
    xdist: with pytest in parallel mode
    under `{basepython}`
    doctesting: including doctests
commands =
    {env:_PYTEST_TOX_COVERAGE_RUN:} pytest {posargs:{env:_PYTEST_TOX_DEFAULT_POSARGS:}}
    doctesting: {env:_PYTEST_TOX_COVERAGE_RUN:} pytest --doctest-modules {env:_PYTEST_TOX_PO

... [truncated 3946 chars] ...

elease]passenv}
deps = {[testenv:release]deps}
commands = python scripts/prepare-release-pr.py {posargs}

[testenv:generate-gh-release-notes]
description = generate release notes that can be published as GitHub Release
usedevelop = True
deps =
    pypandoc_binary
commands = python scripts/generate-gh-release-notes.py {posargs}

[testenv:update-plugin-list]
description = update the plugin list
skip_install = True
deps =
    packaging
    requests
    tabulate[widechars]
    tqdm
    requests-cache
    platformdirs
commands = python scripts/update-plugin-list.py {posargs}
```

## Apply Minification Excerpt

```ini
[tox]
requires =
    tox >= 4
envlist =
    linting
    py310
    py311
    py312
    py313
    py314
    py315
    pypy3
    py310-{pexpect,xdist,twisted24,twisted25,asynctest,numpy,pluggymain,pylib}
    doctesting
    doctesting-coverage
    plugins
    py310-freeze
    docs
    docs-checklinks


    py311-exceptiongroup


[pkgenv]


pass_env =
  SETUPTOOLS_SCM_PRETEND_VERSION_FOR_PYTEST


[testenv]
description =
    run the tests
    coverage: collecting coverage
    exceptiongroup: against `exceptiongroup`
    nobyte: in no-bytecode mode
    lsof: with `--lsof` pytest CLI option
    numpy: against `numpy`
    pexpect: against `pexpect`
    pluggymain: against the bleeding edge `pluggy` from Git
    pylib: against `py` lib
    twisted24: against the unit test extras with twisted prior to 24.0
    twisted25: against the unit test extras with twisted 25.0 or later
    asynctest: against the unit test extras with asynctest
    xdist: with pytest in parallel mode
    under `{basepython}`
    doctesting: including doctests
commands =
    {env:_PYTEST_TOX_COVERAGE_RUN:} pytest {posargs:{env:_PYTEST_TOX_DEFAULT_POSARGS:}}
    doctesting: {env:_PYTEST_TOX_COVERAGE_RUN:} pytest --doctest-modules {env:_PYTEST_TO

... [truncated 3954 chars] ...

elease]passenv}
deps = {[testenv:release]deps}
commands = python scripts/prepare-release-pr.py {posargs}

[testenv:generate-gh-release-notes]
description = generate release notes that can be published as GitHub Release
usedevelop = True
deps =
    pypandoc_binary
commands = python scripts/generate-gh-release-notes.py {posargs}

[testenv:update-plugin-list]
description = update the plugin list
skip_install = True
deps =
    packaging
    requests
    tabulate[widechars]
    tqdm
    requests-cache
    platformdirs
commands = python scripts/update-plugin-list.py {posargs}
```

## Sync Minify Excerpt

```ini
[tox]
requires =
    tox >= 4
envlist =
    linting
    py310
    py311
    py312
    py313
    py314
    py315
    pypy3
    py310-{pexpect,xdist,twisted24,twisted25,asynctest,numpy,pluggymain,pylib}
    doctesting
    doctesting-coverage
    plugins
    py310-freeze
    docs
    docs-checklinks


    py311-exceptiongroup


[pkgenv]


pass_env =
  SETUPTOOLS_SCM_PRETEND_VERSION_FOR_PYTEST


[testenv]
description =
    run the tests
    coverage: collecting coverage
    exceptiongroup: against `exceptiongroup`
    nobyte: in no-bytecode mode
    lsof: with `--lsof` pytest CLI option
    numpy: against `numpy`
    pexpect: against `pexpect`
    pluggymain: against the bleeding edge `pluggy` from Git
    pylib: against `py` lib
    twisted24: against the unit test extras with twisted prior to 24.0
    twisted25: against the unit test extras with twisted 25.0 or later
    asynctest: against the unit test extras with asynctest
    xdist: with pytest in parallel mode
    under `{basepython}`
    doctesting: including doctests
commands =
    {env:_PYTEST_TOX_COVERAGE_RUN:} pytest {posargs:{env:_PYTEST_TOX_DEFAULT_POSARGS:}}
    doctesting: {env:_PYTEST_TOX_COVERAGE_RUN:} pytest --doctest-modules {env:_PYTEST_TO

... [truncated 3954 chars] ...

elease]passenv}
deps = {[testenv:release]deps}
commands = python scripts/prepare-release-pr.py {posargs}

[testenv:generate-gh-release-notes]
description = generate release notes that can be published as GitHub Release
usedevelop = True
deps =
    pypandoc_binary
commands = python scripts/generate-gh-release-notes.py {posargs}

[testenv:update-plugin-list]
description = update the plugin list
skip_install = True
deps =
    packaging
    requests
    tabulate[widechars]
    tqdm
    requests-cache
    platformdirs
commands = python scripts/update-plugin-list.py {posargs}
```

## Async Minify Excerpt

```ini
[tox]
requires =
    tox >= 4
envlist =
    linting
    py310
    py311
    py312
    py313
    py314
    py315
    pypy3
    py310-{pexpect,xdist,twisted24,twisted25,asynctest,numpy,pluggymain,pylib}
    doctesting
    doctesting-coverage
    plugins
    py310-freeze
    docs
    docs-checklinks


    py311-exceptiongroup


[pkgenv]


pass_env =
  SETUPTOOLS_SCM_PRETEND_VERSION_FOR_PYTEST


[testenv]
description =
    run the tests
    coverage: collecting coverage
    exceptiongroup: against `exceptiongroup`
    nobyte: in no-bytecode mode
    lsof: with `--lsof` pytest CLI option
    numpy: against `numpy`
    pexpect: against `pexpect`
    pluggymain: against the bleeding edge `pluggy` from Git
    pylib: against `py` lib
    twisted24: against the unit test extras with twisted prior to 24.0
    twisted25: against the unit test extras with twisted 25.0 or later
    asynctest: against the unit test extras with asynctest
    xdist: with pytest in parallel mode
    under `{basepython}`
    doctesting: including doctests
commands =
    {env:_PYTEST_TOX_COVERAGE_RUN:} pytest {posargs:{env:_PYTEST_TOX_DEFAULT_POSARGS:}}
    doctesting: {env:_PYTEST_TOX_COVERAGE_RUN:} pytest --doctest-modules {env:_PYTEST_TO

... [truncated 3954 chars] ...

elease]passenv}
deps = {[testenv:release]deps}
commands = python scripts/prepare-release-pr.py {posargs}

[testenv:generate-gh-release-notes]
description = generate release notes that can be published as GitHub Release
usedevelop = True
deps =
    pypandoc_binary
commands = python scripts/generate-gh-release-notes.py {posargs}

[testenv:update-plugin-list]
description = update the plugin list
skip_install = True
deps =
    packaging
    requests
    tabulate[widechars]
    tqdm
    requests-cache
    platformdirs
commands = python scripts/update-plugin-list.py {posargs}
```

## Symbols

```txt
No symbols returned for this sample.
```
