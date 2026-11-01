# Octocode config

`@octocodeai/config` is the private workspace package that owns Octocode
home-directory resolution, `.env` policy, the configuration contract
(`config-contract.json`), and the generated tool contract. The CLI launcher, MCP
server, extensions, and standalone skills bundle it. The native runtime resolves
settings from the same contract; `npx octocode config` shows the result.

The root entry uses Node builtins only. The `./schema` (research tools) and
`./chrome` (Chrome DevTools) subpaths re-export `@octocodeai/octocode-core` for
interfaces.

## Public API

- `getOctocodeHome(env?)` resolves `OCTOCODE_HOME` or the platform default.
- `loadOctocodeEnv(options)` loads the home and workspace `.env` files; `applyOctocodeEnv(map, options)` applies them under the protected-key policy; `propagateOctocodeEnv(options)` does both.
- `CONFIG_FIELDS` (every setting's metadata), `DEFAULT_CONFIG` (generated defaults), `configFieldEnvName(path)` (a setting's env variable), and `ENV_TOKEN_VARS` (GitHub token names, priority order) come from the contract.
- `devOverridesAllowed(env, { bundled })` gates the dev-only `OCTOCODE_NATIVE_BIN` override for the launchers; `RuntimeSurface` names the runtime surfaces.

Do not reimplement these rules in a consuming package.

## Environment fallback

```ts
import { getOctocodeHome, propagateOctocodeEnv } from '@octocodeai/config';

const env = { ...process.env };
propagateOctocodeEnv({
  home: getOctocodeHome(env),
  cwd: process.cwd(), // loads <cwd>/.octocode/.env; pass trusted: false to skip it
  env,
});
```

The result lists key names and source files, never values. Precedence,
home-only keys, and the blank-classification-key opt-out are in the
[configuration reference](../../docs/CONFIGURATION.md#env--environment-fallback).

## Development

From the repository root:

```bash
yarn workspace @octocodeai/config generate:config-contract
node skills-dev/octocode-dev/scripts/dev.mjs contracts:regen   # repo root: refresh core, regenerate contract/ (needs cargo-typify 0.8.0)
yarn workspace @octocodeai/config build
yarn workspace @octocodeai/config test
yarn workspace @octocodeai/config lint
```

Configuration fields, defaults, environment bindings, trust policy, and
validation constraints are edited only in `config-contract.json`. The
TypeScript generator and native Rust build script independently validate that
file against `config-contract.schema.json`. The TypeScript generator emits the
field metadata and defaults this package exports and the user settings
reference; the native build emits the resolver the runtime uses.

Tool wire types are generated, never hand-written:
`@octocodeai/octocode-native` embeds `contract/` (enforcement contract,
fixtures, and Rust types) directly at build time, and TypeScript consumers use
the core Zod schemas from `@octocodeai/config/schema`. Change the Zod schema in
`@octocodeai/octocode-core`, then run `node skills-dev/octocode-dev/scripts/dev.mjs contracts:regen` — that one step
updates every consumer. See [ARCHITECTURE.md](./ARCHITECTURE.md#tool-types).

See the repository [configuration reference](../../docs/CONFIGURATION.md),
[generated settings reference](../../docs/generated/CONFIG_SETTINGS.md),
[contributor guide](../../skills-dev/octocode-dev/docs/ADDING_CONFIG.md), and
[security model](../../docs/SECURITY.md).

## License

MIT
