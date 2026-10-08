# Codex plugin package

This package owns distribution metadata, onboarding, and assembly of public skills. Tool behavior, credential resolution, and path security remain in the existing native runtime. The MCP package remains the stdio adapter.

## Build inputs

- `src/plugin.json` owns plugin identity and presentation; `package.json` owns the release version.
- `src/skills/octocode-get-started/` owns plugin onboarding. Build substitutes the CLI version and Node.js requirement from the CLI package.
- Root `skills/` owns all public workflows. Reuse `packages/octocode/scripts/stage-skills.mjs`, including per-skill file lists. Beta and repository development skills are outside this source collection.
- The MCP package's version becomes an exact `npx -y octocode-mcp@VERSION` pin in generated `mcp.json`. No token variables or fixed working directory are embedded.
- Root `assets/logo.png` and `LICENSE` supply existing branding and licensing.

Build prerequisites are the normal config and CLI builds. The workspace task runner orders this package after `octocode`. Generated plugin files, copied skills, and assets stay ignored; the npm archive contains their built contents at its root. Codex does not run npm lifecycle scripts when downloading a plugin.

## Develop and verify

From the repository root:

```sh
node skills-dev/octocode-dev/scripts/dev.mjs build:dev
yarn workspace @octocodeai/codex-plugin verify
yarn workspace @octocodeai/codex-plugin test:smoke
```

For a focused rebuild after prerequisites are built, use `yarn workspace @octocodeai/codex-plugin build:dev`.

The package tests validate the upstream Agent Plugins schemas, release pins, skill coverage, local script imports, and an extracted npm archive. The smoke check uses an isolated Codex home and temporary marketplace to install the built package. Its MCP check explicitly uses the local built server, since the registry pin may not be published yet. It tests project isolation and synthetic GitHub CLI authentication against a loopback fixture; it does not read real credentials or prove registry installation or other operating systems.

Before broad release, test the published pin on each supported operating system, in a fresh desktop chat, with a user-authorized GitHub account and private repository. Check worktrees, missing login, expired credentials, permission denial, duplicate skill installations, updates, and removal. Optional services need their own workflow checks.

## Release

1. Complete the repository release gates and publish compatible runtime packages in the order in `skills-dev/octocode-dev/docs/RELEASE.md`.
2. Set the plugin version in this package and the same exact version in `.agents/plugins/marketplace.json`.
3. Build and verify this package (`yarn verify`), and confirm both pinned runtime packages exist on npm.
4. From this package directory, run `npm pack` and inspect the archive. After owner approval, run `npm publish`.
5. Make the catalog update available on GitHub after the package exists. Test the real marketplace installation against the published artifacts before announcing it.

The catalog uses npm as its plugin source so GitHub does not need committed copies of generated skill bundles. Its name is `octocode`, and the plugin selector is `octocode@octocode`. A narrow root ignore exception tracks only the marketplace catalog under `.agents`.

## Upstream references

- [OpenAI plugin packaging](https://developers.openai.com/plugins/build/plugins): portable manifests, npm-backed catalogs, path rules, and public-directory boundaries.
- [Agent Plugins schemas](https://agent-plugins.org/schemas/1.0.0/plugin.schema.json): immutable-version manifest contract, with the corresponding MCP schema in the same directory. Test snapshots preserve the original schemas; update them only when intentionally adopting another spec version.
- [Plugin testing](https://developers.openai.com/plugins/deploy/connect-chatgpt): verify the installed skills and MCP together.
- [Plugin guidelines](https://developers.openai.com/plugins/plugin-guidelines): accurate descriptions, scoped activation, credential protection, and publication requirements.
