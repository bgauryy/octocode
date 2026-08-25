# GitHub Actions Workflows

This directory contains the active GitHub Actions workflows for the Octocode monorepo.

## Overview

| Workflow | Trigger | Purpose |
|---|---|---|
| `ci.yml` | Pull requests, pushes to `main` | Docs, lint, build, verify outputs, typecheck, test for every package except `octocode-engine`/`octocode-benchmark` |
| `engine.yml` | Pull requests/pushes touching `packages/octocode-engine/**` | Rust clippy, cargo tests, NAPI ABI drift guard, native-required vitest suite |

## CI (`ci.yml`)

The pull request workflow runs a single job (`ci`) in this order:

1. `yarn docs:verify`
2. `yarn lint:ci`
3. `yarn build:ci`
4. `node scripts/workspace-health.mjs check-outputs`
5. `yarn typecheck:ci`
6. `yarn test:ci`, then uploads per-package coverage artifacts

The `:ci`-suffixed scripts exclude `octocode-engine` (Rust/native, covered by `engine.yml` instead) and `octocode-benchmark`. Each of them also runs the same workspace script-contract check as `yarn health:check` (every package must declare `build`/`lint`/`test`/`typecheck`/`verify`) as a side effect of `scripts/workspace-health.mjs run <script>`, so there's no separate health step to run.

Useful local commands before opening a PR:

```bash
yarn docs:verify
yarn lint:ci
yarn build:ci
node scripts/workspace-health.mjs check-outputs
yarn typecheck:ci
yarn test:ci
```

If you want the full repo contract (including `octocode-engine`/`octocode-benchmark`) in one command, run:

```bash
yarn verify
```

## Manual Releases

npm publishing, Homebrew tap updates, and standalone binary uploads are manual.
Use the Release Guide for the current release order and verification checklist.

## Maintenance Notes

- Keep this file aligned with the actual workflow files in this directory.
- `yarn docs:verify` fails if this README references a workflow that does not exist.
