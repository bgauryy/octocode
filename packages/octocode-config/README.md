# Octocode config

`@octocodeai/config` is the zero-runtime-dependency configuration loader shared by the
Octocode CLI, MCP server, native packages, extensions, and standalone skills.
It is the only owner of Octocode home-directory resolution and `.env` or
`.octocoderc` parsing.

## Public API

- `getOctocodeHome(env?)` resolves `OCTOCODE_HOME` or the platform default.
- `parseEnv(text)` parses environment-file content.
- `loadOctocodeEnv(options)` loads Octocode environment values.
- `propagateOctocodeEnv(options)` applies trusted global and project settings.
- `loadOctocoderc(home?)` reads the global `.octocoderc`; `loadOctocodercLayers({ home?, cwd?, env? })` returns `[workspace, global]` for `resolveConfigFields(layers, env)` (per-field precedence). Broken files warn on stderr with their path and are ignored; nothing throws.
- `PROTECTED_KEYS` identifies values that project configuration cannot replace.

Do not reimplement these rules in a consuming package.

## Environment fallback

For each key, non-empty process values win over workspace
`.octocode/.env`, which wins over the home `.env` (default `~/.octocode/.env`).
GitHub and classification credentials can use both files. Protected infrastructure
keys remain blocked; a present-but-blank `OCTOCODE_CLASSIFICATION_API` disables
classification without taking a file fallback.

```ts
import { getOctocodeHome, propagateOctocodeEnv } from '@octocodeai/config';

const env = { ...process.env };
propagateOctocodeEnv({
  home: getOctocodeHome(env),
  cwd: process.cwd(),
  env,
});
```

Workspace loading defaults to on when `cwd` is supplied. Node embedders may explicitly opt out with `trusted: false`. Standalone CLI and MCP load workspace and global files; native `trustedProject` controls executable LSP configuration separately. Diagnostics report
key names and source files without credential values. Credential aliases first choose the highest-priority source, then their declared alias order within that source. A workspace alias can override a global canonical key.

## CLI

The package also exposes `octocode-config`:

```bash
npx @octocodeai/config --keys
npx @octocodeai/config --check OCTOCODE_HOME
```

## Development

From the repository root:

```bash
yarn workspace @octocodeai/config generate:config-contract
yarn contracts:regen   # repo root: refresh core, regenerate contract/ (needs cargo-typify 0.8.0)
yarn workspace @octocodeai/config build
yarn workspace @octocodeai/config test
yarn workspace @octocodeai/config lint
```

Configuration fields, defaults, environment bindings, trust policy, and
validation constraints are edited only in `config-contract.json`. The
TypeScript generator and native Rust build script independently validate that
file against `config-contract.schema.json`, then generate language-native types
and generic-interpreter metadata. The TypeScript generator also emits the user
settings reference.

Tool input/output types are generated, never hand-written: TypeScript
consumers import `ToolQuery<'localFetch'>`, `LocalSearchQuery`,
`GhSearchOutput`, and the rest from `@octocodeai/config/schema`, and
`@octocodeai/octocode-native` embeds `contract/` (enforcement contract,
fixtures, and Rust types) directly at build time. Change the Zod schema in
`@octocodeai/octocode-core`, then run `yarn contracts:regen` — that one step
updates every consumer. See [ARCHITECTURE.md](./ARCHITECTURE.md#tool-types).

See the repository [configuration reference](../../docs/CONFIGURATION.md),
[generated settings reference](../../docs/generated/CONFIG_SETTINGS.md),
[contributor guide](../../skills-dev/octocode-dev/docs/ADDING_CONFIG.md), and
[security model](../../docs/SECURITY.md).

## License

MIT
