# Pre-release audit: octocode-core and the release chain

Date: 2026-09-30. Read-only audit: nothing was published, bumped, committed or edited in product files. Round-3 optimization work changed core and native during this run, so items marked [in-flight] are snapshots, not final verdicts.

**Overall: NO-GO.**

| Package | Local | npm latest | Verdict |
|---|---|---|---|
| @octocodeai/octocode-core | 19.1.6 (unpublished) | 19.1.5 | NO-GO until round 3 lands. HEAD is green, but the working tree is what gets embedded. |
| @octocodeai/config | 20.0.0 | **20.0.0 already published** (an old build without `./schema`) | NO-GO |
| @octocodeai/octocode-native + 6 platform packages | 20.0.0 | never published | NO-GO |
| octocode-mcp | 19.2.0 | 19.1.0 | NO-GO: its config pin resolves to the old published config |
| octocode (launcher) | 19.2.0 | 19.1.0 | NO-GO |

## Blockers
- **B1 (High, in-flight): core lint is red, so `prepublishOnly` aborts.** There are 10 eslint errors: prettier formatting plus the unused variables `_files`, `_sha` and `schema`. HEAD alone is clean (340/340). Fix: `yarn lint:fix`, remove the unused variables, then run lint, typecheck, test and build, and commit.
- **B2 (High): the contract fingerprint isn't tied to a core state.** Core 19.1.6 produced 4 different fingerprints in one hour. `provenance.json` records only the version, with no core SHA or dirty flag. Fix: freeze and commit core, publish from that commit, then regen and rebuild native. Consider adding the core git SHA and a dirty flag to provenance.
- **B3 (High): config@20.0.0 is already on npm** as an old build with only the `"."` export. mcp and the launcher pin 20.0.0, so a clean install breaks `@octocodeai/config/schema`. Fix: bump config (for example to 20.1.0) and update both pins.
- **B4 (High): native's publish gate can never pass.** `packages/octocode-config/scripts/check-core-contract-sync.cjs` reads the core pin from the root `package.json`, which has no dependencies; `prepublish --fix` also deletes the `file:` resolution. `docs/RELEASE.md` repeats the wrong assumption. Fix: read the pin from `packages/octocode-config/package.json` dependencies.
- **B5 (High): the `packages/octocode-native/npm/verify-binary.cjs` smoke queries omit `goal`,** so they exit 2 with "goal: Missing required field". Fix: add `goal` to the smoke queries.
- **B6 (High): platform binaries are missing or stale.** 5 of 6 platforms have none, and the darwin-arm64 `.node` files date from 09-28. Fix: `build:all`, then `yarn platforms:check`.
- **B7 (Medium): config tests fail at HEAD (3 of 261).** `PROTECTED_KEYS` is missing the two storage-mode keys, and two rc-layer tests still expect workspace `storage.mode` to win. Storage mode is home-trusted since commit 17375b97a.
- **B8 (Medium): `cargo fmt --check` fails in 6 native files**, so native `verify` fails. Fix: `cargo fmt --all`.
- **B9 (Medium): the `scheme` usage line shows `[goal]` as optional**, but the runtime requires it on new queries. The launcher test `scheme.test.ts:213` fails.

## Warnings
- **W1:** the core `files` list omits `dist/public-catalog.json`, so the catalog is rebuilt on cold start.
- **W2:** the core README says `./mcp` exports `SYSTEM_PROMPT`; it doesn't. Its `parse()` example fails.
- **W3:** core's `"."` export lists `import` before `types` and exports nothing, and there is no top-level `types` or `main`.
- **W4:** core, native and the platform packages have no LICENSE file, although `package.json` says MIT.
- **W5:** each release binary embeds about 150 `/Users/bgaryy/...` paths. Fix: `trim-paths` or `--remap-path-prefix`.
- **W6:** the config tarball ships 19 `.map` files that point at a `src/` that isn't shipped.
- **W7:** the launcher's `bundledDependencies` bundles nothing.
- **W8:** `packages/octocode/out` is a dev build, and mcp's `dist/docs/TOOL_DATA_CONTRACT.md` is stale. Both need a full build.
- **W9:** core-repo docs still say `ghSearch`: `docs/AGENT_RESEARCH.md:17-18` and `docs/ENVIRONMENT_VARIABLES.md:76`. `CHANGELOG.md` was last touched 09-10. `RELEASE.md` leaves config, the platform packages and mcp out of the publish order.
- **W10:** native `docs:claims` walks the gitignored `octocode-local-testing` directory.
- **W11:** the darwin-arm64 tarball is 55 MB packed and 245 MB unpacked.
- **W12:** core was bumped to 19.1.7 and reverted to 19.1.6. Publish only the final state.
- **W13:** Node engine ranges differ across packages: `>=18.12`, `>=24.15.0` and `^24.15.0`.
- **W14:** the launcher's `workspace:*` devDependency ships verbatim in its published `package.json`.
- **W15:** native and the 6 platform packages have never been published, so they must go out before mcp and the launcher.

## Passed
- **Core:**
  - HEAD passes 340/340 tests, typecheck and eslint, and the Zod-effect audit passes.
  - `npm pack` is 136 files and 110 KB. Only `dist` js/d.ts and the README ship, with no tests, secrets or absolute paths. The only runtime dependency is `zod`.
- **Consumers and contract:**
  - Every consumer import from core exists.
  - A scratch regen matched the staged contract byte for byte.
  - `check:tool-contract` and `check:config-contract` pass.
- **MCP and launcher:**
  - The MCP starts and lists 13 tools, and it fails closed on a fingerprint mismatch (seen live).
  - mcp tests pass 230/230.
  - The launcher e2e test passes.
- **Native:**
  - Node tests pass 109/109.
  - Clippy is clean.
  - Engine tests pass 862/862.
- **Release scripts:**
  - `version:check`, `pack:check` and `docs:verify` pass.
  - The publish guards correctly reject local resolutions.
- **Security:** no tokens or `.env` files in any tarball or binary.

## Release steps for the owner, after the blockers are fixed
Use `npm publish`, not `yarn npm publish`, so that `prepublishOnly` runs.
1. **Core:** run `yarn lint:fix && yarn lint && yarn typecheck && yarn test && yarn build`, commit, and confirm a clean tree.
2. **Monorepo:**
   1. Run `yarn install && yarn contracts:regen`, then `check:tool-contract`.
   2. Fix B3–B5, B7–B9.
   3. Build native with `build:all`, then run `platforms:check`.
   4. Run native and config `verify`.
   5. Do full builds of mcp and the launcher.
   6. Run the `verify-binary` and `tools/list` smoke tests, then commit.
3. **Publish core** with `npm publish`, then check it with `npm view`.
4. **Switch to the registry:** run `node scripts/prepublish.mjs --fix`, `yarn install`, `yarn prepublish`, then `check:core-contract-sync:published`.
5. **Publish the rest, in order:**
   1. config, at a new version;
   2. the `packages/octocode-native/npm/*` platform packages;
   3. native;
   4. mcp;
   5. the launcher.
6. **Smoke-test from a clean temp dir:** `npx -y octocode-mcp@<v>` and `npx -y octocode@<v> scheme --compact`. Then run `yarn devScript` to restore the dev resolutions.
