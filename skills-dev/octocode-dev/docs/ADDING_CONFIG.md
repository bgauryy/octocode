# Adding configuration to Octocode

How to add a setting or credential without TypeScript/Rust drift. User-facing settings: `<repo>/docs/CONFIGURATION.md`; credential flows: `<repo>/docs/AUTHENTICATION.md`. Paths below are under `packages/octocode-config/` unless they start with `crates/` (`packages/octocode-native/crates/`).

## Architecture

The Zod declaration in `scripts/config-contract/declaration/` (`sections.ts`, `environment.ts`), shaped by `scripts/config-contract/schema.ts`, is the only declaration of field policy: paths, types, defaults, one environment variable per setting, enums, URL and path semantics, unknown keys, dotenv trust, credential exclusion, descriptions. The generator validates it and emits `config-contract.json` and `config-contract.schema.json`. From that JSON:

- the TypeScript generator writes `src/config/contract.generated.ts` and the settings table between the markers in `<repo>/docs/CONFIGURATION.md`;
- `crates/runtime-base/build.rs` writes `$OUT_DIR/config_contract.rs` (structs, defaults, field metadata, env policy) by manifest-relative path, so a native-only build needs no TypeScript step; its renderer fails closed on missing or mistyped fields. The generic Rust resolver and validator are the only resolver.

Never edit generated files, add a setting to the Rust config structs, `resolver.rs` or `validation.rs`, or copy a default or range into docs. Set `displayDefault` on a setting whose `null` default means something. TypeScript surfaces never resolve settings: the native runtime does, and `octocode config` shows the result. The package's `.` entry uses only Node builtins; `src/home.ts`, `src/dotenv.ts` and `src/config` never import core (the `./schema` and `./mcp` subpaths re-export it).

## Sources

Highest first, per field: process environment or MCP client `env` block → workspace `.octocode/.env` → home `.env` → workspace `.octocode/.octocoderc` → home `.octocoderc` → generated default. Invalid, missing or blank values fall through. A workspace `.octocoderc` field whose environment binding is protected (`dotenv` other than `all`) is ignored with a `workspace_config_protected` warning, like the workspace `.env`. CLI and MCP load both `.env` files; Node helpers load the supplied workspace unless `trusted:false`. Native LSP-project trust is separate.

API keys that skill scripts read (every `type: 'key'` entry and credential setting, listed as `API_KEYS`) resolve through `getKey` instead: process environment → home `.env` → workspace `.octocode/.env`, so a repository only fills a key the user has not set and never sets a home-only one. A skill that calls a provider is a private workspace (`skills/<skill>/package.json`, devDependency `@octocodeai/config: workspace:*`) whose `src/keys.mjs` re-exports `getKey`; `yarn build` there runs `skills/common/bundle-skill.mjs`, which writes the bundle into the skill's `scripts/` folder as `keys.mjs`, and `yarn lint` fails when that bundle is stale. Its scripts import `getKey` from `./keys.mjs` and never read `process.env` for a key. After changing `src/keys.ts`, build the config package, then each such skill.

## What is a setting

Something a user chooses: a credential, endpoint, feature switch, tool list, scope, storage mode, model or presentation format. Internal limits (timeouts, retries, page sizes, TTLs, concurrency, spend and memory caps) are code constants, never settings; the contract has no number type. A shared limit lives in `RuntimeLimits::DEFAULT` (`crates/runtime-base/src/execution/limits.rs`; tests vary it through `ToolRuntime::with_limits`), any other next to its one use with a doc comment (for example `IDLE_EXIT` in `crates/cli/src/cli/serve.rs`). A limit that bounds evidence is disclosed in the response. Docs link to the constant instead of copying its value.

To retire a setting, delete its declaration, consumers and tests, then regenerate; unknown names get ordinary unknown-key handling, no tombstones. To rename one, rename it everywhere and regenerate; the old name is an unknown key, with no alias or legacy mechanism.

## Add a setting

One edit in `declaration/sections.ts`, then `yarn workspace @octocodeai/config generate:config-contract`. Hypothetical `output.compact`: in the `output` section's `fields`, add `"compact": { "type": "boolean", "default": false, "description": "Render compact text content.", "env": { "name": "OCTOCODE_OUTPUT_COMPACT" } }`. Generation produces the TypeScript type and default, env parsing and precedence, validation, unknown-key recognition, the Rust field, and the rows in `config list` and the settings table. Test only an effect the generic interpreter does not guarantee (a downstream feature gate), never parity that restates the declaration.

- Field types: `boolean`, `string`, `url` (HTTP(S)), `path` (absolute or home) take `default`; `stringArray` takes `default` and optional `itemFormat: "path"`; `enum` takes `values` (the first is the default) and optional `typeName`. A `null` input means unset; a `null` default makes an optional string or nullable array.
- A section: `"cache": { "title": "Cache", "fields": { } }` in `sections.ts`. Every section belongs to `.octocoderc` and `ResolvedConfig`, credential fields apart. Nested sections use dotted names (`parent.child`); declare the parents too.
- Native code reads `config.resolved.<section>.<field>` (for example `config.resolved.tools.enable_clone`) and `config.env_value("OCTOCODE_CLASSIFICATION_API")`.

### Environment binding

At most one `env` binding per setting, `{ "name": "OCTOCODE_..." }`. Every Octocode variable must be in the contract, or source labeling, protection, docs and the resolver cannot derive it. Variables other programs own (toolchain homes, `NPM_CONFIG_USERCONFIG`, `GIT_CONFIG_*`, `XDG_CONFIG_HOME`) are not settings; `PROCESS_ENV` in `crates/engine/src/process_env.rs` lists every one the engine reads so the warm server can key on them. An invalid environment value is skipped with a warning.

| Property | Effect |
|---|---|
| `dotenv` | `all` (default: workspace or home `.env`), `home` (the trusted home `.env` only), `never` (shell, CI or MCP environment only; joins the generated protected-key sets) |
| `normalize` | `"trim"`, or `"lower"` (trims and lowercases) |
| `workspaceNarrowsTo` | On a `dotenv: "home"` binding, the one value a workspace may still set (as `storage.mode: memory`) |

## Credentials

Credential values never appear in `ResolvedConfig`, logs, inspection output or diagnostics; consumers read them from `effective_env` in Rust or `process.env` in TypeScript.

**Environment only** (no `.octocoderc` fallback): `"environment": { "MY_SERVICE_API_KEY": { "dotenv": "all", "description": "My Service API credential" } }`. Use `never` only for a deliberate process-only policy, and never `all` for protected infrastructure (a trusted project's `.env` could override it). Read without copying into a loggable structure: `config.env_value("MY_SERVICE_API_KEY").filter(|v| !v.trim().is_empty())`, `process.env.MY_SERVICE_API_KEY?.trim() || undefined`.

GitHub tokens are such entries with `tokenPriority` (lower wins): `"GH_TOKEN": { "dotenv": "all", "tokenPriority": 0 }`, `"GITHUB_TOKEN": { …, "tokenPriority": 1 }`. The generator derives `ENV_TOKEN_VARS`, token-source types and protected-key sets from them; add no token list elsewhere. Source beats name (a workspace `GITHUB_TOKEN` beats a home `GH_TOKEN`); `tokenPriority` breaks ties within one source.

**Environment first, `.octocoderc` fallback**: mark the field `credential: true` (on `string`, `url` or `path`; without it the secret enters `ResolvedConfig`) and set its `dotenv` deliberately.

```json
"myService": { "title": "My Service", "fields": { "key": {
  "type": "string", "default": null, "credential": true,
  "description": "My Service API key fallback.",
  "env": { "name": "MY_SERVICE_API_KEY", "dotenv": "all", "normalize": "trim" } } } }
```

The generic Rust credential adapter applies the file fallback into `effective_env`; generated input types accept the field, resolved types exclude it. Reference: `classification.api` is the credential, while `classification.apiHost` (`dotenv: "home"`) and `classification.model` resolve normally. An endpoint is not a secret but receives one, so protect it with `dotenv: "home"`. Saving a `url` refuses userinfo, a query and a fragment; one already in a file makes its reader fail closed rather than send the credential to a default host.

Tests prove: process environment beats `.octocoderc`; `.octocoderc` fills an absent value and the workspace file beats the home one; workspace `.env` beats home; missing or blank file values fall back while the explicit process classification opt-out stays disabled; `Debug`, diagnostics, inspection JSON and `get_config_value` never contain the secret (pattern: `crates/runtime-base/src/config/layering_tests.rs`). After building CLI and MCP, check outgoing credential selection with synthetic keys and a loopback provider: the token-precedence item of the [packaged auth checks](../../../packages/octocode-native/docs/GITHUB_AUTH_TESTS.md#packaged-checks).

**Keys and endpoints a skill or script uses**: declare them in `declaration/environment.ts` with `type`, `displayDefault`, `group` and, for a key, `docsUrl`; mark a key `credential: true` and its endpoint `dotenv: 'home'`. `tests/config-coverage.test.ts` fails on an undeclared `OCTOCODE_*`, `*_API_KEY` or `*_BASE_URL` name and on an endpoint a workspace could set.

## Checks

```bash
yarn workspace @octocodeai/config generate:config-contract
yarn workspace @octocodeai/config check:config-contract
yarn workspace @octocodeai/config test
node skills-dev/octocode-dev/scripts/dev.mjs test:dev --crate runtime-base -- config::
```

A compile-only check misses CLI and MCP loading, redaction and interface wiring: exercise `octocode config --json` and one MCP session per setting.
