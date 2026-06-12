# YAML (.yml)

Source sample: `yaml/typescript-ci.yml`

Strategy: `conservative`

Agent rating: **7/10 (good)**

Agent understanding from minified output: **9.2/10 (excellent)**

Artifacts:

- `raw/source.excerpt.txt`
- `minified/content-view.excerpt.txt`
- `minified/apply-minification.excerpt.txt`
- `minified/minify-content-sync.excerpt.txt`
- `minified/minify-content-async.excerpt.txt`
- `symbol/signatures.txt`

| Tool | Bytes | Cut | Time | Rating |
| --- | ---: | ---: | ---: | ---: |
| input | 12508 | - | - | - |
| content-view | 11733 | 6.2% | 3.927 ms | 7/10 |
| applyMinification | 11733 | 6.2% | 4.468 ms | 7/10 |
| sync minify | 11733 | 6.2% | 3.899 ms | 7/10 |
| async minify | 11733 | 6.2% | 3.55 ms | 7/10 |
| symbols | n/a | n/a | 0.009 ms | n/a |

## Agent Understanding

Measured from `standard` minified output.

| Component | Score |
| --- | ---: |
| syntax anchors | 10/10 (3/3) |
| delimiter structure | 10/10 |
| output health | 9/10 |
| context budget | 7/10 |
| symbol context | 7/10 |
| signals passed | 6/6 |

## Agent Observation By Output Level

Ratings are computed from the actual raw, standard, minify, and symbol outputs
for this language sample.

| Level | Bytes | Cut | Agent observation | Syntax anchors | Structure |
| --- | ---: | ---: | ---: | ---: | ---: |
| none | 12508 | 0% | 10/10 excellent | 10/10 | 10/10 |
| standard | 11733 | 6.2% | 9.2/10 excellent | 10/10 | 10/10 |
| minify | 11733 | 6.2% | 9.2/10 excellent | 10/10 | 10/10 |
| symbols | n/a | n/a | n/a | n/a | n/a |

## Notes

- conservative text strategy.
- symbols are not implemented for this extension.

## Before Excerpt

```yaml
name: CI

on:
  push:
    branches:
      - main
      - release-*
  pull_request:
    branches:
      - main
      - release-*
  merge_group:
    branches:
      - main
      # - release-*

permissions:
  contents: read

# Ensure scripts are run with pipefail. See:
# https://docs.github.com/en/actions/using-workflows/workflow-syntax-for-github-actions#exit-codes-and-error-action-preference
defaults:
  run:
    shell: bash

jobs:
  test:
    strategy:
      fail-fast: ${{ github.event_name == 'merge_group' }}
      matrix:
        config:
          # PRs only check the newest and oldest Node versions.
          # macOS only ever checks the neest and oldest Node versions, but never in PR runs.
          - os: ubuntu-latest
            node-version: '24'
            bundle: true
          - os: windows-latest
            node-version: '24'
            bundle: true
            skip: ${{ github.event_name == 'merge_group' }}
          - os: macos-latest
            node-version: '24'
            bundle: true
            skip: ${{ github.event_name == 'pull_request' || github.event_name == 'merge_group' }}

          - os: ubuntu-latest
            node-version: '22'
            bundle: true
            skip: 

... [truncated 10708 chars] ...

    name: fix_baselines.patch
          path: fix_baselines.patch

  required:
    runs-on: ubuntu-latest
    if: ${{ always() }}
    needs:
      - test
      - coverage
      - lint
      - knip
      - format
      - typecheck
      - smoke
      - package-size
      - misc
      - self-check
      - baselines

    steps:
      - name: Check required jobs
        env:
          NEEDS: ${{ toJson(needs) }}
        run: |
          ! echo $NEEDS | jq -e 'to_entries[] | { job: .key, result: .value.result } | select((.result == "success" or .result == "skipped") | not)'

```

## Content-View Excerpt

```yaml
name: CI

on:
  push:
    branches:
      - main
      - release-*
  pull_request:
    branches:
      - main
      - release-*
  merge_group:
    branches:
      - main

permissions:
  contents: read

defaults:
  run:
    shell: bash

jobs:
  test:
    strategy:
      fail-fast: ${{ github.event_name == 'merge_group' }}
      matrix:
        config:

          - os: ubuntu-latest
            node-version: '24'
            bundle: true
          - os: windows-latest
            node-version: '24'
            bundle: true
            skip: ${{ github.event_name == 'merge_group' }}
          - os: macos-latest
            node-version: '24'
            bundle: true
            skip: ${{ github.event_name == 'pull_request' || github.event_name == 'merge_group' }}

          - os: ubuntu-latest
            node-version: '22'
            bundle: true
            skip: ${{ github.event_name == 'pull_request' || github.event_name == 'merge_group' }}
          - os: windows-latest
            node-version: '22'
            bundle: true
            skip: ${{ github.event_name == 'pull_request' || github.event_name == 'merge_group' }}

          - os: ubuntu-latest
            node-version: '20'
            bundle:

... [truncated 9933 chars] ...

     name: fix_baselines.patch
          path: fix_baselines.patch

  required:
    runs-on: ubuntu-latest
    if: ${{ always() }}
    needs:
      - test
      - coverage
      - lint
      - knip
      - format
      - typecheck
      - smoke
      - package-size
      - misc
      - self-check
      - baselines

    steps:
      - name: Check required jobs
        env:
          NEEDS: ${{ toJson(needs) }}
        run: |
          ! echo $NEEDS | jq -e 'to_entries[] | { job: .key, result: .value.result } | select((.result == "success" or .result == "skipped") | not)'
```

## Apply Minification Excerpt

```yaml
name: CI

on:
  push:
    branches:
      - main
      - release-*
  pull_request:
    branches:
      - main
      - release-*
  merge_group:
    branches:
      - main

permissions:
  contents: read

defaults:
  run:
    shell: bash

jobs:
  test:
    strategy:
      fail-fast: ${{ github.event_name == 'merge_group' }}
      matrix:
        config:

          - os: ubuntu-latest
            node-version: '24'
            bundle: true
          - os: windows-latest
            node-version: '24'
            bundle: true
            skip: ${{ github.event_name == 'merge_group' }}
          - os: macos-latest
            node-version: '24'
            bundle: true
            skip: ${{ github.event_name == 'pull_request' || github.event_name == 'merge_group' }}

          - os: ubuntu-latest
            node-version: '22'
            bundle: true
            skip: ${{ github.event_name == 'pull_request' || github.event_name == 'merge_group' }}
          - os: windows-latest
            node-version: '22'
            bundle: true
            skip: ${{ github.event_name == 'pull_request' || github.event_name == 'merge_group' }}

          - os: ubuntu-latest
            node-version: '20'
            bundle:

... [truncated 9933 chars] ...

     name: fix_baselines.patch
          path: fix_baselines.patch

  required:
    runs-on: ubuntu-latest
    if: ${{ always() }}
    needs:
      - test
      - coverage
      - lint
      - knip
      - format
      - typecheck
      - smoke
      - package-size
      - misc
      - self-check
      - baselines

    steps:
      - name: Check required jobs
        env:
          NEEDS: ${{ toJson(needs) }}
        run: |
          ! echo $NEEDS | jq -e 'to_entries[] | { job: .key, result: .value.result } | select((.result == "success" or .result == "skipped") | not)'
```

## Sync Minify Excerpt

```yaml
name: CI

on:
  push:
    branches:
      - main
      - release-*
  pull_request:
    branches:
      - main
      - release-*
  merge_group:
    branches:
      - main

permissions:
  contents: read

defaults:
  run:
    shell: bash

jobs:
  test:
    strategy:
      fail-fast: ${{ github.event_name == 'merge_group' }}
      matrix:
        config:

          - os: ubuntu-latest
            node-version: '24'
            bundle: true
          - os: windows-latest
            node-version: '24'
            bundle: true
            skip: ${{ github.event_name == 'merge_group' }}
          - os: macos-latest
            node-version: '24'
            bundle: true
            skip: ${{ github.event_name == 'pull_request' || github.event_name == 'merge_group' }}

          - os: ubuntu-latest
            node-version: '22'
            bundle: true
            skip: ${{ github.event_name == 'pull_request' || github.event_name == 'merge_group' }}
          - os: windows-latest
            node-version: '22'
            bundle: true
            skip: ${{ github.event_name == 'pull_request' || github.event_name == 'merge_group' }}

          - os: ubuntu-latest
            node-version: '20'
            bundle:

... [truncated 9933 chars] ...

     name: fix_baselines.patch
          path: fix_baselines.patch

  required:
    runs-on: ubuntu-latest
    if: ${{ always() }}
    needs:
      - test
      - coverage
      - lint
      - knip
      - format
      - typecheck
      - smoke
      - package-size
      - misc
      - self-check
      - baselines

    steps:
      - name: Check required jobs
        env:
          NEEDS: ${{ toJson(needs) }}
        run: |
          ! echo $NEEDS | jq -e 'to_entries[] | { job: .key, result: .value.result } | select((.result == "success" or .result == "skipped") | not)'
```

## Async Minify Excerpt

```yaml
name: CI

on:
  push:
    branches:
      - main
      - release-*
  pull_request:
    branches:
      - main
      - release-*
  merge_group:
    branches:
      - main

permissions:
  contents: read

defaults:
  run:
    shell: bash

jobs:
  test:
    strategy:
      fail-fast: ${{ github.event_name == 'merge_group' }}
      matrix:
        config:

          - os: ubuntu-latest
            node-version: '24'
            bundle: true
          - os: windows-latest
            node-version: '24'
            bundle: true
            skip: ${{ github.event_name == 'merge_group' }}
          - os: macos-latest
            node-version: '24'
            bundle: true
            skip: ${{ github.event_name == 'pull_request' || github.event_name == 'merge_group' }}

          - os: ubuntu-latest
            node-version: '22'
            bundle: true
            skip: ${{ github.event_name == 'pull_request' || github.event_name == 'merge_group' }}
          - os: windows-latest
            node-version: '22'
            bundle: true
            skip: ${{ github.event_name == 'pull_request' || github.event_name == 'merge_group' }}

          - os: ubuntu-latest
            node-version: '20'
            bundle:

... [truncated 9933 chars] ...

     name: fix_baselines.patch
          path: fix_baselines.patch

  required:
    runs-on: ubuntu-latest
    if: ${{ always() }}
    needs:
      - test
      - coverage
      - lint
      - knip
      - format
      - typecheck
      - smoke
      - package-size
      - misc
      - self-check
      - baselines

    steps:
      - name: Check required jobs
        env:
          NEEDS: ${{ toJson(needs) }}
        run: |
          ! echo $NEEDS | jq -e 'to_entries[] | { job: .key, result: .value.result } | select((.result == "success" or .result == "skipped") | not)'
```

## Symbols

```txt
No symbols returned for this sample.
```
