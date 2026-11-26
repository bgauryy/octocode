Load when adding a credential, a GitHub token source, or a key or endpoint a skill or script reads.

# Configuration credentials

Paths are under `packages/octocode-config/` unless they start with `crates/` (`packages/octocode-native/crates/`), as in [ADDING_CONFIG.md](ADDING_CONFIG.md).

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

## API keys in skill scripts

API keys that skill scripts read (every `type: 'key'` entry and credential setting, listed as `API_KEYS`) resolve through `getKey` instead: process environment → home `.env` → workspace `.octocode/.env`, so a repository only fills a key the user has not set and never sets a home-only one. A skill that calls a provider is a private workspace (`skills/<skill>/package.json`, devDependency `@octocodeai/config: workspace:*`) whose `src/keys.mjs` re-exports `getKey`; `yarn build` there runs `skills/common/bundle-skill.mjs`, which writes the bundle into the skill's `scripts/` folder as `keys.mjs`, and `yarn lint` fails when that bundle is stale. Its scripts import `getKey` from `./keys.mjs` and never read `process.env` for a key. After changing `src/keys.ts`, build the config package, then each such skill.
