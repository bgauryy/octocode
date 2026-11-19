# Adding configuration to Octocode

How to add settings and credentials without TypeScript/Rust drift. User-facing settings: `<repo>/docs/CONFIGURATION.md`; credential flows: `<repo>/docs/AUTHENTICATION.md`; build commands: [DEVELOPMENT.md](DEVELOPMENT.md).

## Architecture

The Zod declaration in `packages/octocode-config/scripts/config-contract/declaration/` (`sections.ts`, `environment.ts`), shaped by `scripts/config-contract/schema.ts`, is the only declaration of field policy: paths and sections, input/resolved types, defaults, one environment variable per setting, enums, URL/path semantics, unknown keys, dotenv trust (`all`, `home`, `never`), credential exclusion from `ResolvedConfig`, descriptions. The generator validates it and emits `config-contract.json` and `config-contract.schema.json` (never hand-edited; the JSON examples below show the emitted form). Both build paths consume the JSON:

- TypeScript generator: `src/config/contract.generated.ts` (types, defaults, field metadata, env policy) and the settings table in `<repo>/docs/CONFIGURATION.md`.
- Rust `build.rs`: `$OUT_DIR/config_contract.rs` (structs, defaults, field metadata, env policy); the generic Rust resolver/validator is the only resolver.

- Never edit generated files; never add a setting directly to Rust config structs, `resolver.rs` or `validation.rs`; never copy a default or range into docs (the generated reference and table own them). Set `displayDefault` on a setting whose `null` default means something.
- `@octocodeai/config` owns this policy; its `.` entry uses only Node builtins. TypeScript surfaces never resolve settings: the native runtime does, and `octocode config` shows the result. `src/home.ts`, `src/dotenv.ts` and `src/config` never import core (the `./schema` and `./mcp` subpaths re-export it; see the [contract pipeline](DEVELOPMENT.md#contract-pipeline)).

## Source precedence and file policy

Highest priority first: process environment / MCP client env block → workspace `.octocode/.env` → home `.octocode/.env` → workspace `.octocode/.octocoderc` → home `.octocode/.octocoderc` → generated default.

- Resolution is per field: the first valid value wins; invalid, missing or blank values fall through.
- A workspace `.octocoderc` field whose environment binding is protected (`dotenv` other than `all`) is ignored with a `workspace_config_protected` warning, like the workspace `.env`. Details: `<repo>/docs/CONFIGURATION.md#where-every-value-comes-from`.
- CLI and MCP load both `.env` files. Node helpers load the supplied workspace by default; `trusted:false` opts out. Native executable LSP-project trust is separate.

| Dotenv policy | Meaning |
|---|---|
| omitted or `all` | Loads from workspace or global Octocode `.env`. |
| `home` | May load from the trusted home `.env`, never a project `.env`. |
| `never` | Shell/CI/MCP environment only; never loaded from either `.env`. |

## What is a setting

A setting is something a user chooses: a credential, endpoint, feature switch, tool list, scope, storage mode, model or presentation format. Internal limits (timeouts, retries, waits, page sizes, retained-work sizes and TTLs, concurrency, spend and memory caps) are code constants, never settings; the contract has no number field type. A limit lives in:

- **`RuntimeLimits::DEFAULT`** (`packages/octocode-native/crates/runtime-base/src/execution/limits.rs`) when several surfaces share it or tests vary it through `ToolRuntime::with_limits`;
- **a named constant next to its single use** otherwise (for example `IDLE_EXIT` in `crates/cli/src/cli/serve.rs`), with a doc comment giving the rationale. When it bounds evidence, the response discloses the cut (a warning, a `*Clipped` flag or a continuation), never a silent gap.

Docs link to the constant instead of copying its value. To retire a setting, delete its declaration, consumers and tests, then regenerate; unknown names use ordinary unknown-key handling. No compatibility aliases, tombstones, special diagnostics or second policy registry.

## Add a normal setting

One edit in `scripts/config-contract/declaration/sections.ts` plus regeneration. Hypothetical example: `output.compact`, env `OCTOCODE_OUTPUT_COMPACT`, default `false`: in the `output` section's `fields`, add `"compact": { "type": "boolean", "default": false, "description": "Render compact text content.", "env": { "name": "OCTOCODE_OUTPUT_COMPACT" } }`.

Enum defaults are the first value in `values`; runtime-surface defaults are the first surface. `generate:config-contract` produces the TypeScript type, default, env precedence and parsing, validation, unknown-key recognition, the Rust field with resolution metadata, and the rows in `config list` and `<repo>/docs/CONFIGURATION.md`. Test only the setting's effect the generic interpreter does not guarantee (for example a downstream feature gate); no language-parity tests that restate the declaration.

### Supported field shapes

`boolean`, `string`, `url` (HTTP(S)), `path` (absolute/home) take `default`; `stringArray` takes `default` and optional `itemFormat: "path"`; `enum` takes `values` and optional `typeName`. A `null` input means "unset; use the next source"; a `null` default becomes an optional string or nullable array.

### Environment binding

Each setting has at most one `env` binding, `{ "name": "OCTOCODE_..." }`, no aliases. Every Octocode variable must be in the contract, or source labeling, protection, docs and the resolver cannot derive it. Variables other programs own (toolchain homes, `NPM_CONFIG_USERCONFIG`, `GIT_CONFIG_*`, `XDG_CONFIG_HOME`) are not settings: Octocode reads them as their owner does, and `engine/src/process_env.rs` (`PROCESS_ENV`) lists every one the engine reads so the warm server can key on them. An invalid environment value is skipped with a warning and the next source applies.

| Property | Effect |
|---|---|
| `dotenv` | `all` (default), `home` or `never`. |
| `normalize: "trim"` / `"lower"` | Trims a string / trims and lowercases. |
| `workspaceNarrowsTo: "<value>"` | On a `dotenv: "home"` binding: a workspace `.env` or `.octocoderc` may still set this one narrowing value (as `storage.mode: memory`). |

### Add a new section

Declare the section and its fields in `declaration/sections.ts`: `"cache": { "title": "Cache", "fields": { } }`. Every section belongs to `.octocoderc` and the generated `ResolvedConfig`, apart from credential fields. Nested sections use dotted names (`parent.child`); declare parents too, even with empty `fields`. `typeName` keeps a public type name when automatic PascalCase is unsuitable.

## Add credentials and protected environment keys

Credential values never appear in `ResolvedConfig`, logs, inspection output or generated diagnostics. Consumers read them from `effective_env` in Rust or `process.env` in TypeScript. Pattern A when no `.octocoderc` value is a fallback; Pattern B when a trusted `.octocoderc` supplies one (environment preferred).

### Pattern A: environment entry only

```json
"environment": { "MY_SERVICE_API_KEY": { "dotenv": "all", "description": "My Service API credential" } }
```

`never` is for a deliberate process-only policy (bootstrap settings); it adds the name to the generated protected-key sets. Do not use `all` for protected infrastructure: a trusted project's `.env` can override it. Read without copying into a loggable structure: `config.env_value("MY_SERVICE_API_KEY").filter(|v| !v.trim().is_empty())` in Rust, `process.env.MY_SERVICE_API_KEY?.trim() || undefined` in TypeScript.

**GitHub token priority.** GitHub tokens are Pattern A entries with `tokenPriority` (lower wins), e.g. `"GH_TOKEN": { "dotenv": "all", "tokenPriority": 0 }`, `"GITHUB_TOKEN": { ..., "tokenPriority": 1 }`. The generator derives `ENV_TOKEN_VARS`, token-source types and protected-key sets from them; add no token array elsewhere. Source beats name order (process → workspace `.env` → global `.env`), so a workspace `GITHUB_TOKEN` beats a global `GH_TOKEN`; `tokenPriority` breaks ties within one source.

### Pattern B: environment preferred, `.octocoderc` fallback

- Mark each secret field `credential: true` (on `string`, `url` or `path`); without it the secret enters `ResolvedConfig` and inspection can expose it.
- Set its `dotenv` policy deliberately; never rely on an undocumented trust assumption. A section may mix credential and ordinary fields.

```json
"myService": { "title": "My Service", "fields": { "key": {
  "type": "string", "default": null, "credential": true,
  "description": "My Service API key fallback.",
  "env": { "name": "MY_SERVICE_API_KEY", "dotenv": "all", "normalize": "trim" } } } }
```

- The generic Rust credential adapter applies environment-first file fallback into `effective_env`. Generated input types and validators recognize credential fields in `.octocoderc`; generated resolved types exclude them.
- Reference: `classification.api` is the credential; `classification.apiHost` (`dotenv: "home"`) and `classification.model` resolve normally. An endpoint is not a secret: keep it readable, protected with `dotenv: "home"`. Saving a `url` setting refuses userinfo, a query and a fragment; one already in a file makes its reader fail closed instead of sending the credential to a default host.

Pattern B tests prove: (1) process environment wins over `.octocoderc`; (2) `.octocoderc` fills an absent value and a workspace `.octocoderc` beats the home one; (3) workspace `.env` wins over global; (4) missing/blank file values fall back, while the explicit process classification opt-out stays disabled; (5) `Debug`, diagnostics, inspection JSON and `get_config_value` do not contain the secret. Test the applicable process env, home/project `.env` and workspace/home `.octocoderc` precedence (see `packages/octocode-native/crates/runtime-base/src/config/layering_tests.rs`). After building CLI/MCP, check outgoing credential selection by hand with synthetic keys and a loopback provider: the token precedence item of the [packaged auth checks](../../../packages/octocode-native/docs/GITHUB_AUTH_TESTS.md#packaged-checks).

### API keys and endpoints a skill or script uses

Declare them in `declaration/environment.ts` with `type`, `displayDefault`, `group` and, for a key, `docsUrl`; the settings page and `config list` offer them from there. Mark a key `credential: true` and its endpoint `dotenv: 'home'` (an endpoint receives its key, so a repository's `.env` must never choose one). `tests/config-coverage.test.ts` fails on an undeclared `OCTOCODE_*`, `*_API_KEY` or `*_BASE_URL` name and on an endpoint a workspace could set.

## Read resolved configuration

Native code reads `config.resolved.<section>.<field>` (for example `config.resolved.tools.enable_clone`) and `config.env_value("OCTOCODE_CLASSIFICATION_API")`. The resolved structs are generated at build time: `crates/runtime-base/build.rs` reads `config-contract.json` by manifest-relative path, so native-only builds need no prior TypeScript generation, and its renderer fails closed on missing or mistyped fields (the TypeScript generator validates with Zod).

## Generated artifacts and checks

Source: `packages/octocode-config/scripts/config-contract/declaration/` (policy) and `schema.ts` (Zod shape). Generated: `packages/octocode-config/config-contract.json`, `config-contract.schema.json`, `src/config/contract.generated.ts`, the settings table between the markers in `<repo>/docs/CONFIGURATION.md`, and Rust `$OUT_DIR/config_contract.rs` (build output, never commit).

```bash
yarn workspace @octocodeai/config generate:config-contract
yarn workspace @octocodeai/config check:config-contract
yarn workspace @octocodeai/config lint
yarn workspace @octocodeai/config test
yarn workspace @octocodeai/config build

cargo check --manifest-path packages/octocode-native/crates/runtime/Cargo.toml
cargo test --manifest-path packages/octocode-native/crates/runtime/Cargo.toml --lib config::
```

A compile-only test misses CLI/MCP loading, redaction and interface wiring.
