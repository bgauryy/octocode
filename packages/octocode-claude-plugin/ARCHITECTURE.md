# Claude Code plugin packaging

`@octocodeai/claude-plugin` is a separate public npm distribution. It contains Claude's manifest, the local MCP launch configuration, every public skill, and a small host-specific onboarding skill. It owns no tool execution or authentication implementation.

## Build inputs and outputs

`scripts/build.ts` stages canonical `skills/` with the CLI's `stageSkills` helper, adds `src/skills/`, and substitutes the current CLI version and Node.js range in onboarding. Build the CLI first so generated skill helpers exist. It emits `.claude-plugin/plugin.json`, `.mcp.json`, `skills/`, and `LICENSE` in this package. Generated copies are ignored by Git and included explicitly in the npm archive. Do not edit them directly.

`package.json` owns the plugin version. `src/plugin.json` owns Claude metadata. The build pins the current `octocode-mcp` version exactly; it leaves credentials and working directory out of the manifest. MCP inherits the user's active project context and uses native credential discovery. No install lifecycle script is required on a user's machine.

The repository's `.claude-plugin/marketplace.json` is a GitHub-hosted catalog pointing to this exact npm package version. Claude supports an npm **plugin source**, but an npm **marketplace source** is not implemented. Users add `bgauryy/octocode`, then install `octocode@octocode`. Keep the catalog's version synchronized with the package.

## Validate locally

From the monorepo root:

```sh
yarn workspace octocode build:dev
yarn workspace octocode-mcp build:dev
yarn workspace @octocodeai/claude-plugin verify
yarn workspace @octocodeai/claude-plugin validate
yarn workspace @octocodeai/claude-plugin test:smoke
node skills-dev/octocode-dev/scripts/dev.mjs docs:verify
```

The archive tests check public-skill completeness, relative script imports, forbidden private/build files, and the extracted skill navigation. `validate` uses Claude's authoritative validator in strict mode for the plugin and marketplace. The smoke test installs the actual npm archive through a loopback registry into a temporary Claude configuration, checks the installed manifest and skills, then uses the built local MCP to read two isolated project roots and reject unrelated plugin paths. It never uses the user's GitHub or Claude credentials. Claude Code 2.1.286 was used during development.

The smoke test explicitly substitutes the local MCP build for the unpublished runtime. Public-registry startup, interactive skill invocation, and Windows/Linux behavior need release validation; an npm fixture install does not establish them.

## Publish the npm package and marketplace

1. Follow `<repo>/skills-dev/octocode-dev/docs/RELEASE.md` to publish compatible config/native, MCP, and CLI packages first. Do not change runtime pins to an older version without compatibility testing.
2. Set the next plugin version in `package.json` and the catalog's npm source. Run the checks above and `node packages/octocode-claude-plugin/scripts/release-check.ts`. The guard checks that both pinned runtime packages exist on npm.
3. Inspect the archive with `npm pack --dry-run` in this package. From this package directory, run `npm publish --access public`. Use npm so `prepublishOnly` runs; it validates the bundle, Claude manifests, and runtime availability.
4. After npm publication succeeds, publish the matching catalog to the default branch of `bgauryy/octocode`. Do not expose a catalog version before its package exists.
5. In a clean user profile, add the GitHub marketplace, install the plugin, run `/octocode:octocode-get-started`, and verify local reads plus a permitted GitHub read using `gh` login. Confirm the actual registry-pinned MCP starts. Repeat on supported operating systems before claiming support.

For every update, increment the plugin version, rebuild, publish npm first, then update the GitHub catalog. Users can run `claude plugin update octocode@octocode`.

## Anthropic directory submission

This marketplace is independently hosted. npm publication does not list the plugin in Anthropic's directory or `claude-plugins-official`.

For a directory listing, use [the developer portal](https://claude.ai/directory/manage) and choose Plugin bundle. The portal reads a GitHub repository and plugin folder, not the npm tarball. Prepare a separate release directory or repository containing the **extracted npm artifact**, including its generated manifest, skills, README, and license; publish that directory to GitHub. Do not submit this source package path while its generated outputs remain Git-ignored. Select that artifact folder and the release branch/tag in the portal, validate, complete the listing/data-handling details, and submit for review. The repository must be public before the listing goes live.

The portal applies additional checks and security review. Review the whole skill bundle and optional services against its checklist; CLI validation is not directory approval. Existing local credential discovery may require reviewer explanation. Keep the local GitHub CLI model explicit rather than silently collecting tokens through plugin configuration. Confirm component support for each Claude surface before advertising it. For `claude-plugins-official`, the current publishing guide directs publishers to an Anthropic partner contact; directory submission is a different route.

## Official sources

Checked 2026-10-01:

- [Manifest and authoritative validation](https://code.claude.com/docs/en/plugins-reference)
- [Marketplace reference and npm plugin sources](https://code.claude.com/docs/en/plugins/marketplace-reference)
- [Publish and distribute](https://code.claude.com/docs/en/plugins/publish)
- [Directory submission](https://claude.com/docs/plugins/submit)
- [Directory pre-submission checklist](https://claude.com/docs/plugins/pre-submission-checklist)
- [GitHub CLI credential storage](https://cli.github.com/manual/gh_auth_login)
