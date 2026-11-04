# Octocode config

`@octocodeai/config` is the private workspace package that owns Octocode
home-directory resolution, `.env` policy, the configuration contract
(Zod declaration emitting `config-contract.json`), and the generated tool contract. The CLI launcher, MCP
server, extensions, and standalone skills bundle it. The native runtime resolves
settings from the same contract; `npx octocode config` shows the result.

The root entry uses Node builtins only. The `./schema` (research tools) and
`./chrome` (Chrome DevTools) subpaths re-export `@octocodeai/octocode-core` for
interfaces.

## The one place configuration is known

Everything Octocode can be configured with is declared once, in
[`scripts/config-contract/declaration/`](scripts/config-contract/declaration/):
[`sections.ts`](scripts/config-contract/declaration/sections.ts) for settings and
[`environment.ts`](scripts/config-contract/declaration/environment.ts) for
variables that are not settings, such as API keys, endpoints and proxies.
[`schema.ts`](scripts/config-contract/schema.ts) defines what an entry can say.

| To know | Read | On an entry |
|---|---|---|
| What it is | keys, strings, URLs, paths, booleans, lists, choices | `type`, `values` |
| Its default | | `default`, `displayDefault` |
| Whether it is sensitive | masked in every output, never resolved into configuration | `credential` |
| Who may set it | any file, trusted home only, or the process only | `dotenv`, `workspaceNarrowsTo` |
| Where it appears on the settings page | provider group, where to get a key | `group`, `docsUrl` |

An API endpoint receives its key, so it must be `dotenv: 'home'`. Otherwise a
repository's `.octocode/.env` could send your key to its own host.

Everything else derives from this, and `generate:config-contract` regenerates it:

| Consumer | How it derives |
|---|---|
| `config-contract.json`, its schema, `src/config/contract.generated.ts` | Generated |
| Native resolver, `config get`, `config list`, the settings page | Rust build reads the JSON |
| `.env` loading, including every skill's bundled `config-env.mjs` | Generated protected-key lists |
| The table in `docs/CONFIGURATION.md` | Generated |

[`tests/config-coverage.test.ts`](tests/config-coverage.test.ts) enforces this:
the code may not read an `OCTOCODE_*`, `*_API_KEY` or `*_BASE_URL` name that is
not declared, endpoints must be home-only, secrets must be marked, and the settings
page may not keep its own list of keys or providers.

To see the result on a machine: `npx octocode config list`.

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
[configuration reference](../../docs/CONFIGURATION.md#where-every-value-comes-from).

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
validation constraints are edited only in the Zod declaration
(`scripts/config-contract/declaration/`, shaped by `scripts/config-contract/schema.ts`).
`generate:config-contract` validates it with Zod and emits `config-contract.json` and
`config-contract.schema.json`; both are generated, guarded by a drift test and
`check:config-contract`, and never hand-edited. The generator also emits the
field metadata and defaults this package exports and the user settings
reference; the native build emits the resolver the runtime uses.

Tool wire types are generated, never hand-written:
`@octocodeai/octocode-native` embeds `contract/` (enforcement contract,
fixtures, and Rust types) directly at build time, and TypeScript consumers use
the core Zod schemas from `@octocodeai/config/schema`. Change the Zod schema in
`@octocodeai/octocode-core`, then run `node skills-dev/octocode-dev/scripts/dev.mjs contracts:regen` — that one step
updates every consumer. See [ARCHITECTURE.md](./ARCHITECTURE.md#tool-types).

See the repository [configuration reference](../../docs/CONFIGURATION.md),
[settings table](../../docs/CONFIGURATION.md#all-settings-and-defaults),
[contributor guide](../../skills-dev/octocode-dev/docs/ADDING_CONFIG.md), and
[security model](../../docs/SECURITY.md).

## License

MIT
