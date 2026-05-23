# Scripts

Root-level automation scripts for the Octocode monorepo.

## Scripts

### `workspace-health.mjs`

Workspace orchestration tool that discovers all packages and skills, resolves their dependency graph, and runs tasks in topological order.

**Modes:**

| Mode | Command | Description |
|------|---------|-------------|
| `report` | `yarn health:report` | Print script matrix for all workspaces |
| `check` | `yarn health:check` | Verify every workspace has its required scripts |
| `check-outputs` | _(direct)_ | Verify expected build artifacts exist |
| `run <script>` | `yarn build`, `yarn test`, `yarn lint`, `yarn typecheck` | Run a named script across all workspaces in dependency order |
| `verify` | `yarn verify` | Full CI-grade check: contracts, docs, build, lint, test, typecheck, outputs |

### `docs-verify.mjs`

Documentation link validator. Ensures every markdown link inside `docs/` uses absolute GitHub URLs (per the [Documentation Links Rule](https://github.com/bgauryy/octocode-mcp/blob/main/AGENTS.md)) and that in-repo links point to files that actually exist. Also validates that `.github/workflows/README.md` only references workflow files that are present on disk.

**Command:** `yarn docs:verify`
