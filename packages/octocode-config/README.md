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
- `loadOctocoderc(home?)` reads the structured Octocode configuration.
- `PROTECTED_KEYS` identifies values that project configuration cannot replace.

Do not reimplement these rules in a consuming package.

## CLI

The package also exposes `octocode-config`:

```bash
npx @octocodeai/config --keys
npx @octocodeai/config --check OCTOCODE_HOME
```

## Development

From the repository root:

```bash
yarn workspace @octocodeai/config generate:shared-constants
yarn workspace @octocodeai/config build
yarn workspace @octocodeai/config test
yarn workspace @octocodeai/config lint
```

Shared TypeScript/Rust values are edited only in `shared-constants.json`.
The generator validates that file with Zod and emits the TypeScript literal
exports; Rust consumes the same JSON from its build script.

See the repository [configuration reference](../../docs/CONFIGURATION.md) and
[security model](../../docs/SECURITY.md).

## License

MIT
