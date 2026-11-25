# Package managers

Load when the workspace has no package manager or the user asks which to use. An existing one wins: `detect-workspace.mjs` reports it from `packageManager`, `devEngines` or the lockfile; two lockfiles are a conflict to settle with the user, never by deleting one.

## JavaScript

| Manager | Use when | Watch out |
|---|---|---|
| npm | Zero setup, ships with Node, simplest CI; small single-package projects | Slower installs; hoisted `node_modules` hides undeclared imports |
| pnpm | Default for new projects and monorepos: strict isolated `node_modules`, content-addressed store, workspaces with catalogs, `minimumReleaseAge` supply-chain delay (1 day by default) | A tool that assumes hoisting may need `public-hoist-pattern`; pnpm 12 reports unknown settings |
| Yarn (Berry, 4.x) | The repository already uses it; Plug'n'Play zero-installs or constraints are wanted | Needs Corepack, which Node.js stopped bundling in v25: `npm install --global corepack && corepack enable` |
| Bun | The app runs on Bun (`Bun.serve`, `bun test`), or install speed dominates; workspaces, catalogs, isolated installs for new workspaces | Runtime gaps for some Node APIs and native addons: check each dependency; pin the Bun version in CI |

Pin the manager: `"packageManager": "<name>@<version>"` in package.json (read by pnpm, Yarn and `pnpm/action-setup`), and the Node version in `.nvmrc` or the `engines` field of package.json. Commit exactly one lockfile.

| Task | npm | pnpm | Yarn | Bun |
|---|---|---|---|---|
| Install from lockfile (CI) | `npm ci` | `pnpm install --frozen-lockfile` | `yarn install --immutable` | `bun install --frozen-lockfile` |
| Add dev dependency | `npm i -D x` | `pnpm add -D x` | `yarn add -D x` | `bun add -d x` |
| Run script | `npm run x` | `pnpm x` | `yarn x` | `bun run x` |
| One-off binary | `npx x` | `pnpm dlx x` | `yarn dlx x` | `bunx x` |
| Scaffold | `npm create x@latest` | `pnpm create x` | `yarn create x` | `bun create x` |
| Workspaces | `workspaces` in package.json | `pnpm-workspace.yaml` | `workspaces` in package.json | `workspaces` in package.json |

## Other ecosystems

| Ecosystem | Manager | Lock and pin | Workspaces |
|---|---|---|---|
| Rust | Cargo | `Cargo.lock` (commit it for apps and libraries); toolchain in `rust-toolchain.toml` | `[workspace]` members and `[workspace.dependencies]` |
| Python | uv (manages Python too) | `uv.lock`; Python in `.python-version`; `uv sync --locked` in CI | `[tool.uv.workspace]` |
| Go | Go modules | `go.sum`; Go version in `go.mod` (`go` and `toolchain` lines) | `go.work` |

Poetry, pip-tools and Conda remain valid where a repository already uses them; do not migrate unasked.
