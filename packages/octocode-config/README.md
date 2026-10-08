# Octocode config

`@octocodeai/config` is the zero-runtime-dependency owner of Octocode
home-directory resolution, `.env` policy, and the configuration contract
(`config-contract.json`), shared by the Octocode CLI launcher, MCP server,
extensions, and standalone skills. Settings are resolved by the native runtime
from the same contract; `octocode config` shows what it resolved.

## Public API

- `getOctocodeHome(env?)` resolves `OCTOCODE_HOME` or the platform default; `getConfigFilePath(home?)` and `getProjectConfigFilePath(cwd?)` name the global and workspace `.octocoderc`.
- `loadOctocodeEnv(options)` loads the home and workspace `.env` files; `applyOctocodeEnv(map, options)` applies them under the protected-key policy; `propagateOctocodeEnv(options)` does both.
- `CONFIG_FIELDS` (every setting's metadata), `DEFAULT_CONFIG` (generated defaults), `configFieldEnvNames(path)` (a setting's env names), and `ENV_TOKEN_VARS` (GitHub token names, priority order) come from the contract.
- `contractDriftAllowed(env, { bundled })` and `contractDriftMessage(core, native)` are the shared fail-closed core/native fingerprint gate (`devOverridesAllowed` covers every dev-only override); `INTERACTIVE_EXECUTION_TIMEOUT_SECS` is the CLI/MCP per-request budget; `RuntimeSurface` names the runtime surfaces.

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
file against `config-contract.schema.json`. The TypeScript generator emits the
field metadata and defaults this package exports and the user settings
reference; the native build emits the resolver the runtime uses.

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
