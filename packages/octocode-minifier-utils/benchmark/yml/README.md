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
| content-view | 11733 | 6.2% | 2.877 ms | 7/10 |
| applyMinification | 11738 | 6.2% | 3.022 ms | 7/10 |
| sync minify | 11738 | 6.2% | 2.901 ms | 7/10 |
| async minify | 11738 | 6.2% | 3.002 ms | 7/10 |
| symbols | 13959 | -11.6% | 0.837 ms | n/a |

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
| minify | 11738 | 6.2% | 9.2/10 excellent | 10/10 | 10/10 |
| symbols | 13959 | -11.6% | 6.1/10 fair | 3.3/10 | 10/10 |

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
            bund

... [truncated 9938 chars] ...

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
            bund

... [truncated 9938 chars] ...

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
            bund

... [truncated 9938 chars] ...

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
  1| name: CI
  3| on:
  4|   push:
  5|     branches:
  6|       - main
  7|       - release-*
  8|   pull_request:
  9|     branches:
 10|       - main
 11|       - release-*
 12|   merge_group:
 13|     branches:
 14|       - main
 15|       # - release-*
 17| permissions:
 18|   contents: read
 20| # Ensure scripts are run with pipefail. See:
 21| # https://docs.github.com/en/actions/using-workflows/workflow-syntax-for-github-actions#exit-codes-and-error-action-preference
 22| defaults:
 23|   run:
 24|     shell: bash
 26| jobs:
 27|   test:
 28|     strategy:
 29|       fail-fast: ${{ github.event_name == 'merge_group' }}
 30|       matrix:
 31|         config:
 32|           # PRs only check the newest and oldest Node versions.
 33|           # macOS only ever checks the neest and oldest Node versions, but never in PR runs.
 34|           - os: ubuntu-latest
 35|             node-version: '24'
 36|             bundle: true
 37|           - os: windows-latest
 38|             node-version: '24'
 39|             bundle: true
 40|             skip: ${{ github.event_name == 'merge_group' }}
 41|           - os: macos-latest
 42|             node-version: '24'
 43|             bundle: true
 44|             skip: ${{ github.event_name == 'pull_request' || github.event_name == 'merge_group' }}
 46|           - os: ubuntu-latest
 47|             node-version: '22'
 48|             bundle: true
 49|             skip: ${{ github.event_name == 'pull_request' || github.event_name == 'merge_group' }}
 50|           - os: windows-latest
 51|             node-version: '22'
 52|             bundle: true
 53|             skip: ${{ github.event_name == 'pull_request' || github.event_name == 'merge_group' }}
 55|           - os: ubuntu-latest
 56|   

... [truncated 11359 chars] ...

onclusion == 'failure' }}
393|         uses: actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a # v7.0.1
394|         with:
395|           name: fix_baselines.patch
396|           path: fix_baselines.patch
398|   required:
399|     runs-on: ubuntu-latest
400|     if: ${{ always() }}
401|     needs:
402|       - test
403|       - coverage
404|       - lint
405|       - knip
406|       - format
407|       - typecheck
408|       - smoke
409|       - package-size
410|       - misc
411|       - self-check
412|       - baselines
414|     steps:
415|       - name: Check required jobs
416|         env:
417|           NEEDS: ${{ toJson(needs) }}
418|         run: |
419|           ! echo $NEEDS | jq -e 'to_entries[] | { job: .key, result: .value.result } | select((.result == "success" or .result == "skipped") | not)'
```
