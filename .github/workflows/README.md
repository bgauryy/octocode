# GitHub Actions workflows

This directory contains the active GitHub Actions workflows for the Octocode monorepo.

## Overview

| Workflow | Trigger | Purpose |
|---|---|---|
| `ci.yml` | Pull requests and pushes to `main` | Documentation, lint, build-output, typecheck, test, and coverage checks |
| `engine.yml` | Engine-related pull requests and pushes to `main` | Engine Rust tests, Clippy, fmt, and the N-API ABI check against the committed snapshot |
| `rust-tools-core.yml` | Native package pull requests and pushes to `main` | cargo-deny, per-OS test & Clippy |
| `agents-communication.yml` | Communication skill changes, pushes to `main`, manual dispatch | Python runtime across OS targets plus the declared Python 3.9 minimum |
| `skill-installer-windows.yml` | Skill-installer changes, pushes to `main` | Windows installer and junction behavior |

## CI (`ci.yml`)

The main workflow runs one ordered `Lint, Build & Test` job. It installs with
the immutable lockfile, verifies documentation, runs the CI lint profile,
builds the TypeScript packages, checks build outputs, then runs the CI
typecheck and test profiles. It uploads package coverage even when a preceding
check fails.

CI never builds native artifacts: every `*:ci` profile excludes
`@octocodeai/octocode-native`, and no workflow builds addons, binaries, or
platform packages. Build those locally (`build:dev`, `build:target <platform>`,
`build:all`, or `dev.mjs build:publish` for a release).

The engine workflow runs only when engine paths change. It runs `cargo fmt`,
checks the N-API ABI against the committed snapshot, runs Clippy with warnings
denied, and executes Cargo tests.

Useful local commands before opening a PR:

```bash
node skills-dev/octocode-dev/scripts/dev.mjs health:check
node skills-dev/octocode-dev/scripts/dev.mjs docs:verify
node skills-dev/octocode-dev/scripts/dev.mjs lint
node skills-dev/octocode-dev/scripts/dev.mjs typecheck
node skills-dev/octocode-dev/scripts/dev.mjs build
node skills-dev/octocode-dev/scripts/dev.mjs test
```

To run the full repository contract in one command, use:

```bash
node skills-dev/octocode-dev/scripts/dev.mjs verify
```

## Manual Releases

npm publishing, Homebrew tap updates, and standalone binary uploads are manual.
Use the [release guide](../../releases/README.md) for the current executable
release order and verification checklist.

## Maintenance Notes

- Keep this file aligned with the actual workflow files in this directory.
- `node skills-dev/octocode-dev/scripts/dev.mjs docs:verify` fails if this README references a workflow that does not exist.
