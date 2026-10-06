# Release checklist

Publishing is owner-triggered. It never runs from CI or an agent session. Every gate is machine-checked except the config-pin gate. Contract pipeline and build commands: [DEVELOPMENT.md](DEVELOPMENT.md#contract-pipeline). Native platform packages: `<repo>/packages/octocode-native/docs/PUBLISHING.md`.

## Order of operations

Core publishes before the packages that embed its contracts: the native binary embeds the contract that `@octocodeai/config` generates from core (`yarn contracts:regen`), so a stale published core makes a clean `npm install` diverge from the shipped binary. Native and its platform packages must be on npm before mcp and the launcher. The plugins (steps 8–9) wait until the pinned MCP and launcher versions are available.

1. **Core** (`octocode-mcp-host` repo): commit a clean tree. Publish `@octocodeai/octocode-core` at the version pinned in `packages/octocode-config/package.json` `dependencies`. Check it with `npm view`.
2. **Switch to the registry**: `node skills-dev/octocode-dev/scripts/prepublish.mjs --fix`, `yarn install`, `node skills-dev/octocode-dev/scripts/dev.mjs prepublish`, then `yarn workspace @octocodeai/config check:core-contract-sync:published`.
3. **Config** (`@octocodeai/config`) at a **new** version. npm never accepts a version twice; an already-published number ships the old build. Config is 20.1.0 while every other package is 20.0.0: `@octocodeai/config@20.0.0` is already on npm as an old build, so a version alignment never moves config back to 20.0.0.
4. **Native platform packages** (`packages/octocode-native/npm/*`), after `yarn build:native:all` and `yarn platforms:check`.
5. **Native** (`@octocodeai/octocode-native`).
6. **MCP** (`octocode-mcp`).
7. **Launcher** (`octocode`).
8. **Codex plugin** (`@octocodeai/codex-plugin`). Follow `<repo>/packages/octocode-codex-plugin/ARCHITECTURE.md`, then update `.agents/plugins/marketplace.json` to the published plugin version.
9. **Claude Code plugin** (`@octocodeai/claude-plugin`). Follow `<repo>/packages/octocode-claude-plugin/ARCHITECTURE.md`, then publish `.claude-plugin/marketplace.json` with the matching npm version.

Then smoke-test from a clean temp dir (`npx -y octocode-mcp@<v>`, `npx -y octocode@<v> scheme --compact`). Run `node skills-dev/octocode-dev/scripts/dev.mjs setup` to restore the dev resolutions.

- **Use `npm publish`, not `yarn npm publish`.** Only npm runs each package's `prepublishOnly` guard. Every package rejects `workspace:`/`file:` dependencies; native also runs `version:check` and the published-contract sync.
- **Config-pin gate (manual):** before you publish mcp or the launcher, their `@octocodeai/config` dependency must name the config version from step 3, and `npm view @octocodeai/config@<pin> exports` must list `./schema`. A pin to an older config breaks `@octocodeai/config/schema` on a clean install.

## Gates (all must be green)

| Gate | Command | What it proves |
|---|---|---|
| Full verify | `yarn workspace @octocodeai/octocode-native verify` | crate boundaries, fmt, clippy, tests, loader/ABI/version checks, contract freshness, doc claims |
| Contract freshness (dev) | `yarn workspace @octocodeai/config check:tool-contract` | `packages/octocode-config/contract/` (embedded by native) is exactly what the resolved core generates |
| Contract sync (published) | `yarn workspace @octocodeai/config check:core-contract-sync:published` | Embedded contracts match the **npm-published** core at the pinned version. Runs in native's `prepublishOnly`; blocks publish only, never dev |
| Docs drift | `yarn workspace @octocodeai/octocode-native docs:claims` | README exit codes, tool count, and `CONFIGURATION.md` env names match source; no retired pre-v20 CLI grammar |
| Docs links and catalog | `node skills-dev/octocode-dev/scripts/dev.mjs docs:verify` (repo root) | Doc links, workflow references, tool catalog and counts, and config keys match source |
| Launcher e2e | `yarn workspace octocode exec vitest run tests/e2e/launcher.e2e.test.ts` after `build:dev` (local; not in CI) | The built npm launcher drives the real native binary end-to-end |
| Version consistency | `yarn workspace @octocodeai/octocode-native version:check` | Native Cargo crates, `optionalDependencies`, and platform packages share the native version |

## Failure playbook

| Failure | Action |
|---|---|
| `Generated tool contract is stale` | Core changed since the last regen. Run `yarn contracts:regen` (repo root) and re-verify. |
| `contract-sync: FINGERPRINT MISMATCH` in `--published` mode | Publish the matching core first (step 1), or align the pinned core version. Never publish native around this gate. |
| Docs-drift failure | Fix the doc or the source. If a pin false-positives twice in a month, shrink the claim set in `packages/octocode-native/scripts/check-doc-claims.cjs`. Never drop the gate. |
