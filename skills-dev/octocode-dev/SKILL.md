---
name: octocode-dev
description: "Load this skill first for any development request in the Octocode monorepo and in every cloud session: building, testing, linting, regenerating the tool contract, changing or auditing a tool or setting, profiling, preparing a release. It owns the task runner, the request flow and the developer docs."
---

# Octocode Dev

Run from the repo root with `$DEV` and `$OCTO` as AGENTS.md defines them.

## Request flow

Find the owning layer, its callers and its tests ([where code lives](docs/DEVELOPMENT.md#where-code-lives)). Write the failing test, fix the root cause, remove what the change made dead, update the docs that describe it, then validate through the interface that changed. A lesson the next session would need goes into this skill or its docs. A cloud session delivers one pull request per request from its own branch, its description listing the checks run and the unchecked scope; follow-up fixes go to the same pull request. A local session reports the green point.

## Task runner

`$DEV --help` lists every task; extra arguments go to the task's last step. [scripts/README.md](scripts/README.md) maps tasks to scripts.

| When | Run |
|---|---|
| First checkout | `yarn install`; against the sibling core `$DEV setup --install` (`--reset --install` restores the npm pin) |
| One crate's tests | `$DEV test:dev --crate <name> [--lib] -- <filter>`: the package name with or without `octocode-` (`tool-github`, `runtime-base`); `crates/runtime` is `native` |
| Handing over a change | `$DEV check` (static only) with a focused test; Rust alone: `$DEV lint:rust` |
| A skill or doc changed | `$DEV skills:check`, `$DEV docs:verify` |
| Cross-package handoff | `$DEV verify` |
| Release candidate, end of a multi-agent round | `$DEV gates` ([orchestration](references/orchestration.md)) |
| Behavior after a build | [CHECKS.md](../../packages/octocode-benchmark/CHECKS.md): direct calls judged by reading, no harness scripts |

## Pitfalls

- `$DEV format` runs every workspace `format` script, and native's is `cargo fmt --all`. Format only your files: `rustfmt --edition 2024 <file>`, `npx prettier --write <file>`.
- `$DEV profile --build`, `cargo dev-test` and `bacon` run cargo outside the build slots.
- A `-p` or feature selection other than `test:rust`'s compiles a second copy of the crates; `$DEV test:dev` reuses that selection.
- A change in `tools-shared`, `runtime-base` or `github` also needs the tests of the crates that use it (`tool-local`, `tool-github`, `native`).
- `scripts/tool-inventory.mjs` reads the generated contract; regenerate first if core changed.

## Read next

- Crate map, contract pipeline, build and lint internals, Rust dev tools, profiling: [DEVELOPMENT.md](docs/DEVELOPMENT.md)
- Changing, testing or auditing a tool: [TOOL_QUALITY.md](docs/TOOL_QUALITY.md)
- A setting, credential or provider key: [ADDING_CONFIG.md](docs/ADDING_CONFIG.md)
- The `skills/octocode-scraping` scripts or a new vendor: [SCRAPING_MAINTENANCE.md](docs/SCRAPING_MAINTENANCE.md)
- Delegating to several agents: [orchestration](references/orchestration.md)
- The graph engine or its language battery: the `octocode-graph-dev` skill
