# GitHub Actions Workflows

This directory contains the active GitHub Actions workflows for the Octocode monorepo.

## Overview

| Workflow | Trigger | Purpose |
|---|---|---|
| `ci.yml` | Pull requests | Repo health, lint, typecheck, build, test |

## CI (`ci.yml`)

The pull request workflow runs two parallel jobs to minimize wall-clock time:

1. `checks` — Health, Lint & Typecheck
   Runs `yarn health:check`, `yarn docs:verify`, `yarn lint`, builds shared types, then `yarn typecheck`.
2. `build-and-test` — Build & Test
   Runs `yarn build`, verifies outputs, then `yarn test` and uploads per-package coverage artifacts.

Useful local commands before opening a PR:

```bash
yarn health:check
yarn docs:verify
yarn lint
yarn typecheck
yarn build
yarn test
```

If you want the full repo contract in one command, run:

```bash
yarn verify
```

## Manual Releases

npm publishing, Homebrew tap updates, and standalone binary uploads are manual.
Use the Release Guide for the current release order and verification checklist.

## Maintenance Notes

- Keep this file aligned with the actual workflow files in this directory.
- `yarn docs:verify` fails if this README references a workflow that does not exist.
