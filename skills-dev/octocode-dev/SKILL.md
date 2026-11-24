---
name: octocode-dev
description: "Load this skill first for development work in the Octocode monorepo and in every cloud session: build, test, lint, regenerate the contract, change or audit a tool or setting, profile, release. Owns the task runner, request flow and developer docs."
---

# Octocode Dev

Run from the repo root with `$DEV` and `$OCTO` as [AGENTS.md](../../AGENTS.md#work-locally) defines them.

## Request flow

1. Find the owning layer, its callers and its tests ([where code lives](docs/DEVELOPMENT.md#where-code-lives)).
2. Write a test that fails on the defect; when none can (docs, wording, provider or environment behavior), keep the reproducing command and its output as before evidence.
3. Fix the root cause at the owner, remove what the change made dead and update the docs that describe it; a lesson the next session needs goes into this skill or its docs.
4. Validate through the interface that changed ([task runner](#task-runner)); stop when that check passes.
5. Deliver: a local session reports the green point, checks run and unchecked scope ([AGENTS.md](../../AGENTS.md#report-to-the-user)) and leaves git writes to the owner; a cloud session lists the same in one pull request per request from its own branch and pushes follow-up fixes to it.

## Task runner

`$DEV --help` lists every task; extra arguments go to the task's last step. [scripts/README.md](scripts/README.md) maps tasks to scripts.

| When | Run |
|---|---|
| One crate's tests | `$DEV test:dev --crate <name> [--lib] -- <filter>`: the package name with or without `octocode-`; `crates/runtime` is `native` |
| Handing over a change | `$DEV check` (static only) with a focused test; Rust alone: `$DEV lint:rust` |
| A skill or doc changed | `$DEV skills:check`, `$DEV docs:verify` |
| Cross-package handoff | `$DEV verify` |
| Release candidate, end of a multi-agent round | `$DEV gates` |
| Behavior after a build | [CHECKS.md](../../packages/octocode-benchmark/CHECKS.md): direct calls judged by reading, no harness scripts |

## Pitfalls

- `$DEV format` runs native's `cargo fmt --all`: format only your files (`rustfmt --edition 2024 <file>`, `npx prettier --write <file>`).
- `$DEV profile --build`, `cargo dev-test` and `bacon` run cargo outside the build slots.
- A `-p` or feature selection other than `test:rust`'s compiles a second copy of the crates; `$DEV test:dev` reuses that selection.
- A change in `tools-shared`, `runtime-base` or `github` also runs its users' tests (`tool-local`, `tool-github`, `native`).
- `scripts/tool-inventory.mjs` reads the generated contract; regenerate first if core changed.

## Read next

- Crate map, contract pipeline, build and lint internals, Rust dev tools, profiling: [DEVELOPMENT.md](docs/DEVELOPMENT.md)
- Changing, testing or auditing a tool: [TOOL_QUALITY.md](docs/TOOL_QUALITY.md)
- A setting, credential or provider key: [ADDING_CONFIG.md](docs/ADDING_CONFIG.md)
- The `skills/octocode-scraping` scripts or a new vendor: [SCRAPING_MAINTENANCE.md](docs/SCRAPING_MAINTENANCE.md)
- Delegating to several agents: [orchestration](references/orchestration.md)
- The graph engine or its language battery: the `octocode-graph-dev` skill
