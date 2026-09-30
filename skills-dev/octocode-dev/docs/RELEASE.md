# Release checklist

Publishing is owner-triggered and never runs from CI or an agent session.
Every gate below is machine-checked except the config-pin gate; a release
that skips one reproduces a silent-state incident from the 2026-09 audits.
The contract pipeline and build commands are in
[DEVELOPMENT.md](DEVELOPMENT.md#contract-pipeline); native platform-package
details are in
`<repo>/packages/octocode-native/docs/PUBLISHING.md`.

## Order of operations

Core publishes **before** the packages that embed its contracts. The native
binary embeds the contract `@octocodeai/config` generated from core
(`yarn contracts:regen`), so a stale published core makes a clean
`npm install` diverge from the shipped binary. Native and its platform
packages must be on npm before mcp and the launcher, which depend on them.

1. **Core** (`octocode-mcp-host` repo): commit a clean tree, then publish
   `@octocodeai/octocode-core` at the version pinned in
   `packages/octocode-config/package.json` `dependencies`. Check it with `npm view`.
2. **Switch to the registry**: `node skills-dev/octocode-dev/scripts/prepublish.mjs --fix`,
   `yarn install`, `node skills-dev/octocode-dev/scripts/dev.mjs prepublish`, then
   `yarn workspace @octocodeai/config check:core-contract-sync:published`.
3. **Config** (`@octocodeai/config`) at a **new** version; npm never accepts a
   version twice, so an already-published number ships the old build.
4. **Native platform packages** (`packages/octocode-native/npm/*`), after
   `yarn build:native:all` and `yarn platforms:check`.
5. **Native** (`@octocodeai/octocode-native`).
6. **MCP** (`octocode-mcp`).
7. **Launcher** (`octocode`).

Then smoke-test from a clean temp dir (`npx -y octocode-mcp@<v>`,
`npx -y octocode@<v> scheme --compact`) and run `node skills-dev/octocode-dev/scripts/dev.mjs setup` to restore the
dev resolutions.

**Use `npm publish`, not `yarn npm publish`:** only npm runs each package's
`prepublishOnly` guard. Every package rejects `workspace:`/`file:`
dependencies; native also runs `version:check` and the published-contract
sync.

**Config-pin gate:** before publishing mcp or the launcher, their
`@octocodeai/config` dependency must name the config version from step 3, and
`npm view @octocodeai/config@<pin> exports` must list `./schema`. A pin to an
older published config breaks `@octocodeai/config/schema` on a clean install.
This check is manual.

## Gates (all must be green)

| Gate | Command | What it proves |
|---|---|---|
| Full verify | `yarn workspace @octocodeai/octocode-native verify` | crate boundaries, fmt, clippy, tests, loader/ABI/version checks, contract freshness, doc claims |
| Contract freshness (dev) | `yarn workspace @octocodeai/config check:tool-contract` | `packages/octocode-config/contract/` (embedded by native) is exactly what the resolved core generates |
| Contract sync (published) | `yarn workspace @octocodeai/config check:core-contract-sync:published` | Embedded contracts match the **npm-published** core at the pinned version — what a clean install actually delivers. Runs automatically in native's `prepublishOnly`; blocks publish only, never dev |
| Docs drift | `yarn workspace @octocodeai/octocode-native docs:claims` | README exit codes, tool count, and `CONFIGURATION.md` env names match source; no retired pre-v20 CLI grammar |
| Docs links and catalog | `node skills-dev/octocode-dev/scripts/dev.mjs docs:verify` (repo root) | Doc links, workflow references, tool catalog and counts, and config keys match source |
| Launcher e2e | CI `launcher-e2e` job (Linux) | The built npm launcher drives the real native binary end-to-end |
| Version consistency | `yarn workspace @octocodeai/octocode-native version:check` | Native Cargo crates, `optionalDependencies`, and platform packages share the native version |

## Failure playbook

- `Generated tool contract is stale` — core changed since the last
  regeneration. Run `yarn contracts:regen` (repo root) and re-verify.
- `contract-sync: FINGERPRINT MISMATCH` in `--published` mode — publish the
  matching core first (step 1), or align the pinned core version; never
  publish native around this gate.
- Docs-drift failures — fix the doc or the source. Shrink the pinned claim
  set in `packages/octocode-native/scripts/check-doc-claims.cjs` if a pin
  false-positives twice in a month; never drop the gate.
