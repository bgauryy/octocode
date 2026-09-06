# GitHub Actions workflows

This directory contains the active GitHub Actions workflows for the Octocode monorepo.

## Overview

| Workflow | Trigger | Purpose |
|---|---|---|
| `ci.yml` | Pull requests and pushes to `main` | Documentation, lint, build-output, typecheck, test, and coverage checks |
| `engine.yml` | Engine-related pull requests and pushes to `main` | Rust tests, Clippy, native ABI, and Rust↔JavaScript parity checks |

## CI (`ci.yml`)

The main workflow runs one ordered `Lint, Build & Test` job. It installs with
the immutable lockfile, verifies documentation, runs the CI lint profile,
builds the native host engine and dependent packages, checks build outputs,
then runs the CI typecheck and test profiles. It uploads package coverage even
when a preceding check fails.

The engine workflow runs only when engine paths change. It builds the config
package and native add-on, checks the generated NAPI ABI, runs Clippy with
warnings denied, executes Cargo tests, and requires the native add-on during
the engine Vitest suite.

Useful local commands before opening a PR:

```bash
yarn health:check
yarn docs:verify
yarn lint
yarn typecheck
yarn build
yarn test
```

To run the full repository contract in one command, use:

```bash
yarn verify
```

## Manual Releases

npm publishing, Homebrew tap updates, and standalone binary uploads are manual.
Use the [release guide](../../releases/README.md) for the current executable
release order and verification checklist.

## Maintenance Notes

- Keep this file aligned with the actual workflow files in this directory.
- `yarn docs:verify` fails if this README references a workflow that does not exist.
