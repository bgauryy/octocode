# Adding configuration to Octocode

How to add settings and credentials without TypeScript/Rust drift. User-facing settings: `<repo>/docs/CONFIGURATION.md`. Credential flows: `<repo>/docs/AUTHENTICATION.md`. Package layout and build commands: [DEVELOPMENT.md](DEVELOPMENT.md).

## Architecture

`packages/octocode-config/config-contract.json` is the only declaration of configuration field policy. It owns file paths and section membership; input and resolved types; defaults and inherited defaults; environment names and alias priority; ranges, enum values, URL/path semantics, and unknown-key membership; dotenv trust (`all`, `home`, or `never`); credential exclusion from `ResolvedConfig`; and user-facing descriptions and generated reference data.

The TypeScript generator validates the contract against `config-contract.schema.json` (Ajv). Both build paths then consume it:

```text
packages/octocode-config/config-contract.json
                 │
                 ├─ TypeScript generator
                 │    ├─ src/config/contract.generated.ts
                 │    │    types · defaults · field metadata · env policy
                 │    └─ docs/generated/CONFIG_SETTINGS.md
                 │
                 └─ Rust build.rs
                      └─ $OUT_DIR/config_contract.rs
                           structs · defaults · field metadata · env policy

contract metadata → generic TypeScript resolver/validator
contract metadata → generic Rust resolver/validator
```

- The interpreters contain language mechanics only (reading JavaScript objects or `serde_json::Value`, parsing environment strings, building diagnostics). They contain no per-setting field lists.
- Do not edit generated files; the next generation or build discards the edit.
- Do not add a setting directly to `types.ts`, `defaults.ts`, `resolverSections.ts`, `validator.ts`, Rust config structs, `resolver.rs`, or `validation.rs`. Field-specific logic in one resolver brings back language drift.
- Do not copy a default or range into docs. The generated settings reference owns those facts.
- `@octocodeai/config` owns this policy. Its `.` entry (the config loader) uses only Node builtins; Ajv is a build/test dependency.
- The `./schema` and `./mcp` subpaths re-export `@octocodeai/octocode-core`, and the tool-contract generator reads core (see the [contract pipeline](DEVELOPMENT.md#contract-pipeline)). Configuration code under `src/config` and `src/tokens` never imports core: core owns tool contracts, config owns configuration and environment policy.

## Source precedence and file policy

For ordinary settings, highest priority wins:

```text
process environment / MCP client env block
  → workspace .octocode/.env
  → home .octocode/.env
  → workspace .octocode/.octocoderc
  → home .octocode/.octocoderc
  → generated default
```

- Resolution is per field. The first source with a valid value wins; an invalid value falls through to the next layer.
- A workspace `.octocoderc` field whose environment binding is protected (`dotenv` other than `all`) is ignored with a `workspace_config_protected` warning, the same boundary as the workspace `.env`. Details: `<repo>/docs/CONFIGURATION.md#how-settings-override-each-other`.
- CLI and MCP load both `.env` files. Node helpers load the supplied workspace by default; an explicit `trusted:false` opts out. Native executable LSP-project trust is separate.
- Missing or blank file values fall back to the next source.

| Dotenv policy | Meaning |
|---|---|
| omitted or `all` | Loads from workspace or global Octocode `.env`. |
| `home` | May load from the trusted home `.env`, never a project `.env`. |
| `never` | Shell/CI/MCP environment only; never loaded from either `.env` file. |

## Add a normal setting

In an existing section, a normal setting needs one edit in `config-contract.json` and a regeneration.

Example (hypothetical; not a shipped field): `output.maxResults`, with `OCTOCODE_MAX_RESULTS`, range 1–500, and default 50:

```json
{
  "sections": {
    "output": {
      "title": "Output",
      "file": true,
      "resolved": true,
      "fields": {
        "maxResults": {
          "type": "number",
          "minimum": 1,
          "span": 499,
          "defaultOffset": 49,
          "description": "Maximum results emitted in one response.",
          "env": {
            "OCTOCODE_MAX_RESULTS": { "priority": 0 }
          }
        }
      }
    }
  }
}
```

- Numeric maximum is `minimum + span`; default is `minimum + min(defaultOffset, span)`. Inverted ranges are impossible by construction.
- Enum defaults are the first value in `values`. Runtime-surface defaults are the first surface.

```bash
yarn workspace @octocodeai/config generate:config-contract
```

That one declaration generates `OutputConfigOptions.maxResults?: number`, `RequiredOutputConfig.maxResults: number`, the resolved default, environment precedence and integer parsing, clamping and validation bounds, unknown-key recognition, Rust `OutputConfig.max_results`, Rust resolution and validation metadata, and the settings-reference row and complete example.

Add a focused test only for behavior the generic interpreter does not guarantee, for example a downstream feature gate that consumes the value. Do not add language-parity tests that restate the field declaration.

### Supported field shapes

| `type` | Resolved representation | Relevant properties |
|---|---|---|
| `boolean` | boolean / Rust `bool` | `default` |
| `number` | number / Rust `f64` | `minimum`, `span`, `defaultOffset` |
| `string` | string or optional string | `default` |
| `url` | HTTP(S) URL string | `default` |
| `path` | absolute/home path string | `default` |
| `stringArray` | string array or nullable array | `default`, optional `itemFormat: "path"` |
| `enum` | generated literal union / Rust string | `values`, optional `defaultFrom` |
| `schemaVersion` | schema version | reserved for the root version field |

A `null` input means “unset; use the next source.” A `null` generated default becomes an optional string or nullable array where appropriate.

### Environment aliases and invalid input

Bindings are keyed by environment variable and sorted by `priority`; lower numbers win. Every environment binding must be in the contract; otherwise source labeling, protection, docs, and both resolvers cannot derive it.

```json
"env": {
  "ENABLE_LOCAL": { "priority": 0 },
  "OCTOCODE_ENABLE_LOCAL": { "priority": 1 }
}
```

| Optional property | Effect |
|---|---|
| `normalize: "trim"` | Trims a string. |
| `normalize: "lower"` | Trims and lowercases. |
| `invalid: "skip"` (default) | Ignores an invalid environment value and tries the next source. |
| `invalid: "default"` | An invalid environment value selects the generated default, not file config. Use only when invalid environment input is intentionally authoritative (as with output format). |

### Inherited defaults

Use `defaultFrom`; do not copy another default:

```json
"extension.storage.mode": {
  "type": "enum",
  "values": ["persistent", "memory"],
  "defaultFrom": "storage.mode"
}
```

Both generators reject unresolved or cyclic inheritance. At runtime inheritance uses the already-resolved source field, so an environment or file override of `storage.mode` flows into extension storage.

### Add a new section

Declare the section and its fields in the same contract. `file` controls whether `.octocoderc` accepts it; `resolved` controls whether it appears in generated `ResolvedConfig` types.

```json
"cache": {
  "title": "Cache",
  "file": true,
  "resolved": true,
  "fields": { }
}
```

- Nested sections use dotted names such as `output.pagination`. Declare parent sections too, even with an empty `fields` object.
- `typeName` keeps a public TypeScript name when automatic PascalCase is unsuitable; `rustTypeName` does the same for Rust structs. These are compatibility metadata, not field policy.

## Add credentials and protected environment keys

Credential values never appear in `ResolvedConfig`, logs, inspection output, or generated diagnostics. Consumers read them from `effective_env` in Rust or `process.env` in TypeScript.

### Pattern A: credential without a `.octocoderc` field

Declare an environment entry, not a resolved config field:

```json
"environment": {
  "MY_SERVICE_API_KEY": {
    "dotenv": "all",
    "description": "My Service API credential"
  }
}
```

- `dotenv: "all"` accepts process, workspace, and global values through the shared loader.
- Use `never` only for a deliberate process-only policy, such as bootstrap settings; it adds the name to the generated protected-key sets. Do not use `all` for protected infrastructure: a trusted project's `.env` can override it.

Read it without copying it into a loggable structure:

```rust
let key = config.env_value("MY_SERVICE_API_KEY")
    .filter(|value| !value.trim().is_empty());
```

```ts
const key = process.env.MY_SERVICE_API_KEY?.trim() || undefined;
```

Add `configSource: true` only if the variable changes resolved configuration source labeling. Authentication-only variables usually do not.

### GitHub token priority

GitHub tokens are Pattern A entries with `tokenPriority`; lower numbers win. The generator derives `ENV_TOKEN_VARS`, token-source types, and protected-key sets from these declarations. Do not add a token array elsewhere.

```json
"OCTOCODE_TOKEN": { "dotenv": "all", "tokenPriority": 0 },
"GH_TOKEN": { "dotenv": "all", "tokenPriority": 1 }
```

Source beats alias order: process → workspace `.octocode/.env` → global `.env`. A workspace alias beats a global canonical key. Declared alias order breaks ties within one source.

### Pattern B: environment preferred, trusted `.octocoderc` fallback

- Mark each secret field `credential: true` (allowed on `string`, `url`, and `path`). Without it, the secret enters `ResolvedConfig`, and inspection or point lookup can expose it.
- Choose its binding policy: `all` for both trusted `.env` files, `home` for home only, `never` for process environment only.
- Generators exclude credential fields from `ResolvedConfig` even inside a `resolved: true` section, so a section may mix credentials with ordinary settings. An all-credential section may be `resolved: false`.

```json
"myService": {
  "title": "My Service",
  "file": true,
  "resolved": false,
  "fields": {
    "key": {
      "type": "string",
      "default": null,
      "credential": true,
      "description": "My Service API key fallback.",
      "env": {
        "MY_SERVICE_API_KEY": {
          "priority": 0,
          "dotenv": "all",
          "normalize": "trim"
        }
      }
    }
  }
}
```

- The generic Rust credential adapter applies environment-first file fallback into `effective_env`.
- The generated input type and generic validators recognize credential fields in `.octocoderc`; generated resolved types exclude them.
- An existing credential alias from either file prevents the canonical `.octocoderc` fallback from replacing it.
- Reference: the `classification` section is `resolved: true`. `classification.api` (`dotenv: "all"`) and `classification.apiHost` (`dotenv: "home"`) are credential fields and never enter `ResolvedConfig`; `classification.type` and `classification.maxConcurrency` resolve normally.

Pattern B tests must prove:

1. process environment wins over `.octocoderc`;
2. `.octocoderc` fills an absent value, and a workspace `.octocoderc` beats the home one;
3. workspace `.env` wins over global, including across different aliases;
4. missing/blank file values fall back, while the explicit process classification opt-out stays disabled;
5. `Debug`, inspection JSON, and `get_config_value` do not contain the secret.

After you build CLI/MCP, run `yarn workspace @octocodeai/config test:tokens:acceptance` to verify outgoing credential selection with synthetic keys and a loopback provider.

## Read resolved configuration

```ts
import { loadOctocodercLayers, resolveConfigFields } from '@octocodeai/config';

// Workspace .octocoderc first, then global; environment wins per field.
const config = resolveConfigFields(loadOctocodercLayers(), process.env);
const timeout = config.network.timeout;
const format = config.output.format;
```

The section-specific `resolveGitHub`, `resolveOutput`, and similar exports are compatibility adapters. They delegate to the generic contract interpreter; do not put field logic in them.

```rust
let timeout_ms = config.resolved.network.timeout as u64;
let format = &config.resolved.output.format;
let classification_key = config.env_value("OCTOCODE_CLASSIFICATION_API");
```

Rust resolved structs are generated at build time. `crates/runtime/build.rs` reads `config-contract.json` through a manifest-relative path, so native-only builds do not depend on a prior TypeScript generation or the working directory. It does not re-validate the meta-schema (the TypeScript generator does); its renderer fails closed on missing or mistyped fields.

## Generated artifacts and checks

| Kind | Files |
|---|---|
| Source | `packages/octocode-config/config-contract.json` (policy); `packages/octocode-config/config-contract.schema.json` (meta-schema) |
| Generated | `packages/octocode-config/src/config/contract.generated.ts`; `<repo>/docs/generated/CONFIG_SETTINGS.md`; Rust `$OUT_DIR/config_contract.rs` (build output, never commit it) |

```bash
yarn workspace @octocodeai/config generate:config-contract
yarn workspace @octocodeai/config check:config-contract
yarn workspace @octocodeai/config lint
yarn workspace @octocodeai/config test
yarn workspace @octocodeai/config build

cargo check --manifest-path packages/octocode-native/crates/runtime/Cargo.toml
cargo test --manifest-path packages/octocode-native/crates/runtime/Cargo.toml --lib config::
```

After native/runtime changes, rebuild native and the consuming CLI or MCP interface ([build commands](DEVELOPMENT.md#build-test-lint)), then exercise the real CLI path. A compile-only test misses CLI/MCP loading, redaction, and interface wiring.

## Contributor checklist

### Normal setting

- [ ] Add the field once in `config-contract.json`, with a contract type, default, environment binding, constraint, description, and trust policy.
- [ ] Regenerate TypeScript and documentation.
- [ ] Add a consumer test for the setting's effect.
- [ ] Run config lint/tests/build and native config tests.
- [ ] Exercise the real CLI/MCP path when runtime behavior changes.

### Credential

- [ ] Choose Pattern A (environment-only) or Pattern B (trusted file fallback).
- [ ] Set `dotenv` deliberately; never rely on an undocumented trust assumption.
- [ ] For Pattern B, set `credential: true` on each secret field.
- [ ] Verify no secret reaches `ResolvedConfig`, diagnostics, `Debug`, or inspection output.
- [ ] Test process environment, home `.env`, project `.env`, and workspace/home `.octocoderc` precedence as applicable (see `packages/octocode-native/crates/runtime/src/config/layering_tests.rs`).
